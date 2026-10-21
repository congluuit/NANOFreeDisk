//! The Executor and the Quarantine (PHASE3_CLEAN §2.3): carries out a [`CleanPlan`]
//! that the [`SafetyGate`] checks again, item by item, right before acting.
//!
//! * **Quarantine** (rule matches): the item is *renamed* — never copied — into
//!   `<quarantine root>\<op id>\<n>\<name>` with a line in `manifest.jsonl`, so
//!   [`restore`] can put it back with the same bytes and timestamps. On the system
//!   drive the root is the app's own local data folder (already private to the user);
//!   on other NTFS/ReFS drives it is a hidden private `<volume>\.nfd-quarantine`.
//!   Elsewhere (FAT/exFAT, network) or when the rename crosses a volume, the item
//!   goes to the Recycle Bin instead. A folder with files in use is moved file by file
//!   and the busy files stay where they are (status [`ItemStatus::Partial`]).
//! * **Recycle Bin** (hand-picked items): the Windows shell; restored by reading the
//!   bin's `$I` records (`nfd_platform::trash::find_recycled`).
//! * **Permanent**: removed for good (Expert, 🟢 Safe only — decided by the planner).
//! * **Hard link / block clone** (Duplicates): a link to the copy that stays is made
//!   under a temporary name next to the duplicate, the duplicate is moved to the
//!   quarantine, then the link takes its name. A failure at any step undoes the
//!   steps before it, so no file is ever lost. [`restore`] puts the original back
//!   and sends the link to the Recycle Bin (or simply removes a hard link whose data
//!   still has another name).
//!
//! One item failing never stops the plan; the caller can cancel between items.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::plan::{CleanPlan, DeleteMode, PlanItem};
use crate::safety::gate::{Probe, Warning};
use crate::safety::{SafetyGate, comparable, is_within};

/// Name of the quarantine folder at the root of non-system volumes.
pub const VOLUME_QUARANTINE_DIR: &str = ".nfd-quarantine";
/// Days a quarantined operation is kept before it is purged.
pub const QUARANTINE_TTL_DAYS: i64 = 7;

/// Where quarantined items go.
#[derive(Debug, Clone, Default)]
pub struct QuarantineRoots {
    /// Quarantine folder on the system volume (inside the app's local data folder).
    pub system: Option<PathBuf>,
    /// Quarantine on other volumes too (`<volume>\.nfd-quarantine`); tests turn it off.
    pub other_volumes: bool,
}

impl QuarantineRoots {
    /// `app_local` = the app's local data folder (`%LOCALAPPDATA%\<app id>`).
    #[must_use]
    pub fn new(app_local: Option<&Path>) -> Self {
        Self {
            system: app_local.map(|d| d.join("quarantine")),
            other_volumes: true,
        }
    }

    /// The quarantine root for `item`, or `None` when it must go to the Recycle Bin.
    #[must_use]
    pub fn root_for(&self, item: &Path) -> Option<PathBuf> {
        let item_root = volume_key(item)?;
        if let Some(sys) = &self.system {
            if volume_key(sys).as_deref() == Some(item_root.as_str()) {
                return Some(sys.clone());
            }
        }
        if !self.other_volumes {
            return None;
        }
        let vol = nfd_platform::volumes::list()
            .into_iter()
            .find(|v| volume_key(&v.root).as_deref() == Some(item_root.as_str()))?;
        let fs_ok = ["NTFS", "ReFS"]
            .iter()
            .any(|f| vol.fs.eq_ignore_ascii_case(f));
        let kind_ok = matches!(
            vol.kind,
            nfd_platform::volumes::VolumeKind::Fixed | nfd_platform::volumes::VolumeKind::Removable
        );
        (fs_ok && kind_ok && vol.ready).then(|| vol.root.join(VOLUME_QUARANTINE_DIR))
    }

    /// Every quarantine root that exists on this machine.
    #[must_use]
    pub fn existing(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.system.iter().filter(|p| p.is_dir()).cloned().collect();
        if self.other_volumes {
            for v in nfd_platform::volumes::list() {
                let q = v.root.join(VOLUME_QUARANTINE_DIR);
                if v.ready && v.kind != nfd_platform::volumes::VolumeKind::Network && q.is_dir() {
                    out.push(q);
                }
            }
        }
        out
    }
}

/// `c:` / `//server/share` of an absolute path.
fn volume_key(path: &Path) -> Option<String> {
    let c = comparable(path)?;
    if let Some(rest) = c.strip_prefix("//") {
        let mut it = rest.splitn(3, '/');
        Some(format!("//{}/{}", it.next()?, it.next()?))
    } else {
        c.split('/').next().map(str::to_owned)
    }
}

/// Current time in Unix seconds.
#[must_use]
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// A new operation id: `YYYYMMDD-HHMMSS-xxxx` (UTC, sortable, safe as a folder name).
#[must_use]
pub fn new_op_id(now: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(now);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let salt = (nanos ^ std::process::id().rotate_left(16)) & 0xffff;
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}-{salt:04x}")
}

