//! Health view (docs/PHASE7_HEALTH.md §2.6): SMART / NVMe verdict per drive and the
//! read-only volume checks from `nfd-health`, the "read everything" button that asks
//! the elevated helper once (UAC), the JSON export, and the drive findings of the
//! Health Score.
//!
//! Strictly read-only: nothing here changes a drive or a volume.

use std::path::Path;
use std::sync::Mutex;

use nfd_core::health::{DriveFacts, DriveHeat, DriveStatus, HealthScore};
use nfd_health::{HealthReport, Heat, Options, Privileged, Status};
use tauri::{AppHandle, Manager};

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
}

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
    report
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
