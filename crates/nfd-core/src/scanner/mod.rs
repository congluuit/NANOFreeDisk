//! Filesystem scanner.
//!
//! [`scan`] walks a directory tree and returns a [`ScanTree`]: an arena of
//! [`Entry`] values in depth-first pre-order with directory sizes already rolled up.
//!
//! Guarantees (standard mode):
//! * **No link is ever followed.** Symlinks, junctions and volume mount points are
//!   recorded once as [`EntryKind::Symlink`] / [`EntryKind::Junction`] /
//!   [`EntryKind::MountPoint`] with size 0 and are not descended. A directory on a
//!   different volume than the root is treated as a mount point too.
//! * **Hard-linked data is counted once**: a scan never leaves the root's volume, and
//!   every later name for an already-seen `file_id` is flagged
//!   [`Entry::hardlink_dup`] and excluded from totals (the first name in sorted
//!   depth-first order keeps the bytes). This also catches bind-filter projections
//!   (MSIX-redirected `AppData` folders) that show one file under two paths with a
//!   link count of 1. See [`ScanOptions::exact_links`] for how
//!   [`Entry::hardlink_count`] is filled.
//! * **Errors are data.** Unreadable directories and entries become [`ScanIssue`]s;
//!   only an unreadable root (or cancellation) fails the scan.
//! * **Streaming.** Pass a [`Sender`] to receive [`ScanEvent::Progress`] roughly every
//!   [`ScanOptions::progress_interval`] and a final [`ScanEvent::Done`].
//!
//! [`ScanMode::Fast`] (NTFS `$MFT` reader, `mft` cargo feature) is a stub for now and
//! falls back to standard mode; [`ScanTree::fallback`] says why.

#[cfg(feature = "mft")]
pub mod mft;
pub mod report;
mod standard;

use std::collections::HashMap;
use std::io;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use thiserror::Error;

use crate::model::{Entry, EntryId, EntryKind};

pub use report::Report;

/// How the tree is enumerated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanMode {
    /// Parallel directory walk; works everywhere without elevation.
    #[default]
    Standard,
    /// NTFS `$MFT` reader (admin, NTFS only). Not implemented yet — falls back to
    /// [`ScanMode::Standard`].
    Fast,
}

/// Scan configuration.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub mode: ScanMode,
    /// Worker threads; `None` uses the number of logical CPUs.
    pub threads: Option<NonZeroUsize>,
    /// Minimum time between two [`ScanEvent::Progress`] events.
    pub progress_interval: Duration,
    /// Set to `true` to abort; the scan then returns [`ScanError::Cancelled`].
    pub cancel: Option<Arc<AtomicBool>>,
    /// Open every file to read its exact hard-link count (Windows directory listings
    /// do not carry it). Several times slower; without it, only files seen under two
    /// or more names inside the scan get their exact count and the rest report 1.
    /// Totals are de-duplicated by file ID either way.
    pub exact_links: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            mode: ScanMode::Standard,
            threads: None,
            progress_interval: Duration::from_millis(100),
            cancel: None,
            exact_links: false,
        }
    }
}

/// Running / final counters. `files` excludes hard-link duplicates; `logical` and
/// `on_disk` are the counted bytes (no duplicates, no links).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ScanStats {
    pub files: u64,
    pub dirs: u64,
    pub symlinks: u64,
    pub junctions: u64,
    pub mount_points: u64,
    pub other: u64,
    pub hardlink_dups: u64,
    pub logical: u64,
    pub on_disk: u64,
    pub errors: u64,
    /// Entries whose sizes come from the directory listing because they could not be
    /// opened (on-disk size = logical size, no hard-link detection).
    pub approximate: u64,
}

/// Streamed to the caller while a scan runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanEvent {
    Progress(ScanStats),
    Done { stats: ScanStats, elapsed: Duration },
}

/// Category of a non-fatal problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    AccessDenied,
    NotFound,
    Other,
}

/// A path the scanner could not fully read. Its size is missing from the totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanIssue {
    pub path: String,
    pub kind: IssueKind,
    pub message: String,
}