/// Unix seconds → (year, month, day, hour, minute, second) in UTC.
#[must_use]
pub fn civil(t: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    // Howard Hinnant's days_from_civil inverse.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        m,
        d,
        (secs / 3600) as u32,
        ((secs % 3600) / 60) as u32,
        (secs % 60) as u32,
    )
}

/// What happened to one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemStatus {
    Done,
    /// Some files were in use and stayed where they were.
    Partial,
    /// The SafetyGate refused it (see `reason`).
    Skipped,
    /// The user declined the shell's "delete permanently?" prompt.
    Declined,
    Failed,
    /// Not selected in the plan.
    NotSelected,
    /// The operation was cancelled before this item.
    Cancelled,
}

impl ItemStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Partial => "partial",
            Self::Skipped => "skipped",
            Self::Declined => "declined",
            Self::Failed => "failed",
            Self::NotSelected => "not-selected",
            Self::Cancelled => "cancelled",
        }
    }

    /// Something left the disk.
    #[must_use]
    pub const fn removed_something(self) -> bool {
        matches!(self, Self::Done | Self::Partial)
    }
}

/// Result of one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub n: usize,
    pub path: PathBuf,
    pub rule_id: Option<String>,
    /// The mode actually used (a quarantine may fall back to the Recycle Bin).
    pub mode: DeleteMode,
    pub is_dir: bool,
    /// Bytes planned.
    pub bytes: u64,
    pub files: u64,
    /// Bytes that left the disk (estimate for partial moves).
    pub freed: u64,
    pub status: ItemStatus,
    /// Blocked code (`system-dir`, `changed`…) or error kind.
    pub reason: Option<String>,
    pub message: Option<String>,
    /// Where a quarantined item is now.
    pub stored: Option<PathBuf>,
    #[serde(default)]
    pub warnings: Vec<Warning>,
}

/// A finished (or cancelled) operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub id: String,
    /// `clean` (rules) or `manual` (picked by hand).
    pub kind: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub items: Vec<ItemResult>,
    pub cancelled: bool,
}

impl Operation {
    #[must_use]
    pub fn freed(&self) -> u64 {
        self.items.iter().map(|i| i.freed).sum()
    }

    #[must_use]
    pub fn count(&self, status: ItemStatus) -> usize {
        self.items.iter().filter(|i| i.status == status).count()
    }
}

/// Executor options.
#[derive(Debug, Clone)]
pub struct ExecOptions {
    pub op_id: String,
    pub kind: String,
    /// Native window for the shell's prompts (0 = none).
    pub owner: isize,
    pub roots: QuarantineRoots,
    pub now: i64,
}

/// One line of `manifest.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestLine {
    pub n: usize,
    pub original: PathBuf,
    pub stored: PathBuf,
    pub is_dir: bool,
    pub bytes: u64,
    pub files: u64,
    pub rule_id: Option<String>,
    pub at: i64,
}

/// Run `plan`. `progress(done, total, result)` is called after every item and
/// returns `false` to cancel the rest.
pub fn execute(
    plan: &CleanPlan,
    gate: &SafetyGate,
    probe: &dyn Probe,
    opts: &ExecOptions,
    progress: &mut dyn FnMut(usize, usize, &ItemResult) -> bool,
) -> Operation {
    execute_verified(plan, gate, probe, opts, &mut |_| Ok(()), progress)
}

/// Why a feature-specific check refused an item right before it was acted on
/// (stable `code` for the journal and the UI, and a message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
}

