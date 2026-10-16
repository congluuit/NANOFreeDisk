//! Hardware view commands (docs/HARDWARE_MONITOR.md §4): inventory and sensors.
//! Hardware access is read-only.
//!
//! Sensors are sampled on demand: the Sensors tab (and the Home card) poll
//! [`sensors_snapshot`] about once a second, which samples when the last sample is
//! older than ~0.9 s. Nothing samples while no view asks.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nfd_sensors::{SensorHub, Snapshot};
use tauri::State;

use super::{CmdResult, err};

/// Hardware view state, managed next to `AppState`.
#[derive(Debug, Default)]
pub struct HwState {
    hub: Arc<Mutex<Option<SensorHub>>>,
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
