//! Search and filters over a scan result (the Analyze view's search box and filter
//! bar, PLAN §4.1 / A4).
//!
//! Everything runs on the in-memory arena: no filesystem access, ~1M entries in tens
//! of milliseconds. Results are ranked by counted on-disk size.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::{Attrs, Entry, EntryId, EntryKind};
use crate::scanner::ScanTree;

/// Which entry kinds a [`Filter`] accepts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KindFilter {
    #[default]
    Any,
    Files,
    Dirs,
}

/// Search criteria; every set field must match. The default filter matches every
/// counted entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    /// Case-insensitive name pattern: a substring, or a glob when it contains `*` or
    /// `?` (matched against the whole name).
    pub name: Option<String>,
    /// Lower-case extensions without the dot (files only); empty = any. `""` matches
    /// files without an extension.
    pub exts: Vec<String>,
    /// Counted on-disk size bounds, inclusive.
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    /// Modification time bounds (Unix seconds), inclusive.
    pub modified_after: Option<i64>,
    pub modified_before: Option<i64>,
    pub kind: KindFilter,
    /// Every one of these attribute bits must be set (e.g. only cloud placeholders).
    pub attrs_all: u32,
    /// None of these attribute bits may be set (e.g. hide hidden/system).
    pub attrs_none: u32,
}

/// Matches of a [`search`], largest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SearchResult {
    pub ids: Vec<EntryId>,
    /// Number of matching entries (may exceed `ids.len()`).
    pub matched: u64,
    /// Counted on-disk bytes of all matches, nested matches included once each (a
    /// matching directory and a matching file inside it are both added).
    pub matched_on_disk: u64,
}

impl Filter {
    /// `true` when no criterion is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Prepared form with the pattern lower-cased once; use it to test many entries.
    #[must_use]
    pub fn matcher(&self) -> Matcher<'_> {
        let name = self
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_lowercase);
        let glob = name.as_deref().is_some_and(|n| n.contains(['*', '?']));
        Matcher {
            filter: self,
            name,
            glob,
        }
    }
}

/// A [`Filter`] prepared for matching (see [`Filter::matcher`]).
#[derive(Debug)]
pub struct Matcher<'a> {
    filter: &'a Filter,
    name: Option<String>,
    glob: bool,
}

impl Matcher<'_> {
    /// `true` when `e` is a live, counted entry meeting every criterion.
    #[must_use]
    pub fn matches(&self, e: &Entry) -> bool {
        let f = self.filter;
        if e.removed || e.hardlink_dup {
            return false;
        }
        match f.kind {
            KindFilter::Any => {}
            KindFilter::Files if e.kind == EntryKind::File => {}
            KindFilter::Dirs if e.kind == EntryKind::Dir => {}
            _ => return false,
        }
        let (_, on_disk) = e.counted();
        if f.min_size.is_some_and(|m| on_disk < m) || f.max_size.is_some_and(|m| on_disk > m) {
            return false;
        }
        if f.modified_after.is_some_and(|t| e.mtime < t)
            || f.modified_before.is_some_and(|t| e.mtime > t)
        {
            return false;
        }
        if !e.attrs.contains(Attrs(f.attrs_all)) || e.attrs.0 & f.attrs_none != 0 {
            return false;
        }
        if !f.exts.is_empty() {
            if e.kind != EntryKind::File {
                return false;
            }
            let ext = extension(&e.name);
            if !f.exts.iter().any(|x| x.eq_ignore_ascii_case(&ext)) {
                return false;
            }
        }
        if let Some(pattern) = &self.name {
            let name = e.name.to_lowercase();
            let hit = if self.glob {
                glob_match(pattern.as_bytes(), name.as_bytes())
            } else {
                name.contains(pattern.as_str())
            };
            if !hit {
                return false;
            }
        }
        true
    }
}