/// [`execute`] with one more check per item, run after the SafetyGate and
/// immediately before the item is touched (Duplicates: the keeper and the copy
/// still hold the same bytes). A refused item is skipped with the refusal's code.
pub fn execute_verified(
    plan: &CleanPlan,
    gate: &SafetyGate,
    probe: &dyn Probe,
    opts: &ExecOptions,
    verify: &mut dyn FnMut(&PlanItem) -> Result<(), Refusal>,
    progress: &mut dyn FnMut(usize, usize, &ItemResult) -> bool,
) -> Operation {
    let total = plan.items.len();
    let keys: Vec<Option<String>> = plan.items.iter().map(|i| comparable(&i.path)).collect();
    let mut op = Operation {
        id: opts.op_id.clone(),
        kind: opts.kind.clone(),
        started_at: opts.now,
        finished_at: opts.now,
        items: Vec::with_capacity(total),
        cancelled: false,
    };
    let mut op_dirs: Vec<(PathBuf, fs::File)> = Vec::new();
    for (n, item) in plan.items.iter().enumerate() {
        let mut result = ItemResult {
            n,
            path: item.path.clone(),
            rule_id: item.rule_id.clone(),
            mode: item.mode,
            is_dir: item.is_dir,
            bytes: item.bytes,
            files: item.files,
            freed: 0,
            status: ItemStatus::NotSelected,
            reason: None,
            message: None,
            stored: None,
            warnings: Vec::new(),
        };
        if op.cancelled {
            result.status = ItemStatus::Cancelled;
        } else if item.selected {
            let nested = keys[n].as_deref().is_some_and(|me| {
                plan.items.iter().enumerate().any(|(m, other)| {
                    m != n
                        && other.selected
                        && keys[m]
                            .as_deref()
                            .is_some_and(|o| (o != me && is_within(me, o)) || (o == me && m < n))
                })
            });
            if nested {
                result.status = ItemStatus::Skipped;
                result.reason = Some("nested".into());
            } else {
                match gate.check_live(&item.path, item.origin, Some(item.expected()), probe) {
                    Err(blocked) => {
                        result.status = ItemStatus::Skipped;
                        result.reason = Some(blocked.code().into());
                        result.message = Some(blocked.to_string());
                    }
                    Ok(warnings) => {
                        result.warnings = warnings;
                        match verify(item) {
                            Ok(()) => act(item, &mut result, opts, &mut op_dirs),
                            Err(refusal) => {
                                result.status = ItemStatus::Skipped;
                                result.reason = Some(refusal.code.into());
                                result.message = Some(refusal.message);
                            }
                        }
                    }
                }
            }
        }
        op.items.push(result);
        let last = op.items.last().expect("just pushed");
        if !progress(n + 1, total, last) {
            op.cancelled = true;
        }
    }
    op.finished_at = unix_now();
    op
}

fn act(
    item: &PlanItem,
    result: &mut ItemResult,
    opts: &ExecOptions,
    op_dirs: &mut Vec<(PathBuf, fs::File)>,
) {
    let fail = |result: &mut ItemResult, e: &io::Error| {
        result.status = ItemStatus::Failed;
        result.reason = Some(format!("{:?}", e.kind()).to_lowercase());
        result.message = Some(e.to_string());
    };
    match item.mode {
        DeleteMode::Permanent => {
            let r = if item.is_dir {
                fs::remove_dir_all(&item.path)
            } else {
                fs::remove_file(&item.path)
            };
            match r {
                Ok(()) => {
                    result.status = ItemStatus::Done;
                    result.freed = item.bytes;
                }
                Err(e) => fail(result, &e),
            }
        }
        DeleteMode::RecycleBin => recycle(item, result, opts),
        DeleteMode::Registry => {
            // registry leftovers are deleted (after their `.reg` backup) by the
            // uninstaller, never through a file plan
            result.status = ItemStatus::Failed;
            result.reason = Some("registry".into());
        }
        DeleteMode::Hardlink | DeleteMode::Clone => {
            let Some(keeper) = item.keeper.as_deref() else {
                result.status = ItemStatus::Failed;
                result.reason = Some("no-keeper".into());
                return;
            };
            let Some(root) = opts.roots.root_for(&item.path) else {
                result.status = ItemStatus::Failed;
                result.reason = Some("no-quarantine".into());
                return;
            };
            let make: &dyn Fn(&Path, &Path) -> io::Result<()> = if item.mode == DeleteMode::Clone {
                &|from, to| nfd_platform::link::clone_file(from, to)
            } else {
                &|from, to| {
                    let links = nfd_platform::fsinfo::file_info(from)?.link_count;
                    if links.is_some_and(|n| n >= nfd_platform::link::MAX_HARD_LINKS) {
                        return Err(io::Error::other("too many hard links"));
                    }
                    nfd_platform::link::hard_link(from, to)
                }
            };
            match replace_with_link(
                item,
                result.n,
                keeper,
                &root,
                opts,
                op_dirs,
                make,
                &|a, b| fs::rename(a, b),
            ) {
                Ok(stored) => {
                    result.stored = Some(stored);
                    result.status = ItemStatus::Done;
                    result.freed = item.bytes;
                }
                Err(e) => fail(result, &e),
            }
        }
        DeleteMode::Quarantine => {
            let Some(root) = opts.roots.root_for(&item.path) else {
                recycle(item, result, opts);
                return;
            };
            match quarantine(item, result.n, &root, opts, op_dirs) {
                Ok((stored, moved, complete)) => {
                    result.stored = Some(stored);
                    if complete {
                        result.status = ItemStatus::Done;
                        result.freed = item.bytes;
                    } else if moved > 0 {
                        result.status = ItemStatus::Partial;
                        result.freed = moved.min(item.bytes);
                        result.reason = Some("in-use".into());
                    } else {
                        result.status = ItemStatus::Failed;
                        result.reason = Some("in-use".into());
                        result.stored = None;
                    }
                }
                Err(e) if is_cross_device(&e) => recycle(item, result, opts),
                Err(e) => fail(result, &e),
            }
        }
    }
}

fn is_cross_device(e: &io::Error) -> bool {
    // ERROR_NOT_SAME_DEVICE (17) on Windows, EXDEV (18) on Unix.
    (cfg!(windows) && e.raw_os_error() == Some(17)) || (cfg!(unix) && e.raw_os_error() == Some(18))
}

