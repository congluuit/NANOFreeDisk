//! Entry model produced by the scanner.
//!
//! A scan is an arena (`Vec<Entry>`) in depth-first pre-order: every entry's parent
//! has a smaller [`EntryId`] than the entry itself. [`roll_up`] relies on that to
//! aggregate directory sizes bottom-up in one reverse pass, and [`ChildIndex`] gives
//! the parent → children view a tree UI needs.

use serde::{Deserialize, Serialize};

/// Index of an [`Entry`] inside the scan arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntryId(pub u32);

/// What kind of filesystem object an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    /// NTFS directory junction — displayed, never followed, never counted twice.
    Junction,
    /// Volume mount point — a hard boundary for scanning and deleting.
    MountPoint,
    Other,
}

/// Attribute bits relevant to sizing and safety decisions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Attrs(pub u32);

impl Attrs {
    pub const SYSTEM: Self = Self(1 << 0);
    pub const HIDDEN: Self = Self(1 << 1);
    pub const REPARSE: Self = Self(1 << 2);
    pub const COMPRESSED: Self = Self(1 << 3);
    pub const SPARSE: Self = Self(1 << 4);
    /// Cloud placeholder (OneDrive etc.): logical size exists, on-disk size is ~0.
    pub const OFFLINE: Self = Self(1 << 5);
    pub const READONLY: Self = Self(1 << 6);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Lower-case names of the set bits, in bit order (used by JSON/CSV reports).
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        const NAMES: [(Attrs, &str); 7] = [
            (Attrs::SYSTEM, "system"),
            (Attrs::HIDDEN, "hidden"),
            (Attrs::REPARSE, "reparse"),
            (Attrs::COMPRESSED, "compressed"),
            (Attrs::SPARSE, "sparse"),
            (Attrs::OFFLINE, "offline"),
            (Attrs::READONLY, "readonly"),
        ];
        NAMES
            .iter()
            .filter(|(bit, _)| self.contains(*bit))
            .map(|(_, name)| *name)
            .collect()
    }
}

impl EntryId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One file or directory in a scan result.
///
/// For [`EntryKind::Dir`] entries, `logical` / `on_disk` hold the *rolled-up* totals of
/// everything below the directory (see [`roll_up`]). Links (symlink, junction, mount
/// point) have size 0 and are never descended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: EntryId,
    pub parent: Option<EntryId>,
    pub name: Box<str>,
    pub kind: EntryKind,
    /// Logical size in bytes (what the file claims).
    pub logical: u64,
    /// Allocated size in bytes (what it really occupies: cluster-aligned, compression/sparse aware).
    pub on_disk: u64,
    /// Modification time as Unix seconds.
    pub mtime: i64,
    pub attrs: Attrs,
    /// Number of hard links; `>1` means the bytes are shared with another path.
    pub hardlink_count: u16,
    /// NTFS file reference number (or inode elsewhere) used to count hard-linked data once.
    pub file_id: Option<u128>,
    /// This file is another name for hard-linked data already counted at an earlier
    /// path in the same scan; its bytes are excluded from every total.
    #[serde(default)]
    pub hardlink_dup: bool,
    /// Deleted after the scan (see [`crate::scanner::ScanTree::remove`]): kept in the
    /// arena so ids stay valid, but counted nowhere.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
}

impl Entry {
    /// Bytes this entry contributes to its parent's totals: own sizes for counted
    /// files, rolled-up totals for directories, 0 for links, hard-link duplicates and
    /// removed entries.
    #[must_use]
    pub const fn counted(&self) -> (u64, u64) {
        if self.removed {
            return (0, 0);
        }
        match self.kind {
            EntryKind::File | EntryKind::Other if !self.hardlink_dup => {
                (self.logical, self.on_disk)
            }
            EntryKind::Dir => (self.logical, self.on_disk),
            _ => (0, 0),
        }
    }
}

/// Aggregate directory sizes bottom-up, in place, and return the number of counted
/// files below (and including) each entry.
///
/// Directory entries must start with `logical == on_disk == 0`. The arena must be in
/// pre-order (parent id < child id), which the scanner guarantees.
pub fn roll_up(entries: &mut [Entry]) -> Vec<u64> {
    let mut files: Vec<u64> = entries
        .iter()
        .map(|e| u64::from(matches!(e.kind, EntryKind::File) && !e.hardlink_dup))
        .collect();
    for i in (0..entries.len()).rev() {
        let Some(parent) = entries[i].parent else {
            continue;
        };
        let p = parent.index();
        debug_assert!(p < i, "arena must be in pre-order");
        let (logical, on_disk) = entries[i].counted();
        let parent = &mut entries[p];
        parent.logical += logical;
        parent.on_disk += on_disk;
        files[p] += files[i];
    }
    files
}

