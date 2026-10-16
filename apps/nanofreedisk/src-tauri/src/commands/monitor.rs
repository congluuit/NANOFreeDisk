//! Monitor view commands (docs/PHASE6_MONITOR.md §2.5): disk activity per process
//! and per file. Read-only — nothing here ends, suspends or throttles a process;
//! the view offers End task through the Tasks tab's guarded dialog
//! (`hwmon::task_verdict` / `hwmon::task_end`).
//!
//! A background thread samples once a second while the view is open (it polls
//! [`monitor_snapshot`]) or log-spam alerts are on, and never while paused. The
//! file-level detail is a kernel trace session: in this process when the app runs
//! elevated, otherwise in the elevated helper shared with Advanced sensors. It is
//! stopped whenever sampling stops and started again when sampling resumes.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use nfd_monitor::{Alert, Minute, Monitor, Point, Snapshot, SpamConfig, TopProc, TouchRow};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use super::{AppState, CmdResult, err};

/// The view counts as open this long after its last poll.
const VIEW_TTL: Duration = Duration::from_secs(5);
const MAX_ALERTS: usize = 50;
const MAX_TOUCHES: usize = 2000;

/// Notification texts in the UI language; the backend fills `{name}`, `{rate}`,
/// `{minutes}`, `{path}` and `{size}`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorTexts {
    spam_title: String,
    spam_body: String,
    log_title: String,
    log_body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StampedAlert {
    /// Unix ms.
    at: u64,
    #[serde(flatten)]
    alert: Alert,
}

#[derive(Default)]
struct Inner {
    #[cfg(windows)]
    sampler: Option<nfd_monitor::win::Sampler>,
    monitor: Monitor,
    #[cfg(windows)]
    local: Option<nfd_monitor::win::etw::EtwSession>,
    #[cfg(windows)]
    remote: Option<nfd_helper::RemoteMonitor>,
    silent: nfd_monitor::SilentCheck,
    last: Option<Snapshot>,
    paused: bool,
    view_seen: Option<Instant>,
    /// The user turned file-level detail on (it is restarted after a pause).
    detail_wanted: bool,
    /// Detail runs in the helper (vs. this process).
    detail_remote: bool,
    detail_error: Option<String>,
    watch: Option<String>,
    texts: MonitorTexts,
    record: bool,
    alerts: VecDeque<StampedAlert>,
    touches: VecDeque<TouchRow>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("paused", &self.paused)
            .finish_non_exhaustive()
    }
}

impl Inner {
    fn active(&self) -> bool {
        !self.paused
            && (self.monitor.alerts().enabled
                || self.view_seen.is_some_and(|t| t.elapsed() < VIEW_TTL))
    }

    #[cfg(windows)]
    fn detail_running(&self) -> bool {
        self.local.is_some() || self.remote.is_some()
    }

    #[cfg(not(windows))]
    fn detail_running(&self) -> bool {
        false
    }

    #[cfg(windows)]
    fn stop_detail(&mut self) {
        self.local = None;
        if let Some(mut r) = self.remote.take() {
            r.stop();
        }
    }

    #[cfg(not(windows))]
    fn stop_detail(&mut self) {}
}