fn recycle(item: &PlanItem, result: &mut ItemResult, opts: &ExecOptions) {
    result.mode = DeleteMode::RecycleBin;
    match nfd_platform::trash::recycle(&item.path, opts.owner) {
        Ok(nfd_platform::trash::Recycled::Done) => {
            result.status = ItemStatus::Done;
            result.freed = item.bytes;
        }
        Ok(nfd_platform::trash::Recycled::Declined) => result.status = ItemStatus::Declined,
        Err(e) => {
            result.status = ItemStatus::Failed;
            result.reason = Some(format!("{:?}", e.kind()).to_lowercase());
            result.message = Some(e.to_string());
        }
    }
}

/// Move `item` into `<root>/<op>/<n>/<name>`. Returns (stored path, bytes moved,
/// everything moved).
fn quarantine(
    item: &PlanItem,
    n: usize,
    root: &Path,
    opts: &ExecOptions,
    op_dirs: &mut Vec<(PathBuf, fs::File)>,
) -> io::Result<(PathBuf, u64, bool)> {
    let op_dir = root.join(&opts.op_id);
    let manifest = match op_dirs.iter_mut().find(|(d, _)| *d == op_dir) {
        Some((_, f)) => f,
        None => {
            if let Some(parent) = root.parent() {
                fs::create_dir_all(parent)?;
            }
            let hidden = root.file_name().is_some_and(|n| n == VOLUME_QUARANTINE_DIR);
            nfd_platform::private_dir::create(root, hidden)?;
            fs::create_dir_all(&op_dir)?;
            let file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(op_dir.join("manifest.jsonl"))?;
            op_dirs.push((op_dir.clone(), file));
            &mut op_dirs.last_mut().expect("just pushed").1
        }
    };
    let slot = op_dir.join(n.to_string());
    fs::create_dir(&slot)?;
    let name = item
        .path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let stored = slot.join(name);
    let (moved, complete) = match fs::rename(&item.path, &stored) {
        Ok(()) => (item.bytes, true),
        Err(e) if is_cross_device(&e) => {
            let _ = fs::remove_dir(&slot);
            return Err(e);
        }
        Err(e) if item.is_dir => {
            // something inside is busy: move what can be moved
            let _ = e;
            move_tree(&item.path, &stored)?
        }
        Err(e) => {
            let _ = fs::remove_dir(&slot);
            return Err(e);
        }
    };
    if moved > 0 || complete {
        let line = ManifestLine {
            n,
            original: item.path.clone(),
            stored: stored.clone(),
            is_dir: item.is_dir,
            bytes: item.bytes,
            files: item.files,
            rule_id: item.rule_id.clone(),
            at: opts.now,
        };
        let text = serde_json::to_string(&line).map_err(io::Error::other)?;
        writeln!(manifest, "{text}")?;
        manifest.flush()?;
    } else {
        let _ = fs::remove_dir_all(&slot);
    }
    Ok((stored, moved, complete))
}

/// Replace the file `item` by a link to `keeper` (see the module documentation):
/// `make(keeper, tmp)` creates the link under a temporary name, the original moves
/// into the quarantine (with its manifest line), then `finish(tmp, item)` gives the
/// link the original name. Returns where the original is stored. Any failure undoes
/// what was done before it.
#[allow(clippy::too_many_arguments)]
fn replace_with_link(
    item: &PlanItem,
    n: usize,
    keeper: &Path,
    root: &Path,
    opts: &ExecOptions,
    op_dirs: &mut Vec<(PathBuf, fs::File)>,
    make: &dyn Fn(&Path, &Path) -> io::Result<()>,
    finish: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<PathBuf> {
    if item.is_dir {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a file"));
    }
    let parent = item
        .path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent folder"))?;
    let name = item
        .path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".nfd-{}-{n}.tmp", opts.op_id));
    let tmp = parent.join(tmp_name);
    make(keeper, &tmp)?;
    let (stored, _, complete) = match quarantine(item, n, root, opts, op_dirs) {
        Ok(r) if r.2 => r,
        Ok(r) => {
            let _ = fs::rename(&r.0, &item.path);
            let _ = fs::remove_file(&tmp);
            return Err(io::Error::other("the duplicate could not be moved"));
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    };
    debug_assert!(complete);
    if let Err(e) = finish(&tmp, &item.path) {
        // put the original back where it was; the link was only a second name
        if fs::rename(&stored, &item.path).is_ok() {
            let _ = fs::remove_file(&tmp);
        }
        return Err(e);
    }
    Ok(stored)
}

