//! Standard mode: parallel directory walk on a rayon pool.
//!
//! Each directory is one task: it is listed with [`fsinfo::read_dir`] (metadata for all
//! children in a few syscalls), its children are classified, and a task is spawned for
//! every child directory that may be descended — never for links or other volumes.
//! Listings are sent to the calling thread, which streams progress and, once the walk
//! is over, assembles the arena in depth-first pre-order (children sorted by name), so
//! the result is deterministic whatever the thread interleaving.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use nfd_platform::fsinfo::{self, FileFlags, FileInfo, LinkKind};
use rayon::Scope;

use super::{ScanError, ScanEvent, ScanIssue, ScanMode, ScanOptions, ScanStats, ScanTree};
use crate::model::{Attrs, Entry, EntryId, EntryKind, roll_up};

/// One child of a listed directory, classified on a worker thread.
#[derive(Debug)]
struct Item {
    name: OsString,
    kind: EntryKind,
    info: FileInfo,
    error: Option<io::Error>,
    /// Listing id of this directory's own listing, if it is descended.
    listing: Option<u32>,
}

/// The children of one directory (or why they could not be read).
#[derive(Debug)]
struct Listing {
    id: u32,
    path: PathBuf,
    items: Vec<Item>,
    error: Option<io::Error>,
}

/// State shared by all walker tasks.
struct Walker<'a> {
    root_volume: u64,
    exact_links: bool,
    next_listing: AtomicU32,
    stop: &'a AtomicBool,
}

impl Walker<'_> {
    fn walk<'s>(&'s self, scope: &Scope<'s>, id: u32, dir: PathBuf, tx: Sender<Listing>) {
        if self.stop.load(Ordering::Relaxed) {
            return;
        }
        let mut listing = Listing {
            id,
            path: dir,
            items: Vec::new(),
            error: None,
        };
        match fsinfo::read_dir(&listing.path, self.exact_links) {
            Ok(children) => {
                listing.items.reserve_exact(children.len());
                for child in children {
                    let mut item = self.classify(child);
                    if item.kind == EntryKind::Dir {
                        let child_id = self.next_listing.fetch_add(1, Ordering::Relaxed);
                        item.listing = Some(child_id);
                        let path = listing.path.join(&item.name);
                        let tx = tx.clone();
                        scope.spawn(move |s| self.walk(s, child_id, path, tx));
                    }
                    listing.items.push(item);
                }
                listing.items.sort_unstable_by(|a, b| a.name.cmp(&b.name));
            }
            Err(err) => listing.error = Some(err),
        }
        // The receiver only disappears if the scan is being torn down.
        let _ = tx.send(listing);
    }

    fn classify(&self, child: fsinfo::DirItem) -> Item {
        let (info, error) = match child.info {
            Ok(info) => (info, None),
            Err(err) => (FileInfo::default(), Some(err)),
        };
        let kind = if error.is_some() {
            // Unknown type: record it, never descend.
            EntryKind::Other
        } else {
            kind_of(&info, self.root_volume)
        };
        Item {
            name: child.name,
            kind,
            info,
            error,
            listing: None,
        }
    }
}

/// Entry kind of a non-root entry; anything but [`EntryKind::Dir`] is not descended.
fn kind_of(info: &FileInfo, root_volume: u64) -> EntryKind {
    match info.link {
        LinkKind::Symlink => EntryKind::Symlink,
        LinkKind::Junction => EntryKind::Junction,
        LinkKind::MountPoint => EntryKind::MountPoint,
        // A plain directory on another volume (a Unix mount) is a boundary too.
        LinkKind::None if info.is_dir && !info.approximate && info.volume_id != root_volume => {
            EntryKind::MountPoint
        }
        LinkKind::None if info.is_dir => EntryKind::Dir,
        LinkKind::None => EntryKind::File,
    }
}

