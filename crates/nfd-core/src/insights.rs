//! Ready-made questions about a scan (PLAN §13 A2 / A3), answered on the in-memory
//! arena with [`crate::query`] filters: no filesystem access, one pass each.
//!
//! * [`old_and_large`] — very large files, large files nobody touched for a long
//!   time, and old installers / disk images left in `Downloads` folders.
//! * [`empty_folders`] — folders that contain nothing but (possibly nested) empty
//!   folders and files the system creates on its own ([`CLUTTER_FILES`]).
//! * [`zero_byte_files`] — files of size 0, minus markers that mean something while
//!   empty and anything inside program-owned folders.
//! * [`age_matrix`] — bytes by last-modified age, per child folder (A9 heatmap).
//!
//! They only *list*; anything the user then removes still goes through
//! [`crate::safety`].

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::model::{Attrs, EntryId, EntryKind};
use crate::query::{Filter, KindFilter, Ranked, SearchResult};
use crate::scanner::ScanTree;

/// Extensions treated as "installers and disk images" by [`old_and_large`].
pub const INSTALLER_EXTS: &[&str] = &[
    "exe",
    "msi",
    "msix",
    "msixbundle",
    "appx",
    "iso",
    "img",
    "zip",
    "7z",
    "rar",
];

/// Folder name (case-insensitive) whose contents count as downloads.
pub const DOWNLOADS_DIR: &str = "downloads";

const DAY: i64 = 86_400;

/// Thresholds for [`old_and_large`]. Times are Unix seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OldLargeOptions {
    /// "Large": files at least this big on disk.
    pub large_min: u64,
    /// "Old": files at least this big …
    pub stale_min: u64,
    /// … last modified before this time.
    pub stale_before: i64,
    /// "Old installers": installer / image files under a `Downloads` folder last
    /// modified before this time (any size).
    pub installer_before: i64,
    /// Rows kept per list (the counts and totals always cover every match).
    pub limit: usize,
}

impl OldLargeOptions {
    /// Defaults relative to `now`: large ≥ 1 GiB, old = ≥ 100 MiB and untouched for
    /// a year, installers older than 30 days, 200 rows per list.
    #[must_use]
    pub const fn at(now: i64) -> Self {
        Self {
            large_min: 1 << 30,
            stale_min: 100 << 20,
            stale_before: now - 365 * DAY,
            installer_before: now - 30 * DAY,
            limit: 200,
        }
    }
}

impl Default for OldLargeOptions {
    fn default() -> Self {
        Self::at(0)
    }
}

/// Result of [`old_and_large`]. A file can appear in several lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OldLarge {
    pub large: SearchResult,
    pub stale: SearchResult,
    pub installers: SearchResult,
    /// Files in at least one list, each counted once.
    pub distinct_files: u64,
    pub distinct_on_disk: u64,
}

/// The three filters [`old_and_large`] runs, exposed so a UI can reopen one as a
/// regular search.
#[must_use]
pub fn old_large_filters(o: &OldLargeOptions) -> [Filter; 3] {
    let files = |f: Filter| Filter {
        kind: KindFilter::Files,
        ..f
    };
    [
        files(Filter {
            min_size: Some(o.large_min),
            ..Filter::default()
        }),
        files(Filter {
            min_size: Some(o.stale_min),
            modified_before: Some(o.stale_before),
            ..Filter::default()
        }),
        files(Filter {
            exts: INSTALLER_EXTS.iter().map(|x| (*x).to_owned()).collect(),
            modified_before: Some(o.installer_before),
            ..Filter::default()
        }),
    ]
}

/// Large, old and leftover-installer files strictly below `under` (use the root id
/// for the whole scan). `ends` comes from [`crate::model::subtree_ends`].
#[must_use]
pub fn old_and_large(
    tree: &ScanTree,
    ends: &[u32],
    under: EntryId,
    o: &OldLargeOptions,
) -> OldLarge {
    let [large_f, stale_f, inst_f] = old_large_filters(o);
    let (large_m, stale_m, inst_m) = (large_f.matcher(), stale_f.matcher(), inst_f.matcher());
    let in_downloads = downloads_mask(tree);
    let (mut large, mut stale, mut inst) =
        (Ranked::default(), Ranked::default(), Ranked::default());
    let mut out = OldLarge::default();
    for e in scope(tree, ends, under) {
        let mut hit = false;
        if large_m.matches(e) {
            large.push(e);
            hit = true;
        }
        if stale_m.matches(e) {
            stale.push(e);
            hit = true;
        }
        if in_downloads[e.id.index()] && inst_m.matches(e) {
            inst.push(e);
            hit = true;
        }
        if hit {
            out.distinct_files += 1;
            out.distinct_on_disk += e.counted().1;
        }
    }
    out.large = large.finish(o.limit);
    out.stale = stale.finish(o.limit);
    out.installers = inst.finish(o.limit);
    out
}