/// Move the contents of directory `src` into `dst` one entry at a time (links are
/// moved as links, never followed), then remove `src` if it ended up empty. Returns
/// (bytes moved, everything moved).
fn move_tree(src: &Path, dst: &Path) -> io::Result<(u64, bool)> {
    match fs::create_dir(dst) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && dst.is_dir() => {}
        Err(e) => return Err(e),
    }
    let mut moved = 0;
    let mut complete = true;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let meta = fs::symlink_metadata(&from)?;
        let size = if meta.is_file() { meta.len() } else { 0 };
        if to.symlink_metadata().is_err() && fs::rename(&from, &to).is_ok() {
            moved += if meta.is_dir() { dir_size(&to) } else { size };
            continue;
        }
        if meta.is_dir() && !meta.file_type().is_symlink() {
            let (m, c) = move_tree(&from, &to)?;
            moved += m;
            complete &= c;
        } else {
            complete = false;
        }
    }
    if complete {
        complete = fs::remove_dir(src).is_ok();
    }
    Ok((moved, complete))
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

/// Outcome of restoring one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestoreStatus {
    Restored,
    /// Restored, but some names already existed again and were left in the
    /// quarantine.
    Conflicts,
    /// Nothing to restore (purged, emptied from the bin, or never moved).
    Missing,
    /// Permanent deletions cannot be undone.
    NotRestorable,
    /// Putting it back needs administrator rights (an all-users leftover: a folder
    /// of Program Files / ProgramData, an `HKEY_LOCAL_MACHINE` key).
    NeedsAdmin,
    Failed,
}

/// Put one item back. `deleted_after` (Unix s) narrows Recycle Bin lookups to this
/// operation.
#[must_use]
pub fn restore(
    original: &Path,
    stored: Option<&Path>,
    mode: DeleteMode,
    deleted_after: i64,
) -> (RestoreStatus, Option<String>) {
    match mode {
        DeleteMode::Permanent => (RestoreStatus::NotRestorable, None),
        DeleteMode::Hardlink | DeleteMode::Clone => restore_linked(original, stored),
        DeleteMode::Registry => {
            let Some(stored) = stored.filter(|s| s.is_file()) else {
                return (RestoreStatus::Missing, None);
            };
            match nfd_platform::registry::import(stored) {
                Ok(()) => (RestoreStatus::Restored, None),
                Err(e) => failed(&e),
            }
        }
        DeleteMode::RecycleBin => {
            let found = match nfd_platform::trash::find_recycled(original) {
                Ok(f) => f,
                Err(e) => return failed(&e),
            };
            let Some(item) = found.iter().find(|i| i.deleted_at >= deleted_after - 60) else {
                return (RestoreStatus::Missing, None);
            };
            match nfd_platform::trash::restore_recycled(item) {
                Ok(()) => (RestoreStatus::Restored, None),
                Err(e) => failed(&e),
            }
        }
        DeleteMode::Quarantine => {
            let Some(stored) = stored else {
                return (RestoreStatus::Missing, None);
            };
            if stored.symlink_metadata().is_err() {
                return (RestoreStatus::Missing, None);
            }
            if let Some(parent) = original.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return failed(&e);
                }
            }
            let result = if original.symlink_metadata().is_err() {
                fs::rename(stored, original).map(|()| true)
            } else if stored.is_dir() && original.is_dir() {
                move_tree(stored, original).map(|(_, complete)| complete)
            } else {
                Ok(false)
            };
            match result {
                Ok(true) => {
                    // drop the empty slot folder
                    if let Some(slot) = stored.parent() {
                        let _ = fs::remove_dir(slot);
                    }
                    (RestoreStatus::Restored, None)
                }
                Ok(false) => (RestoreStatus::Conflicts, None),
                Err(e) => failed(&e),
            }
        }
    }
}

/// A restore that failed: putting an all-users leftover back into Program Files /
/// ProgramData / `HKEY_LOCAL_MACHINE` needs administrator rights.
fn failed(e: &io::Error) -> (RestoreStatus, Option<String>) {
    let status = if e.kind() == io::ErrorKind::PermissionDenied {
        RestoreStatus::NeedsAdmin
    } else {
        RestoreStatus::Failed
    };
    (status, Some(e.to_string()))
}

/// Undo a replaced duplicate: the link steps aside, the original comes back from
/// the quarantine, and the link leaves — removed outright when it is a hard link
/// whose data still has another name (nothing is lost), else to the Recycle Bin.
fn restore_linked(original: &Path, stored: Option<&Path>) -> (RestoreStatus, Option<String>) {
    let Some(stored) = stored else {
        return (RestoreStatus::Missing, None);
    };
    if stored.symlink_metadata().is_err() {
        return (RestoreStatus::Missing, None);
    }
    let aside = original.symlink_metadata().is_ok().then(|| {
        let mut name = original.as_os_str().to_owned();
        name.push(format!(".nfd-restore-{}", unix_now()));
        PathBuf::from(name)
    });
    if let Some(aside) = &aside {
        if let Err(e) = fs::rename(original, aside) {
            return (RestoreStatus::Failed, Some(e.to_string()));
        }
    }
    if let Err(e) = fs::rename(stored, original) {
        if let Some(aside) = &aside {
            let _ = fs::rename(aside, original);
        }
        return (RestoreStatus::Failed, Some(e.to_string()));
    }
    if let Some(slot) = stored.parent() {
        let _ = fs::remove_dir(slot);
    }
    let Some(aside) = aside else {
        return (RestoreStatus::Restored, None);
    };
    let shared = nfd_platform::fsinfo::file_info(&aside)
        .ok()
        .and_then(|i| i.link_count)
        .is_some_and(|n| n >= 2);
    let gone = if shared {
        fs::remove_file(&aside).is_ok()
    } else {
        matches!(
            nfd_platform::trash::recycle(&aside, 0),
            Ok(nfd_platform::trash::Recycled::Done)
        )
    };
    if gone {
        (RestoreStatus::Restored, None)
    } else {
        (
            RestoreStatus::Restored,
            Some(format!("the link was left at {}", aside.display())),
        )
    }
}

