//! What the uninstallers do with the user's data (`docs/PHASE8_INSTALLERS.md` §2.3).
//!
//! Uninstalling must never destroy files silently: items NANOFreeDisk moved to its
//! quarantine are still the user's files until the 7-day purge, and with the app
//! gone nothing would purge or restore them. So every uninstaller (NSIS, MSI, the
//! branded setup) runs `nanofreedisk.exe --uninstall-cleanup`, which uses this
//! module to count what is still quarantined, then puts it back or deletes it for
//! good as the user chose, and only then removes the data folders.
//!
//! Recycle Bin items are left to Windows; only quarantine-backed items
//! (`quarantine`, `hardlink`, `clone`) are touched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use nfd_core::exec::{self, QuarantineRoots};

use crate::{History, Result};

/// Journal modes whose files sit in a quarantine folder.
pub(crate) const QUARANTINE_MODES: [&str; 3] = ["quarantine", "hardlink", "clone"];

/// Every operation, however old (the journal keeps 90 days).
const ALL: usize = 1_000_000;

/// Quarantined items that can still be restored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pending {
    /// Operation ids with at least one such item.
    pub operations: Vec<String>,
    pub items: u64,
    pub bytes: u64,
}

impl Pending {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items == 0
    }
}

/// Outcome of [`restore_all`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Restored {
    pub restored: u64,
    /// Items that could not be put back (they stay in the quarantine).
    pub failed: Vec<String>,
}

/// Item numbers of `id` that are quarantined and still restorable.
fn quarantined_items(h: &History, id: &str) -> Result<(Vec<u64>, u64)> {
    let (row, items) = h.operation(id)?;
    let mut numbers = Vec::new();
    let mut bytes = 0;
    for item in items.iter().filter(|i| {
        i.restorable(&row.state)
            && i.stored.is_some()
            && QUARANTINE_MODES.contains(&i.mode.as_str())
    }) {
        numbers.push(item.n);
        bytes += item.bytes;
    }
    Ok((numbers, bytes))
}

/// What is still in the quarantine according to the journal.
///
/// # Errors
/// SQLite failure.
pub fn pending(h: &History) -> Result<Pending> {
    let mut out = Pending::default();
    for op in h.operations(ALL)?.into_iter().filter(|o| o.restorable > 0) {
        let (numbers, bytes) = quarantined_items(h, &op.id)?;
        if !numbers.is_empty() {
            out.items += numbers.len() as u64;
            out.bytes += bytes;
            out.operations.push(op.id);
        }
    }
    Ok(out)
}

/// Put every quarantined item back where it was. Recycle Bin items of the same
/// operations are not touched.
///
/// # Errors
/// SQLite failure (file-system failures are listed in [`Restored::failed`]).
pub fn restore_all(h: &mut History) -> Result<Restored> {
    let mut out = Restored::default();
    for id in pending(h)?.operations {
        let (numbers, _) = quarantined_items(h, &id)?;
        for r in h.restore_operation(&id, Some(&numbers))? {
            match r.status {
                exec::RestoreStatus::Restored => out.restored += 1,
                // nothing left to keep for these
                exec::RestoreStatus::Missing | exec::RestoreStatus::NotRestorable => {}
                exec::RestoreStatus::Conflicts
                | exec::RestoreStatus::Failed
                | exec::RestoreStatus::NeedsAdmin => {
                    out.failed.push(r.path);
                }
            }
        }
    }
    Ok(out)
}

/// Delete everything in the quarantine for good: every operation the journal
/// knows plus operation folders it does not (a cleared database). Returns the
/// number of operations purged.
///
/// # Errors
/// SQLite failure.
pub fn purge_all(h: &mut History, roots: &QuarantineRoots) -> Result<usize> {
    let mut n = 0;
    for id in pending(h)?.operations {
        if h.purge_operation(&id).is_ok() {
            n += 1;
        }
    }
    // a cutoff in the future catches the folders the journal does not know
    let now = exec::unix_now();
    n += h.purge_expired(roots, now + 2 * 86_400, Some(0))?.len();
    Ok(n)
}

/// Remove `<drive>\.nfd-quarantine` folders that are empty (nothing quarantined is
/// left on that drive). Folders that still hold something are kept.
pub fn remove_empty_volume_quarantines(roots: &QuarantineRoots) {
    for root in roots.existing() {
        if Some(&root) == roots.system.as_ref() {
            continue;
        }
        let empty = fs::read_dir(&root).is_ok_and(|mut rd| rd.next().is_none());
        if empty {
            let _ = fs::remove_dir(&root);
        }
    }
}

/// Delete a data folder, except `keep` inside it (a quarantine that still holds
/// files the user did not get back). Missing folders are fine.
///
/// # Errors
/// The first entry that could not be removed (the others are still removed).
pub fn remove_data_dir(dir: &Path, keep: Option<&Path>) -> io::Result<()> {
    let Some(keep) = keep.filter(|k| k.starts_with(dir) && k.exists()) else {
        return match fs::remove_dir_all(dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    };
    let mut first_err = None;
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if keep.starts_with(&path) {
            if path != keep && path.is_dir() {
                if let Err(e) = remove_data_dir(&path, Some(keep)) {
                    first_err.get_or_insert(e);
                }
            }
            continue;
        }
        let res = if entry.file_type().is_ok_and(|t| t.is_dir()) {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        if let Err(e) = res {
            first_err.get_or_insert(e);
        }
    }
    first_err.map_or(Ok(()), Err)
}

/// The app's data folders in installed mode: `%LOCALAPPDATA%\<APP_ID>` (history,
/// quarantine of the system drive, logs, WebView2 data), `%APPDATA%\<APP_ID>`
/// (settings) and `%LOCALAPPDATA%\NANOFreeDisk\pawnio` (PawnIO modules the user
/// downloaded; the PawnIO driver itself is shared with other apps and never
/// touched). In portable mode, the single `data` folder next to the exe.
#[must_use]
pub fn data_dirs() -> Vec<PathBuf> {
    if let Some(dir) = crate::portable_dir() {
        return vec![dir];
    }
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let mut dirs: Vec<PathBuf> = ["LOCALAPPDATA", "APPDATA"]
        .iter()
        .filter_map(|k| env(k))
        .map(|base| base.join(crate::APP_ID))
        .collect();
    if let Some(local) = env("LOCALAPPDATA") {
        dirs.push(local.join("NANOFreeDisk").join("pawnio"));
    }
    dirs
}

/// After [`remove_data_dir`] on the PawnIO module folder, its parent
/// `%LOCALAPPDATA%\NANOFreeDisk` goes too when nothing else is in it.
pub fn remove_empty_parent(dir: &Path) {
    if let Some(parent) = dir.parent() {
        let empty = fs::read_dir(parent).is_ok_and(|mut rd| rd.next().is_none());
        if empty {
            let _ = fs::remove_dir(parent);
        }
    }
}