/// Entries strictly below `under`.
fn scope<'a>(tree: &'a ScanTree, ends: &[u32], under: EntryId) -> &'a [crate::model::Entry] {
    let start = under.index() + 1;
    let end = ends.get(under.index()).map_or(start, |&e| e as usize);
    tree.entries.get(start..end).unwrap_or_default()
}

/// `true` for every entry that has a `Downloads` folder among its ancestors (the
/// scanned root counts when it is itself named `Downloads`).
fn downloads_mask(tree: &ScanTree) -> Vec<bool> {
    let root_is_downloads = tree
        .root
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(DOWNLOADS_DIR));
    let mut mask = vec![false; tree.entries.len()];
    for e in &tree.entries {
        let Some(p) = e.parent else { continue };
        let parent = tree.entry(p);
        let parent_is_downloads = if parent.parent.is_none() {
            root_is_downloads
        } else {
            parent.kind == EntryKind::Dir && parent.name.eq_ignore_ascii_case(DOWNLOADS_DIR)
        };
        // pre-order: the parent's flag is final before its children are visited
        mask[e.id.index()] = mask[p.index()] || parent_is_downloads;
    }
    mask
}

/// Files Windows and macOS create in folders on their own, which hold nothing of the
/// user's (compared case-insensitively): folder view settings and thumbnail caches.
/// A folder holding only these (and empty folders) is empty in practice. Kept short
/// on purpose: anything else counts as content.
pub const CLUTTER_FILES: &[&str] = &[
    "desktop.ini",
    "thumbs.db",
    "ehthumbs.db",
    "ehthumbs_vista.db",
    ".ds_store",
    "._.ds_store",
];

/// `true` when `name` is one of [`CLUTTER_FILES`].
#[must_use]
pub fn is_clutter_file(name: &str) -> bool {
    CLUTTER_FILES.iter().any(|c| name.eq_ignore_ascii_case(c))
}

/// One empty folder tree found by [`empty_folders`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EmptyFolder {
    pub id: EntryId,
    /// Empty sub-folders inside it (0 = the folder itself is empty).
    pub nested: u64,
    /// [`CLUTTER_FILES`] inside it (they go with the folder).
    pub clutter: u64,
}

/// Matches of [`empty_folders`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EmptyFolders {
    /// Outermost empty folders, in path order.
    pub folders: Vec<EmptyFolder>,
    /// Number of outermost empty folders (may exceed `folders.len()`).
    pub matched: u64,
    /// Every empty folder, nested ones included.
    pub total_dirs: u64,
}

/// `true` for a folder Windows customized through its `desktop.ini` (custom icon,
/// localized name): Explorer only reads that file when the folder carries the
/// READONLY or SYSTEM attribute, so there it is part of what the folder is.
fn customized(dir: &crate::model::Entry) -> bool {
    dir.attrs.contains(Attrs::READONLY) || dir.attrs.contains(Attrs::SYSTEM)
}

