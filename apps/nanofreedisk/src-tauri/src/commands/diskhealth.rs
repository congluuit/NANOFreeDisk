//! Health view (docs/PHASE7_HEALTH.md §2.6): SMART / NVMe verdict per drive and the
//! read-only volume checks from `nfd-health`, the "read everything" button that asks
//! the elevated helper once (UAC), the JSON export, the drive findings of the
//! Health Score, and the alert watch (G5): every 30 minutes while the app runs, a
//! notification when a drive's verdict gets worse, a drive passes its warning
//! temperature or a volume drops below 10 % free; the tray icon shows the result.
//!
//! Strictly read-only: nothing here changes a drive or a volume.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use nfd_core::health::{DriveFacts, DriveHeat, DriveStatus, HealthScore};
use nfd_health::alerts::{Alert, AlertKind, AlertState};
use nfd_health::{HealthReport, Heat, Options, Privileged, Status};
use tauri::{AppHandle, Emitter, Manager};

use super::{AppState, CmdResult, err, with_history};

/// Days of scan history the "full in ~N days" forecast fits (PLAN §4.6).
const TREND_DAYS: i64 = 30;

#[derive(Debug, Default)]
pub struct HealthState {
    /// Last report shown (the Health Score reuses it).
    report: Mutex<Option<HealthReport>>,
    /// What the elevated helper read this session; folded into later refreshes
    /// (thresholds never change, the dirty bit only at chkdsk / restart).
    privileged: Mutex<Option<Privileged>>,
    watch: Mutex<Watch>,
}

/// Alert settings from the UI.
#[derive(Debug, Default)]
struct Watch {
    enabled: bool,
    /// Also alert on temperature (off while the Hardware Monitor's own temperature
    /// alerts are on, so a hot drive is not reported twice).
    temperatures: bool,
    /// Notification and tray texts in the UI language (`{subject}`, `{status}`,
    /// `{value}`, `{limit}` are filled in here).
    texts: HashMap<String, String>,
    started: bool,
}

/// First check after start (leave the app time to come up), then every 30 minutes.
const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(30 * 60);

impl HealthState {
    pub fn cached(&self) -> Option<HealthReport> {
        self.report.lock().ok().and_then(|r| r.clone())
    }

    fn store(&self, report: &HealthReport) {
        if let Ok(mut slot) = self.report.lock() {
            *slot = Some(report.clone());
        }
    }
}

fn add_trend(app: &AppHandle, report: &mut HealthReport) {
    let state = app.state::<AppState>();
    let now = report.generated_at;
    for v in &mut report.volumes {
        let trend = with_history(app, &state, |h| {
            h.forecast(&v.root, now, v.free, v.total, TREND_DAYS)
        })
        .ok()
        .flatten();
        v.has_trend = trend.is_some();
        v.days_to_full = trend.and_then(|t| t.days_to_full);
    }
}

/// A full read now (blocking: IOCTLs and a shell property per volume).
pub fn fresh(app: &AppHandle) -> HealthReport {
    let mut report = nfd_health::collect(Options::default());
    let state = app.state::<HealthState>();
    if let Some(p) = state.privileged.lock().ok().and_then(|p| p.clone()) {
        report.merge(&p);
    }
    add_trend(app, &mut report);
    state.store(&report);
    update_tray(app, &report);
    report
}

fn update_tray(app: &AppHandle, report: &HealthReport) {
    let texts = app
        .state::<HealthState>()
        .watch
        .lock()
        .map(|w| w.texts.clone())
        .unwrap_or_default();
    let status = match report.worst() {
        Status::Good => "good",
        Status::Caution => "caution",
        Status::Bad => "bad",
        Status::Unknown => "unknown",
    };
    let drives = report
        .volumes
        .iter()
        .filter(|v| !v.removable && v.total > 0)
        .map(|v| (v.root.clone(), v.free, v.total))
        .collect();
    crate::tray::update(
        app,
        &crate::tray::TrayState {
            status: status.to_owned(),
            drives,
        },
        &texts,
    );
}

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    crate::paths::local_data(app).map(|d| d.join("health-state.json"))
}