/// Monitor state, managed next to `AppState`.
#[derive(Debug, Default)]
pub struct MonitorState {
    inner: Arc<Mutex<Inner>>,
    thread: OnceLock<()>,
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn elevated() -> bool {
    #[cfg(windows)]
    {
        nfd_monitor::win::is_elevated()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Start (or restart) the file-level session for the wanted state. Blocking; may
/// show UAC when the helper is not running yet (only from [`monitor_detail`]).
#[cfg(windows)]
fn start_detail(app: &AppHandle, inner: &Arc<Mutex<Inner>>, allow_uac: bool) -> CmdResult<()> {
    let watch = {
        let mut g = inner.lock().map_err(|e| err("internal", e))?;
        g.stop_detail();
        g.watch.clone()
    };
    if elevated() {
        let s = nfd_monitor::win::etw::EtwSession::start(watch.as_deref())
            .map_err(|e| err("etw", format!("{e:?}")))?;
        let mut g = inner.lock().map_err(|e| err("internal", e))?;
        g.local = Some(s);
        g.detail_remote = false;
    } else {
        let hw = app.state::<super::hwmon::HwState>();
        let slot = super::hwmon::helper_slot(&hw);
        let conn = {
            let guard = slot.lock().map_err(|e| err("internal", e))?;
            guard.as_ref().filter(|c| c.connected()).cloned()
        };
        let conn = match conn {
            Some(c) => c,
            None if allow_uac => super::hwmon::helper_connection(&slot)?,
            None => return Err(err("helper-stopped", "helper not running")),
        };
        let mut remote = nfd_helper::RemoteMonitor::new(conn);
        remote.start(watch).map_err(|e| err("etw", e))?;
        let mut g = inner.lock().map_err(|e| err("internal", e))?;
        g.remote = Some(remote);
        g.detail_remote = true;
    }
    let mut g = inner.lock().map_err(|e| err("internal", e))?;
    g.detail_error = None;
    g.silent = nfd_monitor::SilentCheck::default();
    Ok(())
}

fn fill(s: &str, pairs: &[(&str, String)]) -> String {
    let mut out = s.to_owned();
    for (k, v) in pairs {
        out = out.replace(k, v);
    }
    out
}

fn notify(app: &AppHandle, texts: &MonitorTexts, a: &Alert) {
    use tauri_plugin_notification::NotificationExt;
    let (title, body) = match a {
        Alert::LogSpam {
            name,
            rate,
            minutes,
            ..
        } => {
            let p = [
                ("{name}", name.clone()),
                ("{rate}", format!("{:.1}", rate / nfd_monitor::spam::MB)),
                ("{minutes}", minutes.to_string()),
            ];
            (fill(&texts.spam_title, &p), fill(&texts.spam_body, &p))
        }
        Alert::BigLog {
            name, path, size, ..
        } => {
            let p = [
                ("{name}", name.clone()),
                ("{path}", path.clone()),
                (
                    "{size}",
                    format!("{:.1}", *size as f64 / f64::from(1u32 << 30)),
                ),
            ];
            (fill(&texts.log_title, &p), fill(&texts.log_body, &p))
        }
    };
    if !title.is_empty() {
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

/// Save completed minutes to the history database (when the user keeps history).
fn save_minutes(app: &AppHandle, minutes: &[Minute]) {
    if minutes.is_empty() {
        return;
    }
    let rows: Vec<nfd_history::monitor::MonitorMinute> = minutes
        .iter()
        .map(|m| nfd_history::monitor::MonitorMinute {
            ts: m.ts,
            read: m.read,
            write: m.write,
            peak_read: m.peak_read,
            peak_write: m.peak_write,
            seconds: m.seconds,
            top: serde_json::to_string(&m.top).unwrap_or_else(|_| "[]".into()),
        })
        .collect();
    let state = app.state::<AppState>();
    let now = unix_ms() / 1000;
    let _ = super::with_history(app, &state, |h| h.monitor_save(&rows, now));
}

/// One sampling round (called by the thread once a second).
#[cfg(windows)]
fn round(app: &AppHandle, inner: &Arc<Mutex<Inner>>) {
    use nfd_monitor::Source;
    let restart = {
        let Ok(mut g) = inner.lock() else { return };
        if !g.active() {
            // nothing samples: no trace session either (restarted on resume)
            if g.detail_running() {
                g.stop_detail();
            }
            if g.view_seen
                .is_none_or(|t| t.elapsed() > Duration::from_secs(60))
                && !g.monitor.alerts().enabled
            {
                g.sampler = None;
            }
            return;
        }
        g.detail_wanted && !g.detail_running() && g.detail_error.is_none()
    };
    if restart && start_detail(app, inner, false).is_err() {
        if let Ok(mut g) = inner.lock() {
            g.detail_error = Some("restart".into());
        }
    }
    let Ok(mut g) = inner.lock() else { return };
    let g = &mut *g;
    let mut external = None;
    if let Some(s) = g.local.as_mut() {
        if s.alive() {
            external = Some((Source::Etw, s.take()));
        } else {
            g.local = None;
            g.detail_error = Some("session-stopped".into());
        }
    } else if let Some(r) = g.remote.as_mut() {
        match r.poll() {
            Ok(t) => external = Some((Source::Helper, t)),
            Err(e) => {
                g.remote = None;
                g.detail_error = Some(e);
            }
        }
    }
    let sampler = g.sampler.get_or_insert_with(nfd_monitor::win::Sampler::new);
    let had_detail = external.is_some();
    let s = sampler.sample(external);
    if had_detail && g.silent.observe(&s.tick, &s.disks) {
        g.stop_detail();
        g.detail_error = Some("etw-silent".into());
    }
    let size = |p: &str| std::fs::metadata(p).ok().map(|m| m.len());
    let now = unix_ms();
    let snap = g
        .monitor
        .ingest(now, s.source, &s.tick, &s.procs, s.disks, &size);
    for a in &snap.alerts {
        notify(app, &g.texts, a);
        let _ = app.emit("monitor-alert", a);
        g.alerts.push_front(StampedAlert {
            at: now,
            alert: a.clone(),
        });
    }
    g.alerts.truncate(MAX_ALERTS);
    for t in &snap.touches {
        g.touches.push_front(t.clone());
    }
    g.touches.truncate(MAX_TOUCHES);
    let minutes = g.monitor.take_minutes();
    let record = g.record;
    g.last = Some(snap);
    if record && !minutes.is_empty() {
        let app = app.clone();
        std::thread::spawn(move || save_minutes(&app, &minutes));
    }
}

#[cfg(not(windows))]
fn round(_app: &AppHandle, _inner: &Arc<Mutex<Inner>>) {}

fn ensure_thread(app: &AppHandle, state: &MonitorState) {
    let inner = state.inner.clone();
    let app = app.clone();
    state.thread.get_or_init(move || {
        let _ = std::thread::Builder::new()
            .name("nfd-monitor".into())
            .spawn(move || {
                loop {
                    let start = Instant::now();
                    round(&app, &inner);
                    std::thread::sleep(Duration::from_secs(1).saturating_sub(start.elapsed()));
                }
            });
    });
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    /// The user asked for file-level detail.
    wanted: bool,
    /// A trace session is running.
    running: bool,
    /// It runs in the elevated helper (else in the app, which is elevated).
    remote: bool,
    /// Why it stopped or could not start (`etw-silent`, `helper-stopped`…).
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorView {
    snapshot: Option<Snapshot>,
    paused: bool,
    elevated: bool,
    detail: Detail,
    watch: Option<String>,
    alerts: Vec<StampedAlert>,
    touches: Vec<TouchRow>,
    config: SpamConfig,
}

fn view_of(g: &Inner) -> MonitorView {
    MonitorView {
        snapshot: g.last.clone(),
        paused: g.paused,
        elevated: elevated(),
        detail: Detail {
            wanted: g.detail_wanted,
            running: g.detail_running(),
            remote: g.detail_remote,
            error: g.detail_error.clone(),
        },
        watch: g.watch.clone(),
        alerts: g.alerts.iter().cloned().collect(),
        touches: g.touches.iter().take(500).cloned().collect(),
        config: g.monitor.alerts(),
    }
}

/// The latest sample (polled about once a second by the view, which keeps
/// sampling on).
#[tauri::command]
pub fn monitor_snapshot(app: AppHandle, state: State<'_, MonitorState>) -> CmdResult<MonitorView> {
    ensure_thread(&app, &state);
    let mut g = state.inner.lock().map_err(|e| err("internal", e))?;
    g.view_seen = Some(Instant::now());
    Ok(view_of(&g))
}

/// The realtime chart: the last 60 or 600 seconds.
#[tauri::command]
pub fn monitor_chart(state: State<'_, MonitorState>, seconds: usize) -> CmdResult<Vec<Point>> {
    let g = state.inner.lock().map_err(|e| err("internal", e))?;
    Ok(g.monitor
        .chart(seconds.clamp(10, nfd_monitor::chart::CAPACITY)))
}

/// Pause / resume all sampling (and the trace session).
#[tauri::command]
pub fn monitor_pause(
    app: AppHandle,
    state: State<'_, MonitorState>,
    paused: bool,
) -> CmdResult<bool> {
    set_paused(&app, &state, paused)
}

/// Shared with the tray menu.
pub fn set_paused(app: &AppHandle, state: &MonitorState, paused: bool) -> CmdResult<bool> {
    {
        let mut g = state.inner.lock().map_err(|e| err("internal", e))?;
        g.paused = paused;
        if paused {
            g.stop_detail();
        }
    }
    let _ = app.emit("monitor-paused", paused);
    Ok(paused)
}

/// Turn file-level detail on (may show UAC for the helper) or off.
#[tauri::command]
pub async fn monitor_detail(
    app: AppHandle,
    state: State<'_, MonitorState>,
    enabled: bool,
) -> CmdResult<MonitorView> {
    let inner = state.inner.clone();
    {
        let mut g = inner.lock().map_err(|e| err("internal", e))?;
        g.detail_wanted = enabled;
        g.detail_error = None;
        if !enabled {
            g.stop_detail();
        }
    }
    #[cfg(windows)]
    if enabled {
        let (a, i) = (app.clone(), inner.clone());
        let r = tauri::async_runtime::spawn_blocking(move || start_detail(&a, &i, true))
            .await
            .map_err(|e| err("internal", e))?;
        if let Err(e) = r {
            let mut g = inner.lock().map_err(|e| err("internal", e))?;
            g.detail_wanted = false;
            return Err(e);
        }
    }
    #[cfg(not(windows))]
    if enabled {
        return Err(err("unsupported", "the monitor is Windows-only for now"));
    }
    let _ = app;
    let g = inner.lock().map_err(|e| err("internal", e))?;
    Ok(view_of(&g))
}

/// Watch a folder for file events ("Who touched this?"), or stop watching.
#[tauri::command]
pub async fn monitor_watch(
    app: AppHandle,
    state: State<'_, MonitorState>,
    path: Option<String>,
) -> CmdResult<MonitorView> {
    let inner = state.inner.clone();
    let restart = {
        let mut g = inner.lock().map_err(|e| err("internal", e))?;
        g.watch = path.filter(|p| std::path::Path::new(p).is_absolute());
        g.touches.clear();
        g.detail_running()
    };
    #[cfg(windows)]
    if restart {
        let (a, i) = (app.clone(), inner.clone());
        tauri::async_runtime::spawn_blocking(move || start_detail(&a, &i, false))
            .await
            .map_err(|e| err("internal", e))??;
    }
    let _ = (app, restart);
    let g = inner.lock().map_err(|e| err("internal", e))?;
    Ok(view_of(&g))
}

/// Alert settings, notification texts and whether minutes go to the history.
#[tauri::command]
pub fn monitor_configure(
    app: AppHandle,
    state: State<'_, MonitorState>,
    alerts: SpamConfig,
    texts: MonitorTexts,
    record: bool,
) -> CmdResult<()> {
    {
        let mut g = state.inner.lock().map_err(|e| err("internal", e))?;
        g.monitor.set_alerts(alerts);
        g.texts = texts;
        g.record = record;
    }
    if alerts.enabled {
        ensure_thread(&app, &state);
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorHistory {
    minutes: Vec<Minute>,
    top: Vec<TopProc>,
}

/// The last 24 hours, per minute, and the processes that wrote the most.
#[tauri::command]
pub async fn monitor_history(app: AppHandle) -> CmdResult<MonitorHistory> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let since = (unix_ms() / 1000).saturating_sub(nfd_history::monitor::KEEP_SECS);
        let rows = super::with_history(&app, &state, |h| h.monitor_minutes(since))?;
        let minutes: Vec<Minute> = rows
            .into_iter()
            .map(|r| Minute {
                ts: r.ts,
                read: r.read,
                write: r.write,
                peak_read: r.peak_read,
                peak_write: r.peak_write,
                seconds: r.seconds,
                top: serde_json::from_str(&r.top).unwrap_or_default(),
            })
            .collect();
        let top = nfd_monitor::minute::top_writers(&minutes, 10);
        Ok(MonitorHistory { minutes, top })
    })
    .await
    .map_err(|e| err("internal", e))?
}

/// Show a file the monitor reported in Explorer (only paths the view was given).
#[tauri::command]
pub fn monitor_reveal(
    app: AppHandle,
    state: State<'_, MonitorState>,
    path: String,
) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let known = {
        let g = state.inner.lock().map_err(|e| err("internal", e))?;
        let same = |p: &str| p.eq_ignore_ascii_case(&path);
        g.last.as_ref().is_some_and(|s| {
            s.files.iter().any(|f| same(&f.path))
                || s.processes
                    .iter()
                    .any(|p| p.path.as_deref().is_some_and(same))
        }) || g.touches.iter().any(|t| same(&t.path))
            || g.alerts.iter().any(|a| match &a.alert {
                Alert::BigLog { path, .. } => same(path),
                Alert::LogSpam { path, .. } => path.as_deref().is_some_and(same),
            })
    };
    if !known {
        return Err(err("not-found", "not a path the monitor reported"));
    }
    let p = std::path::Path::new(&path);
    if p.exists() {
        app.opener().reveal_item_in_dir(p).map_err(|e| err("os", e))
    } else if let Some(dir) = p.parent().filter(|d| d.is_dir()) {
        // deleted since: open the folder it was in
        app.opener()
            .open_path(dir.to_string_lossy(), None::<&str>)
            .map_err(|e| err("os", e))
    } else {
        Err(err("not-found", "gone"))
    }
}

/// Save the minute in progress and stop the trace session (app exit).
pub fn monitor_flush(app: &AppHandle) {
    let Some(state) = app.try_state::<MonitorState>() else {
        return;
    };
    let (minute, record) = {
        let Ok(mut g) = state.inner.lock() else {
            return;
        };
        g.stop_detail();
        (g.monitor.flush_minute(), g.record)
    };
    if let (Some(m), true) = (minute, record) {
        save_minutes(app, &[m]);
    }
}
