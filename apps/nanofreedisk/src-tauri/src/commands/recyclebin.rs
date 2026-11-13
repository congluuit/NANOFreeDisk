//! The Recycle Bin card (Activity view, later Clean): how much the bin holds per
//! drive, and emptying it through the shell's own API (PLAN §5.1: `$Recycle.Bin`
//! is never touched any other way). Windows only; the UI hides the card on macOS.
//!
//! Emptying is permanent. It lands in the Activity log as an operation of kind
//! `recycle-bin` whose items (one per drive) are `permanent`, so it is never
//! offered for restore, and in the audit log. Items this app moved to the bin
//! earlier stop being "restorable" in the journal as soon as the bin no longer
//! holds them ([`settle`]), whoever emptied it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nfd_core::exec::{self, ItemResult, ItemStatus, Operation};
use nfd_core::plan::DeleteMode;
use nfd_platform::recycle_bin::{self, RecordedItem};
use serde::Serialize;
use tauri::{AppHandle, State};

use super::{AppState, CmdResult, err, with_history};

/// What the bin of one drive holds.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinDrive {
    /// `C:\`
    pub root: String,
    pub bytes: u64,
    pub items: u64,
}

/// What the Recycle Bin holds.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinStatus {
    pub bytes: u64,
    pub items: u64,
    /// Drives whose bin holds something.
    pub drives: Vec<BinDrive>,
    /// Activity log items found gone from the bin just now (the Activity list
    /// should be read again when this is not 0).
    pub settled: u64,
}

/// How emptying went.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyOutcome {
    /// Bytes and items deleted for good.
    pub bytes: u64,
    pub items: u64,
    /// `C:\: message` for drives whose bin could not be emptied.
    pub failed: Vec<String>,
    pub settled: u64,
    pub journal_error: Option<String>,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| err("internal", e))
}

/// Local drives (fixed, removable, RAM disks that are ready).
fn drives() -> Vec<PathBuf> {
    nfd_apps::icons::local_drives()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

/// What each drive's bin holds (drives without a bin or with an empty one left out).
fn per_drive() -> Vec<BinDrive> {
    drives()
        .iter()
        .filter_map(|d| recycle_bin::query(Some(d)).ok())
        .filter(|b| b.items > 0)
        .map(|b| BinDrive {
            root: b
                .root
                .as_deref()
                .map(|r| r.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bytes: b.bytes,
            items: b.items,
        })
        .collect()
}

/// Mark the journal's Recycle Bin items that the bin no longer holds as deleted
/// for good. Drives whose bin cannot be read keep their items as they are.
/// Returns how many items changed.
fn settle(app: &AppHandle, state: &AppState) -> CmdResult<u64> {
    let pending = with_history(app, state, |h| h.recycle_bin_pending())?;
    if pending.is_empty() {
        return Ok(0);
    }
    let mut records: HashMap<String, Option<Vec<RecordedItem>>> = HashMap::new();
    for item in &pending {
        if let Some(drive) = recycle_bin::drive_of(&item.path) {
            records
                .entry(drive.clone())
                .or_insert_with(|| recycle_bin::recorded_items(Path::new(&drive)).ok());
        }
    }
    with_history(app, state, |h| {
        h.settle_recycle_bin(&|path: &Path, started_at: i64| {
            // back where it was (restored from Explorer): not "deleted for good"
            if path.symlink_metadata().is_ok() {
                return None;
            }
            let drive = recycle_bin::drive_of(&path.to_string_lossy())?;
            let recs = records.get(&drive)?.as_ref()?;
            Some(recycle_bin::still_recorded(recs, path, started_at))
        })
    })
}

/// What the Recycle Bin holds, per drive. Also brings the Activity log up to date
/// with what the bin still holds.
#[tauri::command]
pub async fn recyclebin_status(app: AppHandle, state: State<'_, AppState>) -> CmdResult<BinStatus> {
    let (all, drives) = blocking(|| (recycle_bin::query(None), per_drive())).await?;
    let all = all.map_err(|e| err("os", e))?;
    let settled = settle(&app, &state).unwrap_or(0);
    Ok(BinStatus {
        bytes: all.bytes,
        items: all.items,
        drives,
        settled,
    })
}

/// Empty the Recycle Bin of drive `root` (`C:\`), or of every drive. Permanent;
/// the UI asked the user first. Recorded in the Activity and audit logs.
#[tauri::command]
pub async fn recyclebin_empty(
    app: AppHandle,
    state: State<'_, AppState>,
    root: Option<String>,
) -> CmdResult<EmptyOutcome> {
    let started_at = exec::unix_now();
    let targets: Vec<BinDrive> = {
        let all = blocking(per_drive).await?;
        match root.as_deref() {
            None => all,
            Some(r) => {
                let want = recycle_bin::drive_of(r).ok_or_else(|| err("bad-root", r))?;
                all.into_iter()
                    .filter(|d| d.root.eq_ignore_ascii_case(&want))
                    .collect()
            }
        }
    };
    let results = blocking(move || {
        targets
            .into_iter()
            .map(|d| {
                let r = recycle_bin::empty(Some(Path::new(&d.root)));
                (d, r.map_err(|e| e.to_string()))
            })
            .collect::<Vec<_>>()
    })
    .await?;
    let mut out = EmptyOutcome {
        bytes: 0,
        items: 0,
        failed: Vec::new(),
        settled: 0,
        journal_error: None,
    };
    let mut items = Vec::new();
    for (n, (d, r)) in results.into_iter().enumerate() {
        let ok = r.is_ok();
        if ok {
            out.bytes += d.bytes;
            out.items += d.items;
        }
        items.push(ItemResult {
            n,
            path: Path::new(&d.root).join("$Recycle.Bin"),
            rule_id: None,
            mode: DeleteMode::Permanent,
            is_dir: true,
            bytes: d.bytes,
            files: d.items,
            freed: if ok { d.bytes } else { 0 },
            status: if ok {
                ItemStatus::Done
            } else {
                ItemStatus::Failed
            },
            reason: (!ok).then(|| "os".to_owned()),
            message: r.as_ref().err().cloned(),
            stored: None,
            warnings: Vec::new(),
        });
        if let Err(e) = r {
            out.failed.push(format!("{}: {e}", d.root));
        }
    }
    if !items.is_empty() {
        let op = Operation {
            id: exec::new_op_id(started_at),
            kind: "recycle-bin".into(),
            started_at,
            finished_at: exec::unix_now(),
            items,
            cancelled: false,
        };
        out.journal_error = super::clean::journal(&app, &state, &op);
    }
    out.settled = settle(&app, &state).unwrap_or(0);
    Ok(out)
}