/// Folders strictly below `under` that hold nothing but empty folders and
/// [`CLUTTER_FILES`] (counted in [`EmptyFolder::clutter`]; they go with the folder).
/// Only the outermost one of a nested empty tree is listed. Folders the scanner
/// could not read (see [`ScanTree::issues`]) are never reported as empty, nor are
/// their parents. Removed entries are ignored; links are content, never followed.
/// In a [`customized`] folder the clutter names count as content.
///
/// `keep_out` is asked about every empty folder (normally "would
/// [`crate::safety`] refuse to delete it?"): such a folder is not listed and counts
/// as content for its parent, but empty folders inside it are still listed — an
/// empty `Documents` is normal, an empty `Documents\Old project` is clutter.
#[must_use]
pub fn empty_folders(
    tree: &ScanTree,
    ends: &[u32],
    under: EntryId,
    limit: usize,
    keep_out: &impl Fn(EntryId) -> bool,
) -> EmptyFolders {
    let entries = scope(tree, ends, under);
    let base = under.index() + 1;
    let unreadable: HashSet<String> = tree.issues.iter().map(|i| i.path.to_lowercase()).collect();
    let is_unreadable = |id: EntryId| {
        !unreadable.is_empty()
            && unreadable.contains(&tree.path(id).to_string_lossy().to_lowercase())
    };
    // Bottom-up over the scope (reverse pre-order: children before parents).
    // has_content: the subtree holds something other than listable empty folders
    // and clutter files.
    let mut has_content = vec![false; entries.len()];
    let mut kept_out = vec![false; entries.len()];
    let mut dirs_below = vec![0u64; entries.len()];
    let mut clutter = vec![0u64; entries.len()];
    for e in entries.iter().rev() {
        if e.removed {
            continue;
        }
        let i = e.id.index() - base;
        let is_clutter = e.kind == EntryKind::File
            && is_clutter_file(&e.name)
            && e.parent.is_some_and(|p| !customized(tree.entry(p)));
        if is_clutter {
            // neither content nor a folder: it only travels with its folder
        } else if e.kind != EntryKind::Dir || (!has_content[i] && is_unreadable(e.id)) {
            has_content[i] = true;
        } else if !has_content[i] && keep_out(e.id) {
            kept_out[i] = true;
        }
        let Some(p) = e.parent.filter(|&p| p != under) else {
            continue;
        };
        let p = p.index() - base;
        has_content[p] |= has_content[i] || kept_out[i];
        if is_clutter {
            clutter[p] += 1;
        } else if e.kind == EntryKind::Dir && !kept_out[i] {
            dirs_below[p] += dirs_below[i] + 1;
            clutter[p] += clutter[i];
        }
    }
    let mut out = EmptyFolders::default();
    for e in entries {
        let i = e.id.index() - base;
        if e.removed || e.kind != EntryKind::Dir || has_content[i] || kept_out[i] {
            continue;
        }
        out.total_dirs += 1;
        let outermost = e.parent.is_none_or(|p| {
            p == under || {
                let p = p.index() - base;
                has_content[p] || kept_out[p]
            }
        });
        if !outermost {
            continue;
        }
        out.matched += 1;
        if out.folders.len() < limit {
            out.folders.push(EmptyFolder {
                id: e.id,
                nested: dirs_below[i],
                clutter: clutter[i],
            });
        }
    }
    out
}

/// File names (compared case-insensitively) that mean something while empty and are
/// never listed by [`zero_byte_files`]: Python package and typing markers, lock
/// files. Every name starting with `.` (`.gitkeep`, `.keep`, `.nojekyll`,
/// `.nomedia`, `.gitignore`…) is left out as well.
pub const ZERO_BYTE_KEEP_NAMES: &[&str] = &[
    "__init__.py",
    "__init__.pyi",
    "py.typed",
    "lock",
    "lockfile",
];

/// Extensions (lower case) of marker files that are empty on purpose: locks,
/// process ids, build stamps, "done" and "ready" flags.
pub const ZERO_BYTE_KEEP_EXTS: &[&str] = &[
    "lock",
    "lck",
    "pid",
    "stamp",
    "timestamp",
    "marker",
    "flag",
    "keep",
    "sentinel",
    "done",
    "ready",
];

/// `true` when an empty file called `name` is a marker [`zero_byte_files`] leaves
/// alone ([`ZERO_BYTE_KEEP_NAMES`], [`ZERO_BYTE_KEEP_EXTS`], dot files).
#[must_use]
pub fn is_zero_byte_marker(name: &str) -> bool {
    if name.starts_with('.')
        || ZERO_BYTE_KEEP_NAMES
            .iter()
            .any(|n| name.eq_ignore_ascii_case(n))
    {
        return true;
    }
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && ZERO_BYTE_KEEP_EXTS
                .iter()
                .any(|x| ext.eq_ignore_ascii_case(x))
    })
}

/// Matches of [`zero_byte_files`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ZeroByteFiles {
    /// In path order, at most the requested number.
    pub files: Vec<EntryId>,
    /// Every match (may exceed `files.len()`).
    pub matched: u64,
}

