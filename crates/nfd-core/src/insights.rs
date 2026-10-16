//! Ready-made questions about a scan (PLAN §13 A2 / A3), answered on the in-memory
//! arena with [`crate::query`] filters: no filesystem access, one pass each.
//!
//! * [`old_and_large`] — very large files, large files nobody touched for a long
//!   time, and old installers / disk images left in `Downloads` folders.
//! * [`empty_folders`] — folders that contain nothing but (possibly nested) empty
//!   folders.
//!
//! Both only *list*; anything the user then removes still goes through
//! [`crate::safety`].

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::model::{EntryId, EntryKind};
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

/// One empty folder tree found by [`empty_folders`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EmptyFolder {
    pub id: EntryId,
    /// Empty sub-folders inside it (0 = the folder itself is empty).
    pub nested: u64,
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

/// Folders strictly below `under` that hold nothing but empty folders. Only the
/// outermost one of a nested empty tree is listed. Folders the scanner could not
/// read (see [`ScanTree::issues`]) are never reported as empty, nor are their
/// parents. Removed entries are ignored.
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
    // has_content: the subtree holds something other than listable empty folders.
    let mut has_content = vec![false; entries.len()];
    let mut kept_out = vec![false; entries.len()];
    let mut dirs_below = vec![0u64; entries.len()];
    for e in entries.iter().rev() {
        if e.removed {
            continue;
        }
        let i = e.id.index() - base;
        if e.kind != EntryKind::Dir || (!has_content[i] && is_unreadable(e.id)) {
            has_content[i] = true;
        } else if !has_content[i] && keep_out(e.id) {
            kept_out[i] = true;
        }
        let Some(p) = e.parent.filter(|&p| p != under) else {
            continue;
        };
        let p = p.index() - base;
        has_content[p] |= has_content[i] || kept_out[i];
        if e.kind == EntryKind::Dir && !kept_out[i] {
            dirs_below[p] += dirs_below[i] + 1;
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
            });
        }
    }
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

    use EntryKind::{Dir, File, Junction};

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
    fn scanning_downloads_itself_counts() {
        let mut t = tree(&[
            (None, "Downloads", Dir, 0, 0),
            (Some(0), "a.zip", File, MB, 0),
        ]);
        t.root = PathBuf::from(r"C:\Users\x\Downloads");
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
                nested: 1
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
}