impl ScanIssue {
    pub(crate) fn from_io(path: &Path, err: &io::Error) -> Self {
        let kind = match err.kind() {
            io::ErrorKind::PermissionDenied => IssueKind::AccessDenied,
            io::ErrorKind::NotFound => IssueKind::NotFound,
            _ => IssueKind::Other,
        };
        Self {
            path: path.to_string_lossy().into_owned(),
            kind,
            message: err.to_string(),
        }
    }
}

/// Fatal scan failure.
#[derive(Debug, Error)]
pub enum ScanError {
    #[error("cannot scan {path}")]
    Root {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("scan cancelled")]
    Cancelled,
    #[error("cannot start scanner threads: {0}")]
    Threads(String),
}

/// Result of a scan.
#[derive(Debug, Clone)]
pub struct ScanTree {
    /// Absolute path that was scanned (link targets resolved for the root only).
    pub root: PathBuf,
    /// Arena in depth-first pre-order; `entries[0]` is the root. Directory sizes are
    /// rolled-up totals.
    pub entries: Vec<Entry>,
    /// Counted files at or below each entry (parallel to `entries`).
    pub file_counts: Vec<u64>,
    pub issues: Vec<ScanIssue>,
    pub stats: ScanStats,
    /// Mode actually used.
    pub mode: ScanMode,
    /// Why the requested mode was not used, if it was not.
    pub fallback: Option<String>,
    /// Scan start, Unix seconds.
    pub started_at: i64,
    pub elapsed: Duration,
}

/// Per-extension totals (hard-link duplicates excluded).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExtStat {
    /// Lower-case extension without the dot; empty for files without one.
    pub ext: String,
    pub files: u64,
    pub logical: u64,
    pub on_disk: u64,
}

impl ScanTree {
    /// Id of the root entry, `None` for an empty tree.
    #[must_use]
    pub fn root_id(&self) -> Option<EntryId> {
        (!self.entries.is_empty()).then_some(EntryId(0))
    }

    #[must_use]
    pub fn entry(&self, id: EntryId) -> &Entry {
        &self.entries[id.index()]
    }

