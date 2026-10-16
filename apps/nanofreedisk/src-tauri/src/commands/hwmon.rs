//! Hardware view commands (docs/HARDWARE_MONITOR.md §4): inventory, sensors and
//! tasks. Hardware access is read-only; a process is only ever ended by
//! [`task_end`], which the UI calls after the user confirmed in its dialog.
//!
//! Sensors are sampled on demand: the Sensors tab (and the Home card) poll
//! [`sensors_snapshot`] about once a second, which samples when the last sample is
//! older than ~0.9 s. Nothing samples while no view asks.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nfd_sensors::{SensorHub, Snapshot};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::{CmdResult, err};

/// Hardware view state, managed next to `AppState`.
#[derive(Debug, Default)]
pub struct HwState {
    hub: Arc<Mutex<Option<SensorHub>>>,
    #[cfg(windows)]
    tasks: Arc<Mutex<Option<nfd_tasks::TaskMonitor>>>,
    /// Why Advanced sensors could not start, for the UI.
    advanced_error: Arc<Mutex<Option<String>>>,
}

/// Static hardware inventory (`nanofreedisk.hwinfo.v1`).
#[tauri::command]
pub async fn hw_inventory() -> CmdResult<nfd_hwinfo::HwInventory> {
    tauri::async_runtime::spawn_blocking(nfd_hwinfo::collect)
        .await
        .map_err(|e| err("internal", e))
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

/// Write a fresh inventory as JSON to a path the user picked.
#[tauri::command]
pub async fn hw_export(path: String) -> CmdResult<()> {
    let target = json_path(&path)?.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let inv = nfd_hwinfo::collect();
        let json = serde_json::to_string_pretty(&inv).map_err(|e| err("internal", e))?;
        std::fs::write(&target, json).map_err(|e| err("os", e))
    })
    .await
    .map_err(|e| err("internal", e))?
}

/// Run `f` on the hub (created on first use) off the async runtime.
async fn with_hub<T: Send + 'static>(
    state: &State<'_, HwState>,
    f: impl FnOnce(&mut SensorHub) -> T + Send + 'static,
) -> CmdResult<T> {
    let hub = state.hub.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = hub.lock().map_err(|e| err("internal", e))?;
        let hub = guard.get_or_insert_with(|| {
            let mut h = SensorHub::native();
            h.sample();
            h
        });
        Ok(f(hub))
    })
    .await
    .map_err(|e| err("internal", e))?
}

/// Current values with min / max; samples first when the last sample is stale.
#[tauri::command]
pub async fn sensors_snapshot(state: State<'_, HwState>) -> CmdResult<Snapshot> {
    with_hub(&state, |hub| {
        if hub
            .since_last()
            .is_none_or(|d| d >= Duration::from_millis(900))
        {
            hub.sample();
        }
        hub.snapshot()
    })
    .await
}

#[tauri::command]
pub async fn sensors_reset_minmax(state: State<'_, HwState>) -> CmdResult<()> {
    with_hub(&state, SensorHub::reset_minmax).await
}

/// The last `n` values of one sensor (sparkline), oldest first.
#[tauri::command]
pub async fn sensors_history(
    state: State<'_, HwState>,
    id: String,
    n: usize,
) -> CmdResult<Vec<f32>> {
    with_hub(&state, move |hub| {
        hub.history(&id, n.min(nfd_sensors::HISTORY))
    })
    .await
}

/// Advanced sensors (PawnIO) state for the Sensors tab. Never needs admin.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedStatus {
    #[cfg(windows)]
    pawnio: nfd_sensors::win::pawnio::PawnStatus,
    /// The elevated helper is connected and feeding the hub.
    active: bool,
    /// Why the helper has no sensors (module missing / rejected…).
    unavailable: Option<String>,
}

