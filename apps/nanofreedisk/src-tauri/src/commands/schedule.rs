//! Automatic cleaning, low disk space alerts and "Start with Windows (in the tray)"
//! (`docs/SCHEDULE.md`). All opt-in, off by default.
//!
//! Everything runs inside the app's own process while it is open or sits in the
//! tray: no scheduled task, no service, nothing elevated. One background thread
//! checks every few minutes; the decisions are the pure functions of
//! [`crate::schedule`]. An automatic clean goes the same way as a manual Clean:
//! Planner, SafetyGate, Executor, journal, into the quarantine (restorable
//! for 7 days), journaled as kind [`KIND`].
//!
//! State lives in `schedule.json` in the local data folder (portable: `data`),
//! next to `health-state.json`: the user's choices, the last run, the low space
//! marks per drive and the notification texts in the UI language.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nfd_core::RiskLevel;
use nfd_core::exec::{self, ExecOptions, ItemStatus};
use nfd_core::plan::{CleanPlan, DeleteMode};
use nfd_core::safety::{LiveProbe, Origin, SafetyGate};
use nfd_platform::Platform;
use nfd_rules::matcher::{MatchOptions, Matcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use super::clean::{OperationDto, journal, op_dto, roots, rules_plan};
use super::{AppState, CmdResult, err, with_history};
use crate::schedule::{self as pure, Config, Due, Frequency, Status};

/// Journal kind of an automatic (or "Run now") clean.
pub const KIND: &str = "clean-scheduled";
/// Name of the `Run` value and of the startup entry the Startup view lists.
#[cfg(windows)]
pub const AUTOSTART_NAME: &str = "NANOFreeDisk";

/// First check after the app starts (a missed run waits this long), then every
/// [`EVERY`].
const FIRST_CHECK: Duration = Duration::from_secs(3 * 60);
const EVERY: Duration = Duration::from_secs(5 * 60);
/// "Run now" uses the preview the user saw, if it is this recent.
const PREVIEW_FOR: i64 = 15 * 60;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct FileData {
    config: Config,
    status: Status,
    /// Notification and tray texts in the UI language.
    texts: HashMap<String, String>,
}

/// What a preview found, kept for "Run now".
#[derive(Debug, Clone)]
struct Preview {
    plan: CleanPlan,
    at: i64,
}

#[derive(Debug, Default)]
pub struct ScheduleState {
    data: Mutex<Option<FileData>>,
    preview: Mutex<Option<Preview>>,
    /// A preview or a clean is running.
    busy: AtomicBool,
    started: AtomicBool,
    /// Started with [`crate::TRAY_FLAG`]: closing the window keeps the app in the tray.
    tray_start: AtomicBool,
}

impl ScheduleState {
    /// The app was started in the tray (`--tray`).
    pub fn set_tray_start(&self, on: bool) {
        self.tray_start.store(on, Ordering::Relaxed);
    }

    #[must_use]
    pub fn tray_start(&self) -> bool {
        self.tray_start.load(Ordering::Relaxed)
    }
}

/// Clears `busy` when dropped.
struct Busy<'a>(&'a AtomicBool);

impl<'a> Busy<'a> {
    fn take(flag: &'a AtomicBool) -> CmdResult<Self> {
        if flag.swap(true, Ordering::AcqRel) {
            return Err(err("busy", "an automatic clean or its preview is running"));
        }
        Ok(Self(flag))
    }
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    crate::paths::local_data(app).map(|d| d.join("schedule.json"))
}