    /// Full path of an entry (names are stored lossily as UTF-8).
    #[must_use]
    pub fn path(&self, id: EntryId) -> PathBuf {
        let mut names = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            let e = self.entry(c);
            if e.parent.is_none() {
                break;
            }
            names.push(&*e.name);
            cur = e.parent;
        }
        let mut path = self.root.clone();
        path.extend(names.iter().rev());
        path
    }

    /// The `n` largest counted files by on-disk size (ties: logical size, then path
    /// order), skipping files smaller than `min_on_disk`.
    #[must_use]
    pub fn top_files(&self, n: usize, min_on_disk: u64) -> Vec<EntryId> {
        let ids = self
            .entries
            .iter()
            .filter(|e| {
                e.kind == EntryKind::File
                    && !e.hardlink_dup
                    && !e.removed
                    && e.on_disk >= min_on_disk
            })
            .map(|e| e.id);
        self.largest(ids, n)
    }

    /// Direct children of the root, largest first, skipping those below `min_on_disk`.
    #[must_use]
    pub fn top_children(&self, n: usize, min_on_disk: u64) -> Vec<EntryId> {
        let ids = self
            .entries
            .iter()
            .filter(|e| e.parent == Some(EntryId(0)) && e.counted().1 >= min_on_disk)
            .map(|e| e.id);
        self.largest(ids, n)
    }

    fn largest(&self, ids: impl Iterator<Item = EntryId>, n: usize) -> Vec<EntryId> {
        let key = |id: &EntryId| {
            let (logical, on_disk) = self.entry(*id).counted();
            (std::cmp::Reverse(on_disk), std::cmp::Reverse(logical), *id)
        };
        let mut ids: Vec<EntryId> = ids.collect();
        if n < ids.len() {
            ids.select_nth_unstable_by_key(n, key);
            ids.truncate(n);
        }
        ids.sort_unstable_by_key(key);
        ids
    }

    /// Mark `id` and its subtree (`id..ends[id]`, see [`crate::model::subtree_ends`])
    /// as removed after it was deleted from disk: ancestors, file counts and
    /// [`ScanTree::stats`] lose its bytes, ids stay valid. Returns the counted
    /// `(logical, on_disk, files)` that were taken off; removing an already removed
    /// entry (or the root) changes nothing.
    pub fn remove(&mut self, id: EntryId, ends: &[u32]) -> (u64, u64, u64) {
        let i = id.index();
        if i == 0 || i >= self.entries.len() || self.entries[i].removed {
            return (0, 0, 0);
        }
        let (logical, on_disk) = self.entries[i].counted();
        let files = self.file_counts[i];
        let mut dirs = 0;
        let end = ends.get(i).map_or(i + 1, |&e| e as usize);
        for e in &mut self.entries[i..end] {
            if !e.removed && e.kind == EntryKind::Dir {
                dirs += 1;
            }
            e.removed = true;
        }
        let mut cur = self.entries[i].parent;
        while let Some(p) = cur {
            let parent = &mut self.entries[p.index()];
            parent.logical -= logical;
            parent.on_disk -= on_disk;
            self.file_counts[p.index()] -= files;
            cur = parent.parent;
        }
        self.stats.logical -= logical;
        self.stats.on_disk -= on_disk;
        self.stats.files -= files;
        self.stats.dirs = self.stats.dirs.saturating_sub(dirs);
        (logical, on_disk, files)
    }

    /// Totals per file extension, largest on-disk first.
    #[must_use]
    pub fn extensions(&self) -> Vec<ExtStat> {
        let mut map: HashMap<String, ExtStat> = HashMap::new();
        for e in &self.entries {
            if e.kind != EntryKind::File || e.hardlink_dup || e.removed {
                continue;
            }
            let ext = Path::new(&*e.name)
                .extension()
                .map(|x| x.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let stat = map.entry(ext).or_default();
            stat.files += 1;
            stat.logical += e.logical;
            stat.on_disk += e.on_disk;
        }
        let mut out: Vec<ExtStat> = map
            .into_iter()
            .map(|(ext, s)| ExtStat { ext, ..s })
            .collect();
        out.sort_unstable_by(|a, b| {
            b.on_disk
                .cmp(&a.on_disk)
                .then(b.logical.cmp(&a.logical))
                .then_with(|| a.ext.cmp(&b.ext))
        });
        out
    }
}

/// Scan `root`, streaming progress to `events` if given.
///
/// # Errors
/// [`ScanError::Root`] if the root cannot be read, [`ScanError::Cancelled`] if the
/// cancel flag was raised, [`ScanError::Threads`] if the worker pool cannot start.
pub fn scan(
    root: &Path,
    options: &ScanOptions,
    events: Option<&Sender<ScanEvent>>,
) -> Result<ScanTree, ScanError> {
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);

    let fallback = match options.mode {
        ScanMode::Standard => None,
        ScanMode::Fast => match fast(root, options) {
            Ok(tree) => return Ok(finish(tree, events)),
            Err(reason) => Some(reason),
        },
    };

    let mut tree = standard::scan(root, options, events)?;
    tree.started_at = started_at;
    tree.fallback = fallback;
    Ok(finish(tree, events))
}

#[cfg(feature = "mft")]
fn fast(root: &Path, options: &ScanOptions) -> Result<ScanTree, String> {
    mft::scan(root, options).map_err(|e| e.to_string())
}

#[cfg(not(feature = "mft"))]
fn fast(_root: &Path, _options: &ScanOptions) -> Result<ScanTree, String> {
    Err("this build has no MFT reader (cargo feature `mft`)".to_owned())
}

fn finish(tree: ScanTree, events: Option<&Sender<ScanEvent>>) -> ScanTree {
    if let Some(tx) = events {
        // A dropped receiver only means nobody is listening any more.
        let _ = tx.send(ScanEvent::Done {
            stats: tree.stats,
            elapsed: tree.elapsed,
        });
    }
    tree
}

#[cfg(test)]
mod tests;