#[tauri::command]
pub async fn pawnio_status(state: State<'_, HwState>) -> CmdResult<AdvancedStatus> {
    let active = state
        .hub
        .lock()
        .map_err(|e| err("internal", e))?
        .as_ref()
        .is_some_and(|h| h.has_provider("helper"));
    let unavailable = state
        .advanced_error
        .lock()
        .map_err(|e| err("internal", e))?
        .clone();
    #[cfg(windows)]
    let pawnio = tauri::async_runtime::spawn_blocking(nfd_sensors::win::pawnio::status)
        .await
        .map_err(|e| err("internal", e))?;
    Ok(AdvancedStatus {
        #[cfg(windows)]
        pawnio,
        active,
        unavailable,
    })
}

/// Start the elevated helper (UAC prompt) and add its sensors to the hub.
#[tauri::command]
pub async fn advanced_sensors_start(state: State<'_, HwState>) -> CmdResult<AdvancedStatus> {
    #[cfg(windows)]
    {
        let hub = state.hub.clone();
        let error = state.advanced_error.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if hub
                .lock()
                .map_err(|e| err("internal", e))?
                .as_ref()
                .is_some_and(|h| h.has_provider("helper"))
            {
                return Ok(());
            }
            let remote = nfd_helper::start().map_err(|e| {
                let code = match e {
                    nfd_helper::StartError::Declined => "uac-declined",
                    _ => "helper-failed",
                };
                err(code, format!("{e:?}"))
            })?;
            let mut guard = hub.lock().map_err(|e| err("internal", e))?;
            let h = guard.get_or_insert_with(SensorHub::native);
            let mut remote = Box::new(remote);
            // discover through the hub so ids are merged with the driver-free sensors
            let probe = nfd_sensors::SensorProvider::discover(remote.as_mut());
            let missing = probe.sensors.is_empty().then(|| {
                remote
                    .unavailable
                    .clone()
                    .unwrap_or_else(|| "no-sensors".into())
            });
            *error.lock().map_err(|e| err("internal", e))? = missing.clone();
            if missing.is_none() {
                h.add_provider(Box::new(Rediscovered {
                    inner: remote,
                    first: Some(probe),
                }));
            }
            Ok(())
        })
        .await
        .map_err(|e| err("internal", e))??;
    }
    pawnio_status(state).await
}

/// Hands the hub the discovery already done (a second Discover would reload the
/// PawnIO module in the helper).
#[cfg(windows)]
struct Rediscovered {
    inner: Box<nfd_helper::RemoteProvider>,
    first: Option<nfd_sensors::Discovery>,
}

#[cfg(windows)]
impl nfd_sensors::SensorProvider for Rediscovered {
    fn id(&self) -> &'static str {
        "helper"
    }
    fn discover(&mut self) -> nfd_sensors::Discovery {
        self.first.take().unwrap_or_default()
    }
    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        self.inner.sample(out);
    }
    fn live_notes(&self) -> Vec<nfd_sensors::Note> {
        if self.inner.connected() {
            Vec::new()
        } else {
            vec![nfd_sensors::Note {
                device: "cpu".into(),
                kind: nfd_sensors::Kind::Temperature,
                reason: "helper-stopped".into(),
            }]
        }
    }
}

#[cfg(not(windows))]
fn unsupported<T>() -> CmdResult<T> {
    Err(err("unsupported", "Tasks are Windows-only for now"))
}

/// Process list with CPU / memory / I/O / GPU rates (first call: rates are 0).
#[tauri::command]
pub async fn tasks_snapshot(state: State<'_, HwState>) -> CmdResult<nfd_tasks::TaskSnapshot> {
    #[cfg(windows)]
    {
        let mon = state.tasks.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut guard = mon.lock().map_err(|e| err("internal", e))?;
            Ok(guard
                .get_or_insert_with(nfd_tasks::TaskMonitor::new)
                .snapshot())
        })
        .await
        .map_err(|e| err("internal", e))?
    }
    #[cfg(not(windows))]
    {
        let _ = state;
        unsupported()
    }
}