/// Delete one operation's quarantine folder for good (TTL expired or "Empty").
///
/// # Errors
/// The folder could not be removed completely.
pub fn purge_op_dir(dir: &Path) -> io::Result<()> {
    match fs::remove_dir_all(dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

/// Read an operation folder's `manifest.jsonl` (lines that do not parse are
/// skipped).
#[must_use]
pub fn read_manifest(op_dir: &Path) -> Vec<ManifestLine> {
    fs::read_to_string(op_dir.join("manifest.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RiskLevel;
    use crate::safety::gate::{LiveProbe, Origin};

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_792_150_000), (2026, 10, 16, 11, 26, 40));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
        let id = new_op_id(1_792_150_000);
        assert!(id.starts_with("20261016-112640-"), "{id}");
        assert_eq!(id.len(), 20);
    }

    fn setup() -> (tempfile::TempDir, SafetyGate, ExecOptions) {
        let dir = tempfile::tempdir().expect("tmp");
        let base = dir.path().to_string_lossy().replace('\\', "/");
        let resolve = move |v: &str| (v == "LOCALAPPDATA").then(|| base.clone());
        let gate = SafetyGate::new(&resolve);
        let opts = ExecOptions {
            op_id: "20261016-000000-abcd".into(),
            kind: "clean".into(),
            owner: 0,
            roots: QuarantineRoots {
                system: Some(dir.path().join("app").join("quarantine")),
                other_volumes: false,
            },
            now: unix_now(),
        };
        (dir, gate, opts)
    }

    fn item(path: &Path, level: RiskLevel) -> PlanItem {
        let meta = fs::symlink_metadata(path).expect("meta");
        let info = nfd_platform::fsinfo::file_info(path).expect("info");
        PlanItem {
            path: path.to_path_buf(),
            origin: Origin::Rule { level },
            rule_id: Some("t.rule".into()),
            is_dir: meta.is_dir(),
            bytes: if meta.is_dir() { 0 } else { meta.len() },
            files: 1,
            size: if meta.is_dir() { 0 } else { info.logical },
            mtime: info.mtime,
            requires_closed: Vec::new(),
            mode: DeleteMode::Quarantine,
            selected: true,
            keeper: None,
        }
    }

    #[test]
    fn quarantine_and_restore_keep_bytes_and_times() {
        let (dir, gate, opts) = setup();
        let cache = dir.path().join("Cache");
        fs::create_dir_all(cache.join("sub")).expect("mkdir");
        fs::write(cache.join("sub/a.bin"), b"abc").expect("write");
        let file = dir.path().join("old.log");
        fs::write(&file, b"log line").expect("write");
        let before = fs::metadata(&file)
            .expect("meta")
            .modified()
            .expect("mtime");

        let plan = CleanPlan {
            items: vec![item(&cache, RiskLevel::Safe), item(&file, RiskLevel::Safe)],
        };
        let mut seen = 0;
        let op = execute(&plan, &gate, &LiveProbe, &opts, &mut |done, total, _| {
            seen = done;
            assert_eq!(total, 2);
            true
        });
        assert_eq!(seen, 2);
        assert_eq!(op.count(ItemStatus::Done), 2, "{op:#?}");
        assert!(!cache.exists() && !file.exists());
        let op_dir = opts.roots.system.as_ref().expect("root").join(&opts.op_id);
        let manifest = read_manifest(&op_dir);
        assert_eq!(manifest.len(), 2);

        for r in &op.items {
            let (status, msg) = restore(&r.path, r.stored.as_deref(), r.mode, op.started_at);
            assert_eq!(status, RestoreStatus::Restored, "{msg:?}");
        }
        assert_eq!(fs::read(cache.join("sub/a.bin")).expect("read"), b"abc");
        assert_eq!(fs::read(&file).expect("read"), b"log line");
        let after = fs::metadata(&file)
            .expect("meta")
            .modified()
            .expect("mtime");
        assert_eq!(before, after);
        purge_op_dir(&op_dir).expect("purge");
        assert!(!op_dir.exists());
    }

    #[test]
    fn changed_and_unselected_items_are_left_alone() {
        let (dir, gate, opts) = setup();
        let a = dir.path().join("a.tmp");
        let b = dir.path().join("b.tmp");
        fs::write(&a, b"1").expect("write");
        fs::write(&b, b"2").expect("write");
        let mut ia = item(&a, RiskLevel::Safe);
        ia.size = 999; // planned with another size
        let mut ib = item(&b, RiskLevel::Safe);
        ib.selected = false;
        let op = execute(
            &CleanPlan {
                items: vec![ia, ib],
            },
            &gate,
            &LiveProbe,
            &opts,
            &mut |_, _, _| true,
        );
        assert_eq!(op.items[0].status, ItemStatus::Skipped);
        assert_eq!(op.items[0].reason.as_deref(), Some("changed"));
        assert_eq!(op.items[1].status, ItemStatus::NotSelected);
        assert!(a.exists() && b.exists());
    }

    #[test]
    fn protected_and_container_paths_never_move() {
        let (dir, gate, opts) = setup();
        // the LOCALAPPDATA container itself
        let mut whole = item(dir.path(), RiskLevel::Safe);
        whole.is_dir = true;
        let mut prot = item(dir.path(), RiskLevel::Protected);
        prot.path = dir.path().join("x");
        let op = execute(
            &CleanPlan {
                items: vec![whole, prot],
            },
            &gate,
            &LiveProbe,
            &opts,
            &mut |_, _, _| true,
        );
        assert!(
            op.items.iter().all(|i| i.status == ItemStatus::Skipped),
            "{op:#?}"
        );
        assert!(dir.path().exists());
    }

    #[test]
    fn cancel_stops_after_the_current_item() {
        let (dir, gate, opts) = setup();
        let mut items = Vec::new();
        for n in 0..3 {
            let p = dir.path().join(format!("{n}.tmp"));
            fs::write(&p, b"x").expect("write");
            items.push(item(&p, RiskLevel::Safe));
        }
        let op = execute(
            &CleanPlan { items },
            &gate,
            &LiveProbe,
            &opts,
            &mut |done, _, _| done < 1,
        );
        assert!(op.cancelled);
        assert_eq!(op.items[0].status, ItemStatus::Done);
        assert_eq!(op.items[1].status, ItemStatus::Cancelled);
        assert_eq!(op.items[2].status, ItemStatus::Cancelled);
    }

    #[cfg(windows)]
    #[test]
    fn busy_files_stay_and_the_rest_moves() {
        let (dir, gate, opts) = setup();
        let cache = dir.path().join("Busy");
        fs::create_dir_all(&cache).expect("mkdir");
        fs::write(cache.join("free.bin"), b"free").expect("write");
        let busy = cache.join("busy.bin");
        fs::write(&busy, b"busy").expect("write");
        // opened like most apps do, without FILE_SHARE_DELETE: it cannot be moved
        use std::os::windows::fs::OpenOptionsExt;
        let handle = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&busy)
            .expect("open");
        let op = execute(
            &CleanPlan {
                items: vec![item(&cache, RiskLevel::Safe)],
            },
            &gate,
            &LiveProbe,
            &opts,
            &mut |_, _, _| true,
        );
        let r = &op.items[0];
        assert_eq!(r.status, ItemStatus::Partial, "{r:#?}");
        assert!(busy.exists());
        assert!(!cache.join("free.bin").exists());
        drop(handle);
        let (status, _) = restore(&r.path, r.stored.as_deref(), r.mode, op.started_at);
        assert_eq!(status, RestoreStatus::Restored);
        assert!(cache.join("free.bin").exists());
    }

    #[test]
    fn volume_keys() {
        if cfg!(windows) {
            assert_eq!(volume_key(Path::new(r"C:\x\y")).as_deref(), Some("c:"));
            assert_eq!(
                volume_key(Path::new(r"\\nas\share\x")).as_deref(),
                Some("//nas/share")
            );
        }
        assert_eq!(volume_key(Path::new("relative")), None);
    }

    /// A duplicate `dup` of `keeper` (same bytes), as the Duplicates planner makes it.
    fn linked_item(dup: &Path, keeper: &Path, mode: DeleteMode) -> PlanItem {
        let mut i = item(dup, RiskLevel::Safe);
        i.origin = Origin::Duplicate { group: 1 };
        i.rule_id = None;
        i.mode = mode;
        i.keeper = Some(keeper.to_path_buf());
        i
    }

    fn links(p: &Path) -> Option<u32> {
        nfd_platform::fsinfo::file_info(p).expect("info").link_count
    }

    #[test]
    fn hard_link_replace_and_restore() {
        let (dir, gate, opts) = setup();
        let keeper = dir.path().join("photo.jpg");
        let dup = dir.path().join("photo (copy).jpg");
        let bytes: Vec<u8> = (0..50_000u32).map(|i| (i % 253) as u8).collect();
        fs::write(&keeper, &bytes).expect("write");
        fs::write(&dup, &bytes).expect("write");
        let before = fs::metadata(&dup).expect("meta").modified().expect("mtime");
        let plan = CleanPlan {
            items: vec![linked_item(&dup, &keeper, DeleteMode::Hardlink)],
        };
        let op = execute(&plan, &gate, &LiveProbe, &opts, &mut |_, _, _| true);
        let r = &op.items[0];
        assert_eq!(r.status, ItemStatus::Done, "{r:#?}");
        assert_eq!(r.mode, DeleteMode::Hardlink);
        assert_eq!(fs::read(&dup).expect("read"), bytes);
        if links(&dup).is_some() {
            assert_eq!(links(&dup), Some(2), "one file, two names");
        }
        let stored = r.stored.clone().expect("stored");
        assert_eq!(fs::read(&stored).expect("original kept"), bytes);
        // no temporary name left behind
        assert!(
            fs::read_dir(dir.path())
                .expect("ls")
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp"))
        );

        let (status, msg) = restore(&r.path, r.stored.as_deref(), r.mode, op.started_at);
        assert_eq!(status, RestoreStatus::Restored, "{msg:?}");
        assert_eq!(fs::read(&dup).expect("read"), bytes);
        let after = fs::metadata(&dup).expect("meta").modified().expect("mtime");
        assert_eq!(before, after, "the original came back, times included");
        if links(&dup).is_some() {
            assert_eq!(links(&dup), Some(1), "independent again");
            assert_eq!(links(&keeper), Some(1));
        }
        // writing one no longer changes the other
        fs::write(&dup, b"changed").expect("write");
        assert_eq!(fs::read(&keeper).expect("read"), bytes);
        assert!(!stored.exists());
    }

    #[test]
    fn a_failure_halfway_loses_nothing() {
        let (dir, _gate, opts) = setup();
        let keeper = dir.path().join("a.bin");
        let dup = dir.path().join("b.bin");
        fs::write(&keeper, b"same bytes").expect("write");
        fs::write(&dup, b"same bytes").expect("write");
        let item = linked_item(&dup, &keeper, DeleteMode::Hardlink);
        let root = opts.roots.system.clone().expect("root");
        let mut dirs = Vec::new();
        // the link cannot take the original name
        let err = replace_with_link(
            &item,
            0,
            &keeper,
            &root,
            &opts,
            &mut dirs,
            &|a, b| nfd_platform::link::hard_link(a, b),
            &|_, _| Err(io::Error::other("simulated failure")),
        );
        assert!(err.is_err());
        assert_eq!(fs::read(&dup).expect("original back"), b"same bytes");
        assert_eq!(fs::read(&keeper).expect("keeper"), b"same bytes");
        if links(&dup).is_some() {
            assert_eq!(links(&dup), Some(1));
            assert_eq!(links(&keeper), Some(1), "the temporary link is gone");
        }
        let names: Vec<_> = fs::read_dir(dir.path())
            .expect("ls")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!names.iter().any(|n| n.ends_with(".tmp")), "{names:?}");
        // the link cannot even be made: nothing moves
        let err = replace_with_link(
            &item,
            1,
            &dir.path().join("gone.bin"),
            &root,
            &opts,
            &mut dirs,
            &|a, b| nfd_platform::link::hard_link(a, b),
            &|a, b| fs::rename(a, b),
        );
        assert!(err.is_err());
        assert_eq!(fs::read(&dup).expect("untouched"), b"same bytes");
    }

    #[test]
    fn block_clone_when_the_volume_has_it() {
        let (dir, gate, opts) = setup();
        if nfd_platform::link::mode(dir.path()) != Some(nfd_platform::link::LinkMode::Clone) {
            return; // NTFS: covered by the hard-link tests
        }
        let keeper = dir.path().join("model.gguf");
        let dup = dir.path().join("model-copy.gguf");
        fs::write(&keeper, vec![9u8; 200_000]).expect("write");
        fs::write(&dup, vec![9u8; 200_000]).expect("write");
        let plan = CleanPlan {
            items: vec![linked_item(&dup, &keeper, DeleteMode::Clone)],
        };
        let op = execute(&plan, &gate, &LiveProbe, &opts, &mut |_, _, _| true);
        assert_eq!(op.items[0].status, ItemStatus::Done, "{op:#?}");
        fs::write(&dup, b"edited").expect("write");
        assert_eq!(fs::read(&keeper).expect("read"), vec![9u8; 200_000]);
    }

    #[test]
    fn links_need_a_keeper() {
        let (dir, gate, opts) = setup();
        let dup = dir.path().join("x.bin");
        fs::write(&dup, b"x").expect("write");
        let mut i = linked_item(&dup, &dup, DeleteMode::Hardlink);
        i.keeper = None;
        let op = execute(
            &CleanPlan { items: vec![i] },
            &gate,
            &LiveProbe,
            &opts,
            &mut |_, _, _| true,
        );
        assert_eq!(op.items[0].status, ItemStatus::Failed);
        assert_eq!(op.items[0].reason.as_deref(), Some("no-keeper"));
        assert!(dup.exists());
    }
}