/// Fill an alert's template.
fn fill(template: &str, a: &Alert, texts: &HashMap<String, String>) -> String {
    let status = a
        .status
        .map(|s| {
            let key = match s {
                Status::Bad => "status-bad",
                Status::Caution => "status-caution",
                Status::Good => "status-good",
                Status::Unknown => "status-unknown",
            };
            crate::tray::text(texts, key)
        })
        .unwrap_or_default();
    template
        .replace("{subject}", a.subject.trim_end_matches('\\'))
        .replace("{status}", &status)
        .replace("{value}", &format!("{:.0}", a.value))
        .replace("{limit}", &format!("{:.0}", a.limit.unwrap_or(0.0)))
}

/// English fallbacks until the UI sends its texts.
fn alert_text(texts: &HashMap<String, String>, kind: AlertKind, part: &str) -> String {
    let key = format!(
        "alert-{}-{part}",
        match kind {
            AlertKind::Status => "status",
            AlertKind::Hot => "hot",
            AlertKind::LowSpace => "low",
        }
    );
    if let Some(t) = texts.get(&key).filter(|t| !t.is_empty()) {
        return t.clone();
    }
    match (kind, part) {
        (AlertKind::Status, "title") => "{subject}: drive health {status}",
        (AlertKind::Status, _) => "Back up your data and open Drive health for the details.",
        (AlertKind::Hot, "title") => "{subject} is hot: {value} °C",
        (AlertKind::Hot, _) => "Above the drive's warning temperature ({limit} °C).",
        (AlertKind::LowSpace, "title") => "{subject} is almost full",
        (AlertKind::LowSpace, _) => "Only {value} % free space is left.",
    }
    .to_owned()
}

/// One check: read, compare with the last check, notify.
fn check(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    let report = fresh(app);
    let (enabled, temperatures, texts) = match app.state::<HealthState>().watch.lock() {
        Ok(w) => (w.enabled, w.temperatures, w.texts.clone()),
        Err(_) => return,
    };
    if !enabled {
        return;
    }
    let Some(path) = state_path(app) else {
        return;
    };
    let mut last: AlertState = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let alerts = nfd_health::alerts::check(&mut last, &report, temperatures);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&last) {
        let _ = std::fs::write(&path, json);
    }
    for a in alerts {
        let _ = app
            .notification()
            .builder()
            .title(fill(&alert_text(&texts, a.kind, "title"), &a, &texts))
            .body(fill(&alert_text(&texts, a.kind, "body"), &a, &texts))
            .show();
        let _ = app.emit("health-alert", &a);
    }
}

/// Alerts on or off, with the texts in the UI language. Starts the watch thread on
/// first use (it also keeps the tray icon current while alerts are off).
#[tauri::command]
pub fn health_watch_set(
    app: AppHandle,
    enabled: bool,
    temperatures: bool,
    texts: HashMap<String, String>,
) -> CmdResult<()> {
    let state = app.state::<HealthState>();
    let start = {
        let mut w = state.watch.lock().map_err(|e| err("internal", e))?;
        w.enabled = enabled;
        w.temperatures = temperatures;
        w.texts = texts;
        !std::mem::replace(&mut w.started, true)
    };
    if let Some(r) = state.cached() {
        update_tray(&app, &r);
    }
    if start {
        let app = app.clone();
        std::thread::Builder::new()
            .name("health-watch".into())
            .spawn(move || {
                std::thread::sleep(FIRST_CHECK);
                loop {
                    check(&app);
                    std::thread::sleep(EVERY);
                }
            })
            .map_err(|e| err("internal", e))?;
    }
    Ok(())
}

/// Drives and volumes; `refresh` reads again, otherwise the last report is reused.
#[tauri::command]
pub async fn health_report(app: AppHandle, refresh: bool) -> CmdResult<HealthReport> {
    if !refresh {
        if let Some(r) = app.state::<HealthState>().cached() {
            return Ok(r);
        }
    }
    tauri::async_runtime::spawn_blocking(move || fresh(&app))
        .await
        .map_err(|e| err("internal", e))
}