/// Lower-case extension of a file name without the dot (`""` when there is none).
#[must_use]
pub fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|x| x.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Find entries strictly below `under` (use the root id for the whole tree) that
/// match `filter`, keeping the `limit` largest by counted on-disk size. `ends` comes
/// from [`crate::model::subtree_ends`].
#[must_use]
pub fn search(
    tree: &ScanTree,
    ends: &[u32],
    under: EntryId,
    filter: &Filter,
    limit: usize,
) -> SearchResult {
    let start = under.index() + 1;
    let end = ends.get(under.index()).map_or(start, |&e| e as usize);
    let matcher = filter.matcher();
    let mut ranked = Ranked::default();
    for e in tree.entries.get(start..end).unwrap_or_default() {
        if matcher.matches(e) {
            ranked.push(e);
        }
    }
    ranked.finish(limit)
}

/// Collects matches and keeps the largest (see [`search`]); for callers that run
/// several filters in one pass over the arena.
#[derive(Debug, Clone, Default)]
pub struct Ranked {
    hits: Vec<(u64, EntryId)>,
    matched_on_disk: u64,
}

impl Ranked {
    /// Record a match (its counted on-disk size is the ranking key).
    pub fn push(&mut self, e: &Entry) {
        let on_disk = e.counted().1;
        self.matched_on_disk += on_disk;
        self.hits.push((on_disk, e.id));
    }

    /// The `limit` largest matches, largest first (ties: arena order).
    #[must_use]
    pub fn finish(mut self, limit: usize) -> SearchResult {
        let key = |&(size, id): &(u64, EntryId)| (std::cmp::Reverse(size), id);
        let matched = self.hits.len() as u64;
        if limit < self.hits.len() {
            self.hits.select_nth_unstable_by_key(limit, key);
            self.hits.truncate(limit);
        }
        self.hits.sort_unstable_by_key(key);
        SearchResult {
            ids: self.hits.into_iter().map(|(_, id)| id).collect(),
            matched,
            matched_on_disk: self.matched_on_disk,
        }
    }
}