/// What the End-task dialog must say: blocked, warn (second confirmation) or allowed.
#[tauri::command]
pub async fn task_verdict(pid: u32, create_time: String) -> CmdResult<serde_json::Value> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let v = match nfd_tasks::verdict(pid, &create_time) {
                Ok(v) => serde_json::to_value(v),
                Err(o) => serde_json::to_value(
                    serde_json::json!({ "verdict": "unavailable", "reasons": o }),
                ),
            };
            v.map_err(|e| err("internal", e))
        })
        .await
        .map_err(|e| err("internal", e))?
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, create_time);
        unsupported()
    }
}

/// Command line, user, publisher, hosted services (Expert details / dialog).
#[tauri::command]
pub async fn task_details(pid: u32, create_time: String) -> CmdResult<serde_json::Value> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || match nfd_tasks::details(pid, &create_time) {
            Ok(d) => serde_json::to_value(d).map_err(|e| err("internal", e)),
            Err(o) => Err(err("task-changed", format!("{o:?}"))),
        })
        .await
        .map_err(|e| err("internal", e))?
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, create_time);
        unsupported()
    }
}

/// End a process the user confirmed (`confirmed`: the second confirmation for
/// warned processes). Every attempt is appended to `logs/tasks.log`.
#[tauri::command]
pub async fn task_end(
    app: AppHandle,
    pid: u32,
    create_time: String,
    mode: nfd_tasks::EndMode,
    confirmed: bool,
) -> CmdResult<Vec<nfd_tasks::EndResult>> {
    #[cfg(windows)]
    {
        let log = app.path().app_log_dir().ok().map(|d| d.join("tasks.log"));
        tauri::async_runtime::spawn_blocking(move || {
            let results = nfd_tasks::end_task(pid, &create_time, mode, confirmed);
            if let Some(path) = log {
                append_log(&path, mode, &results);
            }
            Ok(results)
        })
        .await
        .map_err(|e| err("internal", e))?
    }
    #[cfg(not(windows))]
    {
        let _ = (app, pid, create_time, mode, confirmed);
        unsupported()
    }
}

/// One JSON line per process the user asked to end (Activity log, G6).
#[cfg(windows)]
fn append_log(path: &Path, mode: nfd_tasks::EndMode, results: &[nfd_tasks::EndResult]) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for r in results {
        let line = serde_json::json!({ "time": now, "action": "end-task", "mode": mode, "pid": r.pid, "name": r.name, "outcome": r.outcome });
        let _ = writeln!(f, "{line}");
    }
}

/// The official PawnIO site (download page), in the default browser.
#[tauri::command]
pub fn pawnio_open_site(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url("https://pawnio.eu/", None::<&str>)
        .map_err(|e| err("os", e))
}

/// Task Manager, for what this view does not do (startup apps, services…).
#[tauri::command]
pub fn open_task_manager() -> CmdResult<()> {
    std::process::Command::new("taskmgr.exe")
        .spawn()
        .map(|_| ())
        .map_err(|e| err("os", e))
}

/// Show a process's executable in Explorer (the path comes from the live process,
/// not from the UI).
#[tauri::command]
pub async fn task_reveal(
    app: AppHandle,
    state: State<'_, HwState>,
    pid: u32,
    create_time: String,
) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    #[cfg(windows)]
    let path = {
        let mon = state.tasks.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut guard = mon.lock().map_err(|e| err("internal", e))?;
            let snap = guard
                .get_or_insert_with(nfd_tasks::TaskMonitor::new)
                .snapshot();
            snap.tasks
                .into_iter()
                .find(|t| t.pid == pid && t.create_time == create_time)
                .and_then(|t| t.path)
                .ok_or_else(|| err("task-changed", "process not found"))
        })
        .await
        .map_err(|e| err("internal", e))??
    };
    #[cfg(not(windows))]
    let path: String = {
        let _ = (state, pid, create_time);
        return unsupported();
    };
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| err("os", e))
}

#[cfg(test)]
mod tests {
    use super::json_path;

    #[test]
    fn export_only_writes_json() {
        let abs = if cfg!(windows) {
            "C:\\x\\hw.json"
        } else {
            "/x/hw.json"
        };
        assert!(json_path(abs).is_ok());
        assert!(json_path("hw.json").is_err());
        assert!(json_path(&abs.replace(".json", ".exe")).is_err());
    }
}