/// Read SMART thresholds, USB bridges and the dirty bit through the elevated helper
/// (one UAC prompt; the helper exits right after).
#[tauri::command]
pub async fn health_elevated(app: AppHandle) -> CmdResult<HealthReport> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<HealthState>();
            let mut report = state.cached().unwrap_or_else(|| fresh(&app));
            let (drives, volumes) = nfd_health::privileged_targets(&report);
            let p = nfd_helper::health_once(&drives, &volumes).map_err(|e| match e {
                nfd_helper::StartError::Declined => err("declined", "UAC was declined"),
                other => err("helper", format!("{other:?}")),
            })?;
            report.merge(&p);
            if let Ok(mut slot) = state.privileged.lock() {
                *slot = Some(p);
            }
            state.store(&report);
            Ok(report)
        })
        .await
        .map_err(|e| err("internal", e))?
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err(err("unsupported", "Windows only"))
    }
}

/// Only absolute `*.json` paths (the save dialog's result).
fn json_path(path: &str) -> CmdResult<&Path> {
    let p = Path::new(path);
    if p.is_absolute()
        && p.extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("json"))
    {
        Ok(p)
    } else {
        Err(err("bad-path", "only absolute *.json files"))
    }
}

/// Write the last report as JSON (`nanofreedisk.health.v1`); serial numbers are left
/// out unless `serials` is set (the file is usually sent to someone else).
#[tauri::command]
pub async fn health_export(app: AppHandle, path: String, serials: bool) -> CmdResult<()> {
    let target = json_path(&path)?.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HealthState>();
        let mut report = state.cached().unwrap_or_else(|| fresh(&app));
        if !serials {
            report.redact();
        }
        let json = serde_json::to_string_pretty(&report).map_err(|e| err("internal", e))?;
        std::fs::write(&target, json).map_err(|e| err("os", e))
    })
    .await
    .map_err(|e| err("internal", e))?
}

fn facts(report: &HealthReport) -> (Vec<DriveFacts>, Vec<String>) {
    let drives = report
        .drives
        .iter()
        .map(|d| DriveFacts {
            name: d.model.clone(),
            status: match d.status {
                Status::Unknown => DriveStatus::Unknown,
                Status::Good => DriveStatus::Good,
                Status::Caution => DriveStatus::Caution,
                Status::Bad => DriveStatus::Bad,
            },
            heat: match d.heat {
                Heat::Unknown => DriveHeat::Unknown,
                Heat::Normal => DriveHeat::Normal,
                Heat::Warning => DriveHeat::Warning,
                Heat::Critical => DriveHeat::Critical,
            },
            temperature: d.temperature.map(f64::from),
            life_left: d.life_left,
        })
        .collect();
    let dirty = report
        .volumes
        .iter()
        .filter(|v| v.dirty == Some(true))
        .map(|v| v.root.clone())
        .collect();
    (drives, dirty)
}

/// Health Score contribution (F1): drive verdicts, drive temperature against the
/// drive's own limits, dirty volumes. Uses the last report; without one, a quick
/// drives-only read (no volumes, no elevation).
pub fn add_findings(app: &AppHandle, score: &mut HealthScore) {
    let report = app.state::<HealthState>().cached().unwrap_or_else(|| {
        nfd_health::collect(Options {
            volumes: false,
            privileged: false,
        })
    });
    let (drives, dirty) = facts(&report);
    nfd_core::health::add_drive_findings(score, &drives, &dirty);
}

#[cfg(test)]
mod tests {
    use super::json_path;

    #[test]
    fn export_only_writes_json() {
        let abs = if cfg!(windows) {
            "C:\\x\\health.json"
        } else {
            "/x/health.json"
        };
        assert!(json_path(abs).is_ok());
        assert!(json_path("health.json").is_err());
        assert!(json_path(&abs.replace(".json", ".exe")).is_err());
    }
}