pub(super) fn scan(
    root: &Path,
    options: &ScanOptions,
    events: Option<&Sender<ScanEvent>>,
) -> Result<ScanTree, ScanError> {
    let start = Instant::now();
    let root_err = |source: io::Error| ScanError::Root {
        path: root.to_path_buf(),
        source,
    };
    let mut root = std::path::absolute(root).map_err(root_err)?;
    let mut root_info = fsinfo::file_info(&root).map_err(root_err)?;
    if root_info.link != LinkKind::None {
        // The user explicitly asked for this link: scan its target, but nothing below
        // it is followed.
        root = strip_verbatim(std::fs::canonicalize(&root).map_err(root_err)?);
        root_info = fsinfo::file_info(&root).map_err(root_err)?;
    }

    let mut asm = Assembler::new(&root);
    let root_kind = if root_info.is_dir {
        EntryKind::Dir
    } else {
        EntryKind::File
    };
    let root_id = asm.push(&Item {
        name: root.clone().into_os_string(),
        kind: root_kind,
        info: root_info,
        error: None,
        listing: None,
    });

    if root_kind == EntryKind::Dir {
        let listings = walk(&root, root_info.volume_id, options, events)?;
        asm.assemble(listings, root_id).map_err(root_err)?;
        asm.resolve_link_counts();
    }

    let file_counts = roll_up(&mut asm.entries);
    Ok(ScanTree {
        root,
        entries: asm.entries,
        file_counts,
        issues: asm.issues,
        stats: asm.stats,
        mode: ScanMode::Standard,
        fallback: None,
        started_at: 0,
        elapsed: start.elapsed(),
    })
}

/// Run the parallel walk below `root`; returns listings indexed by listing id.
fn walk(
    root: &Path,
    root_volume: u64,
    options: &ScanOptions,
    events: Option<&Sender<ScanEvent>>,
) -> Result<Vec<Option<Listing>>, ScanError> {
    let threads = options
        .threads
        .map_or_else(default_threads, std::num::NonZeroUsize::get);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|i| format!("nfd-scan-{i}"))
        .build()
        .map_err(|e| ScanError::Threads(e.to_string()))?;

    let stop = AtomicBool::new(false);
    let cancelled = || {
        options
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
    };
    let mut listings: Vec<Option<Listing>> = Vec::new();
    let mut progress = ScanStats::default();
    let mut last_emit = Instant::now();

    std::thread::scope(|threads| {
        let (tx, rx) = mpsc::channel::<Listing>();
        let walker = Walker {
            root_volume,
            exact_links: options.exact_links,
            next_listing: AtomicU32::new(1),
            stop: &stop,
        };
        let root = root.to_path_buf();
        threads.spawn(move || pool.scope(|s| walker.walk(s, 0, root, tx)));

        // Ends when every task (and so every sender) is gone.
        loop {
            let listing = match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(listing) => Some(listing),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if cancelled() {
                stop.store(true, Ordering::Relaxed);
            }
            if let Some(listing) = listing {
                count_progress(&mut progress, &listing);
                let slot = listing.id as usize;
                if listings.len() <= slot {
                    listings.resize_with(slot + 1, || None);
                }
                listings[slot] = Some(listing);
            }
            if let Some(tx) = events {
                if last_emit.elapsed() >= options.progress_interval {
                    let _ = tx.send(ScanEvent::Progress(progress));
                    last_emit = Instant::now();
                }
            }
        }
    });

    if stop.load(Ordering::Relaxed) || cancelled() {
        return Err(ScanError::Cancelled);
    }
    Ok(listings)
}

/// Live counters while listings stream in (hard-link duplicates are only known
/// after assembly, so `files`/`logical` may slightly overcount until `Done`).
fn count_progress(p: &mut ScanStats, listing: &Listing) {
    if listing.error.is_some() {
        p.errors += 1;
    }
    for item in &listing.items {
        if item.error.is_some() {
            p.errors += 1;
        }
        match item.kind {
            EntryKind::File => {
                p.files += 1;
                p.logical += item.info.logical;
                p.on_disk += item.info.on_disk;
            }
            EntryKind::Dir => p.dirs += 1,
            EntryKind::Symlink => p.symlinks += 1,
            EntryKind::Junction => p.junctions += 1,
            EntryKind::MountPoint => p.mount_points += 1,
            EntryKind::Other => p.other += 1,
        }
    }
}

fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get)
}

/// `\\?\C:\x` → `C:\x`, `\\?\UNC\srv\share` → `\\srv\share` (other paths unchanged).
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if rest.as_bytes().get(1) == Some(&b':') {
            return PathBuf::from(rest);
        }
    }
    path
}

fn attrs_from(flags: FileFlags) -> Attrs {
    const MAP: [(FileFlags, Attrs); 7] = [
        (FileFlags::SYSTEM, Attrs::SYSTEM),
        (FileFlags::HIDDEN, Attrs::HIDDEN),
        (FileFlags::REPARSE, Attrs::REPARSE),
        (FileFlags::COMPRESSED, Attrs::COMPRESSED),
        (FileFlags::SPARSE, Attrs::SPARSE),
        (FileFlags::OFFLINE, Attrs::OFFLINE),
        (FileFlags::READONLY, Attrs::READONLY),
    ];
    MAP.iter()
        .filter(|(f, _)| flags.contains(*f))
        .fold(Attrs::default(), |acc, (_, a)| acc.with(*a))
}

/// Builds the pre-order arena and the final statistics.
struct Assembler {
    root: PathBuf,
    entries: Vec<Entry>,
    issues: Vec<ScanIssue>,
    stats: ScanStats,
    /// File ID → first entry with that ID. A scan never crosses a volume boundary,
    /// so the ID alone identifies the data.
    first_by_id: HashMap<u128, EntryId>,
    /// First entry → every later name for the same data.
    link_groups: HashMap<EntryId, Vec<EntryId>>,
}