/// Files of size 0 strictly below `under`, leaving out:
///
/// * markers that mean something while empty ([`is_zero_byte_marker`]);
/// * files with the SYSTEM, HIDDEN, OFFLINE (cloud placeholder) or REPARSE
///   attribute, hard links with other names, removed entries;
/// * anything inside a folder whose name starts with `.` (`.git`, `.venv`,
///   `.cargo`, `.vscode`: owned by a tool), or inside a folder `program_dir` says
///   belongs to a program or is refused as a whole by the SafetyGate (asked once
///   per folder, ancestors of `under` included);
/// * files `keep_out` refuses (normally "would [`crate::safety`] refuse to delete
///   it?").
#[must_use]
pub fn zero_byte_files(
    tree: &ScanTree,
    ends: &[u32],
    under: EntryId,
    limit: usize,
    program_dir: &impl Fn(EntryId) -> bool,
    keep_out: &impl Fn(EntryId) -> bool,
) -> ZeroByteFiles {
    const SKIP_ATTRS: [Attrs; 4] = [Attrs::SYSTEM, Attrs::HIDDEN, Attrs::OFFLINE, Attrs::REPARSE];
    // per folder: 0 = not asked yet, 1 = fine, 2 = left out with everything below
    let mut verdict = vec![0u8; tree.entries.len()];
    let mut chain: Vec<EntryId> = Vec::new();
    let mut folder_out = |dir: EntryId| -> bool {
        chain.clear();
        let mut cur = Some(dir);
        while let Some(c) = cur {
            if verdict[c.index()] != 0 {
                break;
            }
            chain.push(c);
            cur = tree.entry(c).parent;
        }
        let mut out = cur.is_some_and(|c| verdict[c.index()] == 2);
        for &c in chain.iter().rev() {
            let e = tree.entry(c);
            out = out || (e.parent.is_some() && e.name.starts_with('.')) || program_dir(c);
            verdict[c.index()] = if out { 2 } else { 1 };
        }
        out
    };
    let mut out = ZeroByteFiles::default();
    for e in scope(tree, ends, under) {
        if e.removed
            || e.kind != EntryKind::File
            || e.logical != 0
            || e.hardlink_dup
            || e.hardlink_count > 1
            || SKIP_ATTRS.iter().any(|a| e.attrs.contains(*a))
            || is_zero_byte_marker(&e.name)
        {
            continue;
        }
        let Some(parent) = e.parent else { continue };
        if folder_out(parent) || keep_out(e.id) {
            continue;
        }
        out.matched += 1;
        if out.files.len() < limit {
            out.files.push(e.id);
        }
    }
    out
}

/// Upper bounds (days, exclusive) of the [`age_matrix`] columns; the last column is
/// everything older, then one column for files without a usable date.
pub const AGE_BUCKET_DAYS: [i64; 5] = [30, 182, 365, 730, 1826];
/// Number of columns: the bounded buckets, "older", "unknown date".
pub const AGE_COLUMNS: usize = AGE_BUCKET_DAYS.len() + 2;

fn age_column(mtime: i64, now: i64) -> usize {
    if mtime <= 0 {
        return AGE_COLUMNS - 1;
    }
    let days = (now - mtime).max(0) / DAY;
    AGE_BUCKET_DAYS
        .iter()
        .position(|&b| days < b)
        .unwrap_or(AGE_BUCKET_DAYS.len())
}

/// One row of [`AgeMatrix`]: a direct child of the folder (or "files directly in
/// it" when `id` is the folder itself).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgeRow {
    pub id: EntryId,
    /// Counted on-disk bytes per column.
    pub bytes: Vec<u64>,
    pub files: Vec<u64>,
    pub total: u64,
}

/// Bytes by last-modified age below a folder, split by its direct children.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AgeMatrix {
    /// Biggest children first; at most the requested number (the rest are summed in
    /// `other`).
    pub rows: Vec<AgeRow>,
    pub other: Vec<u64>,
    /// Column totals over everything below the folder.
    pub totals: Vec<u64>,
    pub files: Vec<u64>,
}