/// One past the last descendant of every entry: the subtree of `id` is the contiguous
/// arena range `id..ends[id]` (pre-order).
#[must_use]
pub fn subtree_ends(entries: &[Entry]) -> Vec<u32> {
    let mut ends: Vec<u32> = (1..=entries.len())
        .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
        .collect();
    for i in (0..entries.len()).rev() {
        if let Some(p) = entries[i].parent {
            let end = ends[i];
            let slot = &mut ends[p.index()];
            *slot = (*slot).max(end);
        }
    }
    ends
}

/// Parent → children lookup over a scan arena (compressed sparse rows).
#[derive(Debug, Clone, Default)]
pub struct ChildIndex {
    offsets: Vec<u32>,
    children: Vec<EntryId>,
}

impl ChildIndex {
    /// Build the index in O(n). Children keep arena order (sorted by name per directory).
    #[must_use]
    pub fn build(entries: &[Entry]) -> Self {
        let mut offsets = vec![0u32; entries.len() + 1];
        for e in entries {
            if let Some(p) = e.parent {
                offsets[p.index() + 1] += 1;
            }
        }
        for i in 1..offsets.len() {
            offsets[i] += offsets[i - 1];
        }
        let mut cursor = offsets.clone();
        let mut children = vec![EntryId(0); entries.len().saturating_sub(1)];
        for e in entries {
            if let Some(p) = e.parent {
                let slot = &mut cursor[p.index()];
                children[*slot as usize] = e.id;
                *slot += 1;
            }
        }
        Self { offsets, children }
    }

    /// Direct children of `id` (empty for files and unknown ids).
    #[must_use]
    pub fn children(&self, id: EntryId) -> &[EntryId] {
        let i = id.index();
        match (self.offsets.get(i), self.offsets.get(i + 1)) {
            (Some(&a), Some(&b)) => &self.children[a as usize..b as usize],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attrs_bit_ops() {
        let a = Attrs::SYSTEM.with(Attrs::HIDDEN);
        assert!(a.contains(Attrs::SYSTEM));
        assert!(a.contains(Attrs::HIDDEN));
        assert!(!a.contains(Attrs::OFFLINE));
        assert_eq!(a.names(), ["system", "hidden"]);
    }

    fn entry(id: u32, parent: Option<u32>, kind: EntryKind, size: u64) -> Entry {
        Entry {
            id: EntryId(id),
            parent: parent.map(EntryId),
            name: format!("e{id}").into(),
            kind,
            logical: size,
            on_disk: size * 2,
            mtime: 0,
            attrs: Attrs::default(),
            hardlink_count: 1,
            file_id: None,
            hardlink_dup: false,
            removed: false,
        }
    }

    #[test]
    fn roll_up_and_children() {
        // 0 root/ ─┬ 1 a/ ─┬ 2 x (10)
        //          │       └ 3 y (5, hard-link duplicate)
        //          ├ 4 j (junction)
        //          └ 5 z (1)
        let mut entries = vec![
            entry(0, None, EntryKind::Dir, 0),
            entry(1, Some(0), EntryKind::Dir, 0),
            entry(2, Some(1), EntryKind::File, 10),
            entry(3, Some(1), EntryKind::File, 5),
            entry(4, Some(0), EntryKind::Junction, 0),
            entry(5, Some(0), EntryKind::File, 1),
        ];
        entries[3].hardlink_dup = true;
        let files = roll_up(&mut entries);
        assert_eq!((entries[1].logical, entries[1].on_disk), (10, 20));
        assert_eq!((entries[0].logical, entries[0].on_disk), (11, 22));
        assert_eq!(files, [2, 1, 1, 0, 0, 1]);

        assert_eq!(subtree_ends(&entries), [6, 4, 3, 4, 5, 6]);

        let idx = ChildIndex::build(&entries);
        assert_eq!(
            idx.children(EntryId(0)),
            [EntryId(1), EntryId(4), EntryId(5)]
        );
        assert_eq!(idx.children(EntryId(1)), [EntryId(2), EntryId(3)]);
        assert!(idx.children(EntryId(2)).is_empty());
        assert!(idx.children(EntryId(99)).is_empty());
    }
}