impl Assembler {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            entries: Vec::new(),
            issues: Vec::new(),
            stats: ScanStats::default(),
            first_by_id: HashMap::new(),
            link_groups: HashMap::new(),
        }
    }

    /// Depth-first assembly of all listings below `root_id`.
    fn assemble(&mut self, mut listings: Vec<Option<Listing>>, root_id: EntryId) -> io::Result<()> {
        let mut take = |id: u32| listings.get_mut(id as usize).and_then(Option::take);
        let root_listing = take(0).ok_or_else(|| io::Error::other("root listing missing"))?;
        if let Some(err) = root_listing.error {
            // An unreadable root is fatal rather than an empty report.
            return Err(err);
        }
        let mut stack = vec![(root_listing.items.into_iter(), root_id)];
        while let Some((items, parent)) = stack.last_mut() {
            let parent = *parent;
            let Some(item) = items.next() else {
                stack.pop();
                continue;
            };
            let id = self.push_child(&item, parent);
            if let Some(child) = item.listing.and_then(&mut take) {
                if let Some(err) = &child.error {
                    self.issue(ScanIssue::from_io(&child.path, err));
                }
                stack.push((child.items.into_iter(), id));
            }
        }
        Ok(())
    }

    fn push_child(&mut self, item: &Item, parent: EntryId) -> EntryId {
        let id = self.push(item);
        self.entries[id.index()].parent = Some(parent);
        if let Some(err) = &item.error {
            let path = self.path_of(id);
            self.issue(ScanIssue::from_io(&path, err));
        }
        id
    }

    fn push(&mut self, item: &Item) -> EntryId {
        let id = EntryId(self.entries.len() as u32);
        let info = &item.info;
        // Dedupe on the file ID even when the link count says 1: bind-filter projections
        // (e.g. MSIX-redirected AppData folders) show one file under two paths without
        // any reparse point or second link.
        let mut hardlink_dup = false;
        if item.kind == EntryKind::File {
            if let Some(file_id) = info.file_id {
                match self.first_by_id.get(&file_id) {
                    Some(&first) => {
                        hardlink_dup = true;
                        self.link_groups.entry(first).or_default().push(id);
                    }
                    None => {
                        self.first_by_id.insert(file_id, id);
                    }
                }
            }
        }

        let stats = &mut self.stats;
        if info.approximate {
            stats.approximate += 1;
        }
        match item.kind {
            EntryKind::File if hardlink_dup => stats.hardlink_dups += 1,
            EntryKind::File => {
                stats.files += 1;
                stats.logical += info.logical;
                stats.on_disk += info.on_disk;
            }
            EntryKind::Dir => stats.dirs += 1,
            EntryKind::Symlink => stats.symlinks += 1,
            EntryKind::Junction => stats.junctions += 1,
            EntryKind::MountPoint => stats.mount_points += 1,
            EntryKind::Other => stats.other += 1,
        }

        let sized = item.kind == EntryKind::File;
        self.entries.push(Entry {
            id,
            parent: None,
            name: item.name.to_string_lossy().into(),
            kind: item.kind,
            logical: if sized { info.logical } else { 0 },
            on_disk: if sized { info.on_disk } else { 0 },
            mtime: info.mtime,
            attrs: attrs_from(info.flags),
            hardlink_count: info
                .link_count
                .map_or(1, |n| u16::try_from(n).unwrap_or(u16::MAX)),
            file_id: info.file_id,
            hardlink_dup,
            removed: false,
        });
        id
    }

    /// Directory listings carry no link counts. Files seen under several names get
    /// their real count from one probe (at least the number of names seen); the rest
    /// keep the count they have (1 when unknown — another name may exist outside the
    /// scanned root).
    fn resolve_link_counts(&mut self) {
        let groups = std::mem::take(&mut self.link_groups);
        for (first, dups) in groups {
            let names_seen = u32::try_from(dups.len() + 1).unwrap_or(u32::MAX);
            let probed = fsinfo::file_info(&self.path_of(first))
                .ok()
                .and_then(|i| i.link_count);
            let count = probed.unwrap_or(names_seen).max(names_seen);
            let count = u16::try_from(count).unwrap_or(u16::MAX);
            for id in std::iter::once(first).chain(dups) {
                self.entries[id.index()].hardlink_count = count;
            }
        }
    }

    fn path_of(&self, id: EntryId) -> PathBuf {
        let mut names = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            let e = &self.entries[c.index()];
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

    fn issue(&mut self, issue: ScanIssue) {
        self.stats.errors += 1;
        self.issues.push(issue);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_prefixes() {
        assert_eq!(strip_verbatim(r"\\?\C:\x".into()), PathBuf::from(r"C:\x"));
        assert_eq!(
            strip_verbatim(r"\\?\UNC\srv\s".into()),
            PathBuf::from(r"\\srv\s")
        );
        assert_eq!(strip_verbatim("/home/x".into()), PathBuf::from("/home/x"));
        let vol = r"\\?\Volume{1}\";
        assert_eq!(strip_verbatim(vol.into()), PathBuf::from(vol));
    }

    #[test]
    fn classification_never_descends_links_or_foreign_volumes() {
        let dir = FileInfo {
            is_dir: true,
            volume_id: 7,
            ..FileInfo::default()
        };
        assert_eq!(kind_of(&dir, 7), EntryKind::Dir);
        assert_eq!(kind_of(&dir, 8), EntryKind::MountPoint);
        let approx = FileInfo {
            approximate: true,
            volume_id: 0,
            ..dir
        };
        assert_eq!(
            kind_of(&approx, 7),
            EntryKind::Dir,
            "unknown volume is not a boundary"
        );
        for (link, kind) in [
            (LinkKind::Junction, EntryKind::Junction),
            (LinkKind::Symlink, EntryKind::Symlink),
            (LinkKind::MountPoint, EntryKind::MountPoint),
        ] {
            assert_eq!(kind_of(&FileInfo { link, ..dir }, 7), kind);
        }
        assert_eq!(kind_of(&FileInfo::default(), 0), EntryKind::File);
    }
}