/// Read `schedule.json`. Each part is read on its own: a config that does not
/// parse (hand-edited, hostile) means "everything off", without losing the rest.
fn read(app: &AppHandle) -> FileData {
    let Some(v) = file(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
    else {
        return FileData::default();
    };
    let part = |k: &str| v.get(k).cloned().unwrap_or(serde_json::Value::Null);
    FileData {
        config: serde_json::from_value::<Config>(part("config"))
            .unwrap_or_default()
            .sanitized(),
        status: serde_json::from_value(part("status")).unwrap_or_default(),
        texts: serde_json::from_value(part("texts")).unwrap_or_default(),
    }
}

fn write(app: &AppHandle, data: &FileData) {
    let Some(path) = file(app) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(data) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// Run `f` on the current data and store the result.
fn update<T>(app: &AppHandle, f: impl FnOnce(&mut FileData) -> T) -> CmdResult<T> {
    let state = app.state::<ScheduleState>();
    let mut slot = state.data.lock().map_err(|e| err("internal", e))?;
    let data = slot.get_or_insert_with(|| read(app));
    let out = f(data);
    write(app, data);
    Ok(out)
}

fn snapshot(app: &AppHandle) -> FileData {
    let state = app.state::<ScheduleState>();
    let Ok(mut slot) = state.data.lock() else {
        return FileData::default();
    };
    slot.get_or_insert_with(|| read(app)).clone()
}

/// Automatic cleaning exists where the Clean view does (Windows).
const fn clean_supported() -> bool {
    cfg!(windows)
}

/// Text `key` in the UI language, else English.
fn text(texts: &HashMap<String, String>, key: &str) -> String {
    if let Some(t) = texts.get(key).filter(|t| !t.trim().is_empty()) {
        return t.clone();
    }
    match key {
        "auto-title" => "Automatic cleaning",
        "auto-body" => "Cleaned {size} automatically. See Cleanup history.",
        "low-title" => "{drive} is almost full",
        "low-body" => "{free} free. Open NANOFreeDisk from the tray icon and choose Free up space.",
        "tray-free-up" => "Free up space",
        _ => key,
    }
    .to_owned()
}

/// Is the low space alert on (the drive health watch then leaves low space to it,
/// so a full drive is not reported twice)?
pub fn low_space_on(app: &AppHandle) -> bool {
    snapshot(app).config.low_space
}

// ---- state for the UI -----------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastRunDto {
    at: i64,
    freed: u64,
    items: usize,
    op: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutostartDto {
    supported: bool,
    on: bool,
    /// Turned off in Task Manager (Windows' Startup apps).
    blocked: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleDto {
    config: Config,
    /// Automatic cleaning is available on this platform.
    supported: bool,
    last: Option<LastRunDto>,
    /// Earliest time of the next automatic clean.
    next: Option<i64>,
    /// Categories the picker offers.
    categories: Vec<String>,
    autostart: AutostartDto,
    /// Started in the tray (`--tray`).
    tray_start: bool,
}

fn dto(app: &AppHandle) -> ScheduleDto {
    let data = snapshot(app);
    let now = exec::unix_now();
    let categories = if clean_supported() {
        nfd_rules::builtin_packs()
            .map(|p| pure::categories(&p, std::env::consts::OS))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    ScheduleDto {
        supported: clean_supported(),
        last: data.status.last_run.map(|at| LastRunDto {
            at,
            freed: data.status.last_freed,
            items: data.status.last_items,
            op: data.status.last_op.clone(),
        }),
        next: match data.status.due(&data.config, now) {
            Due::Off => None,
            Due::NotYet(at) => Some(at),
            Due::Now => Some(now),
        },
        config: data.config,
        categories,
        autostart: autostart::status(app),
        tray_start: app.state::<ScheduleState>().tray_start(),
    }
}

/// The settings, the last automatic clean and the autostart state.
#[tauri::command]
pub fn schedule_get(app: AppHandle) -> CmdResult<ScheduleDto> {
    Ok(dto(&app))
}

/// Store the user's choices (sanitized; automatic cleaning stays off where there
/// is no Clean view) and the texts in the UI language.
#[tauri::command]
pub fn schedule_set(
    app: AppHandle,
    config: Config,
    texts: Option<HashMap<String, String>>,
) -> CmdResult<ScheduleDto> {
    let mut config = config.sanitized();
    if !clean_supported() {
        config.frequency = Frequency::Off;
    }
    let low_space_was = low_space_on(&app);
    update(&app, |d| {
        d.config = config;
        if let Some(t) = texts {
            d.texts = t;
        }
        d.status.sync(&d.config, exec::unix_now());
        if !d.config.low_space {
            // start from scratch when it is turned on again
            d.status.drives.clear();
        }
    })?;
    if low_space_was && !low_space_on(&app) {
        crate::tray::set_free_up(&app, None);
    }
    *app.state::<ScheduleState>()
        .preview
        .lock()
        .map_err(|e| err("internal", e))? = None;
    Ok(dto(&app))
}

/// The notification and tray texts in the UI language (sent on start and when the
/// language changes).
#[tauri::command]
pub fn schedule_texts(app: AppHandle, texts: HashMap<String, String>) -> CmdResult<()> {
    update(&app, |d| d.texts = texts)
}

// ---- what an automatic clean takes ----------------------------------------------

/// Match the rules an automatic clean may use and plan them (blocking). Only the
/// built-in packs; only rules [`pure::picks`] lets through; then only items that
/// are Safe rule matches going to the quarantine, whatever the planner says.
fn plan_now(config: &Config) -> CmdResult<CleanPlan> {
    let packs = nfd_rules::builtin_packs().map_err(|e| err("rules", e))?;
    let running = nfd_platform::locks::running_processes().ok();
    let picks = pure::picks(&packs, config, running.as_deref(), std::env::consts::OS);
    if picks.is_empty() {
        return Ok(CleanPlan::default());
    }
    let resolve = nfd_platform::Native.resolver();
    let matcher = Matcher::new(&packs, &resolve, std::env::consts::OS);
    let report = matcher.scan(
        &|r, _| picks.iter().any(|p| p == &r.id),
        &MatchOptions::default(),
        running.as_deref().unwrap_or_default(),
    );
    let mut plan = rules_plan(&report, &picks, false, false);
    plan.items.retain(|i| {
        matches!(
            i.origin,
            Origin::Rule {
                level: RiskLevel::Safe
            }
        ) && matches!(i.mode, DeleteMode::Quarantine)
            && i.rule_id.as_ref().is_some_and(|r| picks.contains(r))
    });
    Ok(plan)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRule {
    rule_id: String,
    app: String,
    category: String,
    bytes: u64,
    items: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDto {
    rules: Vec<PreviewRule>,
    bytes: u64,
    items: usize,
}

fn preview_dto(plan: &CleanPlan) -> PreviewDto {
    let packs = nfd_rules::builtin_packs().unwrap_or_default();
    let mut by_rule: BTreeMap<String, (u64, usize)> = BTreeMap::new();
    for i in plan.selected() {
        if let Some(r) = &i.rule_id {
            let e = by_rule.entry(r.clone()).or_default();
            e.0 += i.bytes;
            e.1 += 1;
        }
    }
    let mut rules: Vec<PreviewRule> = by_rule
        .into_iter()
        .map(|(rule_id, (bytes, items))| {
            let rule = packs
                .iter()
                .flat_map(|p| &p.rules)
                .find(|r| r.id == rule_id);
            PreviewRule {
                app: rule.map(|r| r.app.clone()).unwrap_or_default(),
                category: rule.map(|r| r.category.clone()).unwrap_or_default(),
                rule_id,
                bytes,
                items,
            }
        })
        .collect();
    rules.sort_by_key(|r| std::cmp::Reverse(r.bytes));
    PreviewDto {
        bytes: plan.selected_bytes(),
        items: plan.selected().count(),
        rules,
    }
}

/// What an automatic clean would take right now (read-only). "Run now" cleans
/// exactly this.
#[tauri::command]
pub async fn schedule_preview(
    app: AppHandle,
    state: State<'_, ScheduleState>,
) -> CmdResult<PreviewDto> {
    if !clean_supported() {
        return Err(err("unsupported", "automatic cleaning is Windows-only"));
    }
    let _busy = Busy::take(&state.busy)?;
    let config = snapshot(&app).config;
    let plan = tauri::async_runtime::spawn_blocking(move || plan_now(&config))
        .await
        .map_err(|e| err("internal", e))??;
    let out = preview_dto(&plan);
    *state.preview.lock().map_err(|e| err("internal", e))? = Some(Preview {
        plan,
        at: exec::unix_now(),
    });
    Ok(out)
}

/// Execute `plan` into the quarantine and journal it as [`KIND`] (blocking).
fn execute(app: &AppHandle, plan: &CleanPlan) -> (exec::Operation, Option<String>) {
    let opts = ExecOptions {
        op_id: exec::new_op_id(exec::unix_now()),
        kind: KIND.into(),
        owner: 0,
        roots: roots(app),
        now: exec::unix_now(),
    };
    let gate = SafetyGate::new(&nfd_platform::Native.resolver());
    let op = exec::execute(plan, &gate, &LiveProbe, &opts, &mut |_, _, _| true);
    let state = app.state::<AppState>();
    let journal_error = journal(app, &state, &op);
    (op, journal_error)
}

fn purge_expired(app: &AppHandle) {
    let state = app.state::<AppState>();
    let r = roots(app);
    let _ = with_history(app, &state, |h| h.purge_expired(&r, exec::unix_now(), None));
}

/// Note the run, notify, tell the UI.
fn finish(app: &AppHandle, op: &exec::Operation, journal_error: Option<String>) -> OperationDto {
    let freed = op.freed();
    let items = op.count(ItemStatus::Done) + op.count(ItemStatus::Partial);
    let _ = update(app, |d| {
        d.status
            .ran(exec::unix_now(), freed, items, Some(op.id.clone()));
    });
    if freed > 0 {
        use tauri_plugin_notification::NotificationExt;
        let texts = snapshot(app).texts;
        let _ = app
            .notification()
            .builder()
            .title(text(&texts, "auto-title"))
            .body(text(&texts, "auto-body").replace("{size}", &nfd_core::units::format_size(freed)))
            .show();
    }
    let out = op_dto(op, journal_error);
    let _ = app.emit("schedule-cleaned", &out);
    out
}

/// Clean what the last preview showed (the user saw the list and confirmed).
/// Counts as this period's automatic clean.
#[tauri::command]
pub async fn schedule_run_now(
    app: AppHandle,
    state: State<'_, ScheduleState>,
) -> CmdResult<OperationDto> {
    if !clean_supported() {
        return Err(err("unsupported", "automatic cleaning is Windows-only"));
    }
    let preview = state
        .preview
        .lock()
        .map_err(|e| err("internal", e))?
        .take()
        .filter(|p| exec::unix_now().saturating_sub(p.at) <= PREVIEW_FOR)
        .ok_or_else(|| err("no-preview", "show the preview again"))?;
    if preview.plan.selected().next().is_none() {
        return Err(err("nothing-to-clean", "the rules have nothing to clean"));
    }
    let _busy = Busy::take(&state.busy)?;
    purge_expired(&app);
    let app2 = app.clone();
    let (op, journal_error) =
        tauri::async_runtime::spawn_blocking(move || execute(&app2, &preview.plan))
            .await
            .map_err(|e| err("internal", e))?;
    Ok(finish(&app, &op, journal_error))
}

// ---- the background check --------------------------------------------------------

/// Seconds since the last keyboard or mouse input, if Windows says.
#[cfg(windows)]
fn idle_secs() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(std::mem::size_of::<LASTINPUTINFO>()).ok()?,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid LASTINPUTINFO with `cbSize` set.
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return None;
    }
    // SAFETY: no arguments.
    let now = unsafe { GetTickCount() };
    Some(u64::from(now.wrapping_sub(info.dwTime)) / 1000)
}

#[cfg(not(windows))]
const fn idle_secs() -> Option<u64> {
    None
}

/// Low space: fixed drives against their thresholds; notify the new ones.
fn check_space(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    if !low_space_on(app) {
        return;
    }
    let drives: Vec<(String, u64, u64)> = nfd_platform::volumes::list()
        .into_iter()
        .filter(|v| v.kind == nfd_platform::volumes::VolumeKind::Fixed && v.ready && v.total > 0)
        .map(|v| (v.root.to_string_lossy().into_owned(), v.free, v.total))
        .collect();
    let Ok((alerts, any_low, texts)) = update(app, |d| {
        let now = exec::unix_now();
        d.status.sync(&d.config, now);
        let alerts = pure::low_space(&mut d.status, &drives, &d.config, now);
        let any_low = d.status.drives.values().any(|m| m.low);
        (alerts, any_low, d.texts.clone())
    }) else {
        return;
    };
    crate::tray::set_free_up(app, any_low.then(|| text(&texts, "tray-free-up")));
    for a in alerts {
        let drive = a.root.trim_end_matches(['\\', '/']).to_owned();
        let drive = if drive.is_empty() {
            a.root.clone()
        } else {
            drive
        };
        let free = nfd_core::units::format_size(a.free);
        let _ = app
            .notification()
            .builder()
            .title(text(&texts, "low-title").replace("{drive}", &drive))
            .body(
                text(&texts, "low-body")
                    .replace("{drive}", &drive)
                    .replace("{free}", &free),
            )
            .show();
        #[allow(clippy::cast_precision_loss)]
        let pct = a.free as f64 * 100.0 / a.total as f64;
        // the same in-app toast as the drive health alerts
        let _ = app.emit(
            "health-alert",
            serde_json::json!({
                "kind": "low-space",
                "subject": a.root,
                "status": null,
                "value": pct,
                "limit": null,
            }),
        );
    }
}

/// Automatic clean, when due (and the PC is idle, if asked).
fn check_clean(app: &AppHandle) {
    if !clean_supported() {
        return;
    }
    let now = exec::unix_now();
    let Ok(due) = update(app, |d| {
        d.status.sync(&d.config, now);
        (d.status.due(&d.config, now), d.config.clone())
    }) else {
        return;
    };
    let (Due::Now, config) = due else {
        return;
    };
    if config.idle_only && idle_secs().is_some_and(|s| s < pure::IDLE_SECS) {
        return;
    }
    let state = app.state::<ScheduleState>();
    let Ok(_busy) = Busy::take(&state.busy) else {
        return;
    };
    purge_expired(app);
    match plan_now(&config) {
        Ok(plan) if plan.selected().next().is_some() => {
            let (op, journal_error) = execute(app, &plan);
            finish(app, &op, journal_error);
        }
        // nothing to clean, or the rules did not load: this period is done either way
        _ => {
            let _ = update(app, |d| d.status.ran(exec::unix_now(), 0, 0, None));
            let _ = app.emit("schedule-cleaned", serde_json::Value::Null);
        }
    }
}

/// Load the state, refresh the autostart entry if the exe moved, and start the
/// background check (once). Called from `setup`.
pub fn start(app: &AppHandle) {
    let state = app.state::<ScheduleState>();
    if state.started.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = update(app, |d| d.status.sync(&d.config, exec::unix_now()));
    autostart::refresh(app);
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("schedule".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK);
            loop {
                check_space(&app);
                check_clean(&app);
                std::thread::sleep(EVERY);
            }
        });
}

/// Turn "Start NANOFreeDisk with Windows (in the tray)" on or off.
#[tauri::command]
pub fn autostart_set(app: AppHandle, enabled: bool) -> CmdResult<AutostartDto> {
    autostart::set(&app, enabled).map_err(|e| err("os", e))?;
    Ok(autostart::status(&app))
}

/// The tray's "Free up space": the UI opens Quick clean (or Clean).
pub fn open_free_up(app: &AppHandle) {
    let _ = app.emit("schedule-open", "free-up");
}

/// Remove the app's own "start with Windows" entry (the uninstall cleanup).
pub fn autostart_remove() {
    let _ = autostart::remove();
}

mod autostart {
    //! This user's `Run` value `NANOFreeDisk` = `"<exe>" --tray` (Windows), or a
    //! LaunchAgent in `~/Library/LaunchAgents` (macOS). Only the app's own entry is
    //! ever written or removed.

    use super::{AppHandle, AutostartDto};

    #[cfg(any(windows, target_os = "macos"))]
    fn exe() -> std::io::Result<String> {
        Ok(std::env::current_exe()?.to_string_lossy().into_owned())
    }

    #[cfg(windows)]
    mod imp {
        use nfd_platform::registry::{self, Hive, Value};

        use super::exe;
        use crate::commands::schedule::AUTOSTART_NAME;
        use crate::schedule::autostart_command;

        const RUN: &str = nfd_apps::startup::RUN;

        fn approved() -> String {
            format!(r"{}\Run", nfd_apps::startup::APPROVED)
        }

        fn command() -> Option<String> {
            match registry::value(Hive::CurrentUser, RUN, AUTOSTART_NAME) {
                Ok(Some(v)) => v.as_str().map(str::to_owned),
                _ => None,
            }
        }

        /// Turned off in Task Manager: its flag's first byte is odd.
        fn blocked() -> bool {
            matches!(
                registry::value(Hive::CurrentUser, &approved(), AUTOSTART_NAME),
                Ok(Some(Value::Binary(b))) if b.first().is_some_and(|x| x % 2 == 1)
            )
        }

        pub fn status() -> (bool, bool) {
            let on = command().is_some();
            (on, on && blocked())
        }

        pub fn set(enabled: bool) -> std::io::Result<()> {
            if enabled {
                nfd_platform::registry_write::set_value(
                    Hive::CurrentUser,
                    RUN,
                    AUTOSTART_NAME,
                    &Value::Sz(autostart_command(&exe()?)),
                )?;
                // turned on here after being turned off in Task Manager: turn its
                // flag back on, the way Task Manager does (`02`, no time)
                if blocked() {
                    let mut on = vec![2u8];
                    on.resize(12, 0);
                    nfd_platform::registry_write::set_value(
                        Hive::CurrentUser,
                        &approved(),
                        AUTOSTART_NAME,
                        &Value::Binary(on),
                    )?;
                }
                Ok(())
            } else if command().is_some() {
                registry::delete_value(Hive::CurrentUser, RUN, AUTOSTART_NAME)
            } else {
                Ok(())
            }
        }

        /// The exe moved (update, portable copy elsewhere): point the entry at it.
        pub fn refresh() {
            let (Some(cmd), Ok(exe)) = (command(), exe()) else {
                return;
            };
            let want = autostart_command(&exe);
            if cmd.contains(crate::TRAY_FLAG) && !cmd.eq_ignore_ascii_case(&want) {
                let _ = nfd_platform::registry_write::set_value(
                    Hive::CurrentUser,
                    RUN,
                    AUTOSTART_NAME,
                    &Value::Sz(want),
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    mod imp {
        use std::path::PathBuf;

        use super::exe;

        const LABEL: &str = "vn.io.nanodev.nanofreedisk";

        fn plist() -> Option<PathBuf> {
            std::env::var_os("HOME").map(|h| {
                PathBuf::from(h)
                    .join("Library/LaunchAgents")
                    .join(format!("{LABEL}.plist"))
            })
        }

        pub fn status() -> (bool, bool) {
            (plist().is_some_and(|p| p.is_file()), false)
        }

        pub fn set(enabled: bool) -> std::io::Result<()> {
            let path = plist().ok_or_else(|| std::io::Error::other("no HOME"))?;
            if enabled {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&path, crate::schedule::launch_agent(LABEL, &exe()?))
            } else if path.is_file() {
                std::fs::remove_file(&path)
            } else {
                Ok(())
            }
        }

        pub fn refresh() {
            let (Some(path), Ok(exe)) = (plist(), exe()) else {
                return;
            };
            let want = crate::schedule::launch_agent(LABEL, &exe);
            if path.is_file() && std::fs::read_to_string(&path).is_ok_and(|t| t != want) {
                let _ = std::fs::write(&path, want);
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    mod imp {
        pub fn status() -> (bool, bool) {
            (false, false)
        }
        pub fn set(_: bool) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "not on this platform",
            ))
        }
        pub fn refresh() {}
    }

    pub fn status(_app: &AppHandle) -> AutostartDto {
        let (on, blocked) = imp::status();
        AutostartDto {
            supported: cfg!(any(windows, target_os = "macos")),
            on,
            blocked,
        }
    }

    pub fn set(_app: &AppHandle, enabled: bool) -> std::io::Result<()> {
        imp::set(enabled)
    }

    pub fn remove() -> std::io::Result<()> {
        imp::set(false)
    }

    pub fn refresh(_app: &AppHandle) {
        imp::refresh();
    }
}