/// Glob match with `*` (any run) and `?` (one byte), iterative with backtracking.
fn glob_match(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((sp, st)) = star {
            p = sp + 1;
            t = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::model::{roll_up, subtree_ends};
    use crate::scanner::{ScanMode, ScanStats};

    fn entry(id: u32, parent: Option<u32>, name: &str, kind: EntryKind, size: u64) -> Entry {
        Entry {
            id: EntryId(id),
            parent: parent.map(EntryId),
            name: name.into(),
            kind,
            logical: size,
            on_disk: size,
            mtime: i64::from(id) * 100,
            attrs: Attrs::default(),
            hardlink_count: 1,
            file_id: None,
            hardlink_dup: false,
            removed: false,
        }
    }

    /// root/ ─┬ 1 src/ ─┬ 2 main.rs (10)
    ///        │         └ 3 big.bin (500)
    ///        ├ 4 movie.MKV (900, hidden)
    ///        └ 5 notes (3)
    fn tree() -> ScanTree {
        let mut entries = vec![
            entry(0, None, "root", EntryKind::Dir, 0),
            entry(1, Some(0), "src", EntryKind::Dir, 0),
            entry(2, Some(1), "main.rs", EntryKind::File, 10),
            entry(3, Some(1), "big.bin", EntryKind::File, 500),
            entry(4, Some(0), "movie.MKV", EntryKind::File, 900),
            entry(5, Some(0), "notes", EntryKind::File, 3),
        ];
        entries[4].attrs = Attrs::HIDDEN;
        let file_counts = roll_up(&mut entries);
        ScanTree {
            root: PathBuf::from("/root"),
            stats: ScanStats {
                files: 4,
                dirs: 1,
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

    fn names(tree: &ScanTree, r: &SearchResult) -> Vec<String> {
        r.ids
            .iter()
            .map(|&id| tree.entry(id).name.to_string())
            .collect()
    }

    #[test]
    fn empty_filter_ranks_everything_by_size() {
        let t = tree();
        let ends = subtree_ends(&t.entries);
        let r = search(&t, &ends, EntryId(0), &Filter::default(), 10);
        assert_eq!(
            names(&t, &r),
            ["movie.MKV", "src", "big.bin", "main.rs", "notes"]
        );
        assert_eq!(r.matched, 5);
        let r = search(&t, &ends, EntryId(0), &Filter::default(), 2);
        assert_eq!(names(&t, &r), ["movie.MKV", "src"]);
        assert_eq!(r.matched, 5);
    }

    #[test]
    fn name_substring_and_glob() {
        let t = tree();
        let ends = subtree_ends(&t.entries);
        let by = |name: &str| Filter {
            name: Some(name.into()),
            ..Filter::default()
        };
        let r = search(&t, &ends, EntryId(0), &by("MAIN"), 10);
        assert_eq!(names(&t, &r), ["main.rs"]);
        let r = search(&t, &ends, EntryId(0), &by("*.mkv"), 10);
        assert_eq!(names(&t, &r), ["movie.MKV"]);
        let r = search(&t, &ends, EntryId(0), &by("b?g*"), 10);
        assert_eq!(names(&t, &r), ["big.bin"]);
        let r = search(&t, &ends, EntryId(0), &by("*"), 10);
        assert_eq!(r.matched, 5);
    }

    #[test]
    fn ext_size_kind_attr_and_scope() {
        let t = tree();
        let ends = subtree_ends(&t.entries);
        let f = Filter {
            exts: vec!["mkv".into(), String::new()],
            ..Filter::default()
        };
        assert_eq!(
            names(&t, &search(&t, &ends, EntryId(0), &f, 10)),
            ["movie.MKV", "notes"]
        );
        let f = Filter {
            min_size: Some(100),
            kind: KindFilter::Files,
            attrs_none: Attrs::HIDDEN.0,
            ..Filter::default()
        };
        assert_eq!(
            names(&t, &search(&t, &ends, EntryId(0), &f, 10)),
            ["big.bin"]
        );
        let f = Filter {
            kind: KindFilter::Dirs,
            ..Filter::default()
        };
        assert_eq!(names(&t, &search(&t, &ends, EntryId(0), &f, 10)), ["src"]);
        // scoped to src/
        let r = search(&t, &ends, EntryId(1), &Filter::default(), 10);
        assert_eq!(names(&t, &r), ["big.bin", "main.rs"]);
        let f = Filter {
            modified_after: Some(250),
            modified_before: Some(450),
            ..Filter::default()
        };
        assert_eq!(
            names(&t, &search(&t, &ends, EntryId(0), &f, 10)),
            ["movie.MKV", "big.bin"]
        );
    }

    #[test]
    fn remove_updates_totals_and_hides_entries() {
        let mut t = tree();
        let ends = subtree_ends(&t.entries);
        assert_eq!(t.entries[0].on_disk, 1413);
        assert_eq!(t.remove(EntryId(1), &ends), (510, 510, 2));
        assert_eq!(t.entries[0].on_disk, 903);
        assert_eq!(t.file_counts[0], 2);
        assert_eq!(t.stats.files, 2);
        assert!(t.entries[2].removed);
        // idempotent, root is never removed
        assert_eq!(t.remove(EntryId(1), &ends), (0, 0, 0));
        assert_eq!(t.remove(EntryId(0), &ends), (0, 0, 0));
        let r = search(&t, &ends, EntryId(0), &Filter::default(), 10);
        assert_eq!(names(&t, &r), ["movie.MKV", "notes"]);
        assert_eq!(t.top_files(10, 0).len(), 2);
        assert_eq!(t.extensions().iter().map(|x| x.files).sum::<u64>(), 2);
    }

    #[test]
    fn glob_edge_cases() {
        assert!(glob_match(b"*", b""));
        assert!(glob_match(b"a*b*c", b"axxbyyc"));
        assert!(!glob_match(b"a*b*c", b"axxbyy"));
        assert!(glob_match(b"*.tar.gz", b"x.tar.gz"));
        assert!(!glob_match(b"?", b""));
    }
}