/// Age heatmap data for the folder `under` at time `now` (Unix seconds), keeping the
/// `limit` largest children as rows. Hard-link duplicates and removed entries are
/// skipped, like everywhere else.
#[must_use]
pub fn age_matrix(
    tree: &ScanTree,
    ends: &[u32],
    under: EntryId,
    now: i64,
    limit: usize,
) -> AgeMatrix {
    let entries = scope(tree, ends, under);
    let base = under.index() + 1;
    // row of each entry: the direct child of `under` it lives in (pre-order)
    let mut row_of = vec![usize::MAX; entries.len()];
    let mut children: Vec<EntryId> = Vec::new();
    let mut rows: Vec<AgeRow> = Vec::new();
    let mut out = AgeMatrix {
        other: vec![0; AGE_COLUMNS],
        totals: vec![0; AGE_COLUMNS],
        files: vec![0; AGE_COLUMNS],
        ..AgeMatrix::default()
    };
    // files directly inside `under` get their own row (id = under)
    let mut direct = AgeRow {
        id: under,
        bytes: vec![0; AGE_COLUMNS],
        files: vec![0; AGE_COLUMNS],
        total: 0,
    };
    for e in entries {
        let i = e.id.index() - base;
        let parent = e.parent.unwrap_or(under);
        let row = if parent == under {
            children.push(e.id);
            rows.push(AgeRow {
                id: e.id,
                bytes: vec![0; AGE_COLUMNS],
                files: vec![0; AGE_COLUMNS],
                total: 0,
            });
            rows.len() - 1
        } else {
            row_of[parent.index() - base]
        };
        row_of[i] = row;
        if e.removed || e.hardlink_dup || e.kind != EntryKind::File {
            continue;
        }
        let col = age_column(e.mtime, now);
        let size = e.counted().1;
        out.totals[col] += size;
        out.files[col] += 1;
        let target = if parent == under {
            &mut direct
        } else {
            &mut rows[row]
        };
        target.bytes[col] += size;
        target.files[col] += 1;
        target.total += size;
    }
    if direct.total > 0 {
        rows.push(direct);
    }
    rows.retain(|r| r.total > 0);
    rows.sort_by(|a, b| b.total.cmp(&a.total).then(a.id.cmp(&b.id)));
    for r in rows.iter().skip(limit) {
        for (o, b) in out.other.iter_mut().zip(&r.bytes) {
            *o += b;
        }
    }
    rows.truncate(limit);
    out.rows = rows;
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::model::{Attrs, Entry, roll_up, subtree_ends};
    use crate::scanner::{IssueKind, ScanIssue, ScanMode, ScanStats};

    const NOW: i64 = 1_800_000_000;
    const GB: u64 = 1 << 30;
    const MB: u64 = 1 << 20;

    fn tree(spec: &[(Option<u32>, &str, EntryKind, u64, i64)]) -> ScanTree {
        let mut entries: Vec<Entry> = spec
            .iter()
            .enumerate()
            .map(|(i, &(parent, name, kind, size, mtime))| Entry {
                id: EntryId(i as u32),
                parent: parent.map(EntryId),
                name: name.into(),
                kind,
                logical: size,
                on_disk: size,
                mtime,
                attrs: Attrs::default(),
                hardlink_count: 1,
                file_id: None,
                hardlink_dup: false,
                removed: false,
            })
            .collect();
        let file_counts = roll_up(&mut entries);
        ScanTree {
            root: PathBuf::from("/scan"),
            stats: ScanStats {
                files: file_counts[0],
                logical: entries[0].logical,
                on_disk: entries[0].on_disk,
                ..ScanStats::default()
            },
            entries,
            file_counts,
            issues: Vec::new(),
            mode: ScanMode::Standard,
            fallback: None,
            started_at: 0,
            elapsed: Duration::ZERO,
        }
    }

    fn names(t: &ScanTree, r: &SearchResult) -> Vec<String> {
        r.ids
            .iter()
            .map(|&id| t.entry(id).name.to_string())
            .collect()
    }

    use EntryKind::{Dir, File, Junction, Symlink};

    #[test]
    fn old_large_and_installers() {
        let old = NOW - 400 * DAY;
        let recent = NOW - 5 * DAY;
        let t = tree(&[
            (None, "scan", Dir, 0, 0),
            (Some(0), "vm.vhdx", File, 40 * GB, recent), // 1 large
            (Some(0), "old.mkv", File, 3 * GB, old),     // 2 large + stale
            (Some(0), "notes.txt", File, 200 * MB, old), // 3 stale
            (Some(0), "tiny.log", File, MB, old),        // 4 nothing (too small)
            (Some(0), "Users", Dir, 0, 0),               // 5
            (Some(5), "Downloads", Dir, 0, 0),           // 6
            (Some(6), "setup.exe", File, 80 * MB, NOW - 60 * DAY), // 7 installer
            (Some(6), "fresh.msi", File, 5 * MB, recent), // 8 too recent
            (Some(6), "photo.jpg", File, 5 * MB, old),   // 9 not an installer
            (Some(6), "nested", Dir, 0, 0),              // 10
            (Some(10), "win.ISO", File, 5 * GB, old),    // 11 all three
            (Some(0), "setup.exe", File, 80 * MB, old),  // 12 not in Downloads
        ]);
        let ends = subtree_ends(&t.entries);
        let r = old_and_large(&t, &ends, EntryId(0), &OldLargeOptions::at(NOW));
        assert_eq!(names(&t, &r.large), ["vm.vhdx", "win.ISO", "old.mkv"]);
        assert_eq!(names(&t, &r.stale), ["win.ISO", "old.mkv", "notes.txt"]);
        assert_eq!(names(&t, &r.installers), ["win.ISO", "setup.exe"]);
        assert_eq!(r.installers.ids[1], EntryId(7));
        assert_eq!(r.distinct_files, 5);
        assert_eq!(
            r.distinct_on_disk,
            40 * GB + 3 * GB + 200 * MB + 80 * MB + 5 * GB
        );

        // limit keeps counts and totals complete
        let o = OldLargeOptions {
            limit: 1,
            ..OldLargeOptions::at(NOW)
        };
        let r = old_and_large(&t, &ends, EntryId(0), &o);
        assert_eq!(names(&t, &r.large), ["vm.vhdx"]);
        assert_eq!(r.large.matched, 3);
        assert_eq!(r.large.matched_on_disk, 48 * GB);

        // scoped to Downloads
        let r = old_and_large(&t, &ends, EntryId(6), &OldLargeOptions::at(NOW));
        assert_eq!(names(&t, &r.large), ["win.ISO"]);
        assert_eq!(r.installers.matched, 2);
    }

    #[test]
    fn age_matrix_buckets_and_rows() {
        let t = tree(&[
            (None, "scan", Dir, 0, 0),
            (Some(0), "a", Dir, 0, 0),                           // 1
            (Some(1), "new.bin", File, 100, NOW - DAY),          // < 30 d
            (Some(1), "deep", Dir, 0, 0),                        // 3
            (Some(3), "old.bin", File, 50, NOW - 3 * 365 * DAY), // 2–5 y
            (Some(0), "b", Dir, 0, 0),                           // 5
            (Some(5), "mid.bin", File, 300, NOW - 200 * DAY),    // 6–12 m
            (Some(0), "loose.txt", File, 7, 0),                  // unknown date, direct
            (Some(0), "c", Dir, 0, 0),                           // 8 small
            (Some(8), "x", File, 1, NOW - 10 * 365 * DAY),       // > 5 y
        ]);
        let ends = subtree_ends(&t.entries);
        let m = age_matrix(&t, &ends, EntryId(0), NOW, 2);
        assert_eq!(m.totals, [100, 0, 300, 0, 50, 1, 7]);
        assert_eq!(m.files, [1, 0, 1, 0, 1, 1, 1]);
        let rows: Vec<(u32, u64)> = m.rows.iter().map(|r| (r.id.0, r.total)).collect();
        assert_eq!(rows, [(5, 300), (1, 150)]);
        assert_eq!(m.rows[1].bytes, [100, 0, 0, 0, 50, 0, 0]);
        // c (1 byte) and the loose file (7 bytes, row "scan" itself) are folded
        assert_eq!(m.other, [0, 0, 0, 0, 0, 1, 7]);
        let all = age_matrix(&t, &ends, EntryId(0), NOW, 10);
        assert!(all.rows.iter().any(|r| r.id == EntryId(0) && r.total == 7));
        // scoped below a
        let a = age_matrix(&t, &ends, EntryId(1), NOW, 10);
        assert_eq!(a.totals, [100, 0, 0, 0, 50, 0, 0]);
        assert_eq!(a.rows.iter().map(|r| r.id.0).collect::<Vec<_>>(), [1, 3]);
    }

    #[test]
    fn scanning_downloads_itself_counts() {
        let mut t = tree(&[
            (None, "Downloads", Dir, 0, 0),
            (Some(0), "a.zip", File, MB, 0),
        ]);
        // Built with the native separator so the root's file name is `Downloads` on
        // every OS (a `C:\...` literal is one opaque name on Unix).
        t.root = ["Users", "x", "Downloads"]
            .iter()
            .fold(PathBuf::from(std::path::MAIN_SEPARATOR_STR), |p, c| {
                p.join(c)
            });
        let ends = subtree_ends(&t.entries);
        let r = old_and_large(&t, &ends, EntryId(0), &OldLargeOptions::at(NOW));
        assert_eq!(names(&t, &r.installers), ["a.zip"]);
    }

    #[test]
    fn empty_folder_trees() {
        let mut t = tree(&[
            (None, "scan", Dir, 0, 0),
            (Some(0), "empty", Dir, 0, 0),    // 1 listed
            (Some(0), "deep", Dir, 0, 0),     // 2 listed, nested 2
            (Some(2), "a", Dir, 0, 0),        // 3
            (Some(3), "b", Dir, 0, 0),        // 4
            (Some(0), "full", Dir, 0, 0),     // 5 has a file somewhere
            (Some(5), "x", Dir, 0, 0),        // 6 listed (empty inside a full one)
            (Some(5), "y", Dir, 0, 0),        // 7
            (Some(7), "f.txt", File, 0, 0),   // 8 zero-byte files still count
            (Some(0), "linkonly", Dir, 0, 0), // 9 holds a junction: not empty
            (Some(9), "j", Junction, 0, 0),   // 10
            (Some(0), "denied", Dir, 0, 0),   // 11 unreadable: not empty
            (Some(0), "gone", Dir, 0, 0),     // 12
            (Some(12), "g.txt", File, 1, 0),  // 13 removed → gone becomes empty
        ]);
        t.issues.push(ScanIssue {
            path: PathBuf::from("/scan")
                .join("DENIED")
                .to_string_lossy()
                .into_owned(),
            kind: IssueKind::AccessDenied,
            message: String::new(),
        });
        let ends = subtree_ends(&t.entries);
        t.remove(EntryId(13), &ends);
        let r = empty_folders(&t, &ends, EntryId(0), 100, &|_| false);
        let got: Vec<(String, u64)> = r
            .folders
            .iter()
            .map(|f| (t.entry(f.id).name.to_string(), f.nested))
            .collect();
        assert_eq!(
            got,
            [
                ("empty".into(), 0),
                ("deep".into(), 2),
                ("x".into(), 0),
                ("gone".into(), 0)
            ]
        );
        assert_eq!(r.matched, 4);
        assert_eq!(r.total_dirs, 6);

        // scoped to "deep": its children are the outermost ones there
        let r = empty_folders(&t, &ends, EntryId(2), 100, &|_| false);
        assert_eq!(
            r.folders,
            [EmptyFolder {
                id: EntryId(3),
                nested: 1,
                clutter: 0
            }]
        );

        let r = empty_folders(&t, &ends, EntryId(0), 1, &|_| false);
        assert_eq!(r.folders.len(), 1);
        assert_eq!(r.matched, 4);

        // a protected empty folder is not listed, but empty folders inside it are
        let r = empty_folders(&t, &ends, EntryId(0), 100, &|id| id == EntryId(2));
        let got: Vec<(&str, u64)> = r
            .folders
            .iter()
            .map(|f| (&*t.entry(f.id).name, f.nested))
            .collect();
        assert_eq!(got, [("empty", 0), ("a", 1), ("x", 0), ("gone", 0)]);
        assert_eq!(r.total_dirs, 5);
    }

    fn with_attrs(mut t: ScanTree, id: u32, attrs: Attrs) -> ScanTree {
        t.entries[id as usize].attrs = attrs;
        t
    }

    #[test]
    fn clutter_files_do_not_make_a_folder_full() {
        let t = tree(&[
            (None, "scan", Dir, 0, 0),
            (Some(0), "old", Dir, 0, 0), // 1 listed, 3 clutter files
            (Some(1), "desktop.ini", File, 282, 0), // 2
            (Some(1), "THUMBS.DB", File, 9000, 0), // 3
            (Some(1), "sub", Dir, 0, 0), // 4
            (Some(4), ".DS_Store", File, 6148, 0), // 5
            (Some(0), "real", Dir, 0, 0), // 6 a real file: not empty
            (Some(6), "desktop.ini", File, 100, 0), // 7
            (Some(6), "notes.txt", File, 0, 0), // 8
            (Some(0), "Contacts", Dir, 0, 0), // 9 customized: desktop.ini matters
            (Some(9), "desktop.ini", File, 400, 0), // 10
            (Some(0), "linked", Dir, 0, 0), // 11 a link named like clutter
            (Some(11), "Thumbs.db", Symlink, 0, 0), // 12
            (Some(0), "Protected", Dir, 0, 0), // 13 refused by the gate
            (Some(13), "desktop.ini", File, 300, 0), // 14
            (Some(0), "deep", Dir, 0, 0), // 15 listed, nested 1, clutter 1
            (Some(15), "inner", Dir, 0, 0), // 16
            (Some(16), "Thumbs.db", File, 50, 0), // 17
        ]);
        let t = with_attrs(t, 9, Attrs::READONLY);
        let ends = subtree_ends(&t.entries);
        let r = empty_folders(&t, &ends, EntryId(0), 100, &|id| id == EntryId(13));
        let got: Vec<(&str, u64, u64)> = r
            .folders
            .iter()
            .map(|f| (&*t.entry(f.id).name, f.nested, f.clutter))
            .collect();
        assert_eq!(got, [("old", 1, 3), ("deep", 1, 1)]);
        assert_eq!(r.total_dirs, 4);
        // the folder's size and file count include its clutter, so the user is shown
        // (and the plan moves) exactly what goes
        assert_eq!(t.entries[1].on_disk, 282 + 9000 + 6148);
        assert_eq!(t.file_counts[1], 3);

        // a SYSTEM folder is customized as well
        let t = with_attrs(t, 1, Attrs::SYSTEM);
        let r = empty_folders(&t, &ends, EntryId(0), 100, &|id| id == EntryId(13));
        let got: Vec<(&str, u64, u64)> = r
            .folders
            .iter()
            .map(|f| (&*t.entry(f.id).name, f.nested, f.clutter))
            .collect();
        // "old" keeps its own desktop.ini and Thumbs.db; its sub-folder is listed
        assert_eq!(got, [("sub", 0, 1), ("deep", 1, 1)]);
    }

    #[test]
    fn clutter_names() {
        assert!(is_clutter_file("Desktop.ini"));
        assert!(is_clutter_file("thumbs.db"));
        assert!(!is_clutter_file("thumbs.db.bak"));
        assert!(!is_clutter_file("notes.ini"));
    }

    #[test]
    fn zero_byte_markers() {
        for keep in [
            "__init__.py",
            "py.typed",
            ".gitkeep",
            ".keep",
            ".nojekyll",
            ".nomedia",
            "LOCK",
            "build.stamp",
            "invoked.timestamp",
            "app.PID",
            "x.lock",
        ] {
            assert!(is_zero_byte_marker(keep), "{keep}");
        }
        for list in [
            "New Text Document.txt",
            "untitled",
            "report.docx",
            "lock.txt",
        ] {
            assert!(!is_zero_byte_marker(list), "{list}");
        }
    }

    #[test]
    fn zero_byte_files_skip_markers_and_program_folders() {
        let mut t = tree(&[
            (None, "scan", Dir, 0, 0),
            (Some(0), "New Text Document.txt", File, 0, 0), // 1 listed
            (Some(0), "full.txt", File, 10, 0),             // 2 not empty
            (Some(0), "__init__.py", File, 0, 0),           // 3 marker
            (Some(0), ".gitkeep", File, 0, 0),              // 4 marker
            (Some(0), "proj", Dir, 0, 0),                   // 5
            (Some(5), ".git", Dir, 0, 0),                   // 6 tool folder
            (Some(6), "FETCH_HEAD", File, 0, 0),            // 7
            (Some(5), "empty.log", File, 0, 0),             // 8 listed
            (Some(0), "Program Files", Dir, 0, 0),          // 9 program folder
            (Some(9), "app", Dir, 0, 0),                    // 10
            (Some(10), "installed", File, 0, 0),            // 11
            (Some(0), "hidden.dat", File, 0, 0),            // 12 HIDDEN
            (Some(0), "refused.txt", File, 0, 0),           // 13 keep_out
            (Some(0), "gone.txt", File, 0, 0),              // 14 removed
            (Some(0), "link", Symlink, 0, 0),               // 15 not a file
        ]);
        t.entries[12].attrs = Attrs::HIDDEN;
        let ends = subtree_ends(&t.entries);
        t.remove(EntryId(14), &ends);
        let asked = std::cell::RefCell::new(Vec::new());
        let program = |id: EntryId| {
            asked.borrow_mut().push(id.0);
            id == EntryId(9)
        };
        let r = zero_byte_files(&t, &ends, EntryId(0), 100, &program, &|id| {
            id == EntryId(13)
        });
        let names: Vec<&str> = r.files.iter().map(|&id| &*t.entry(id).name).collect();
        assert_eq!(names, ["New Text Document.txt", "empty.log"]);
        assert_eq!(r.matched, 2);
        // every folder is asked at most once (.git is left out by its name first)
        let mut seen = asked.borrow().clone();
        seen.sort_unstable();
        let n = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), n, "{seen:?}");
        assert!(!seen.contains(&6));

        let r = zero_byte_files(&t, &ends, EntryId(0), 1, &|_| false, &|_| false);
        assert_eq!(r.files.len(), 1);
        // nothing is a program folder or refused: 1, 8, 11, 13 (the .git one stays out)
        assert_eq!(r.matched, 4);
        // scoped inside a program folder: its ancestors are asked too
        let r = zero_byte_files(&t, &ends, EntryId(10), 100, &|id| id == EntryId(9), &|_| {
            false
        });
        assert_eq!(r.matched, 0);
    }
}
