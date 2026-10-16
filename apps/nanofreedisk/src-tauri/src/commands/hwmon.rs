//! Hardware view commands (docs/HARDWARE_MONITOR.md §4): inventory.
//! Hardware access is read-only.

use std::path::Path;

use super::{CmdResult, err};

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
