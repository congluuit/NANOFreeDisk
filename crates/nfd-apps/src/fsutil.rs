//! Folder totals that never follow links (junctions, symlinks and mount points
//! count as nothing), stop after a number of entries, and notice conversation
//! history of AI agents inside (never removed by any path — PLAN §5.1).

use std::path::{Path, PathBuf};

use nfd_platform::fsinfo::{self, LinkKind};
use serde::{Deserialize, Serialize};

/// Size of a folder tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    /// Bytes on disk.
    pub bytes: u64,
    pub files: u64,
    /// Newest last-write time of a file inside (Unix seconds, 0 = none).
    pub newest: i64,
    /// The walk stopped at the entry limit: the numbers are a lower bound.
    pub truncated: bool,
    /// The first conversation-history entry found inside, if any.
    #[serde(skip)]
    pub history: Option<PathBuf>,
}

/// Total of `path` (a folder or a single file), visiting at most `max_entries`
/// entries.
#[must_use]
pub fn totals(path: &Path, max_entries: u64) -> Totals {
    let mut t = Totals::default();
    let Ok(info) = fsinfo::file_info(path) else {
        return t;
    };
    if info.link != LinkKind::None {
        return t;
    }
    if !info.is_dir {
        t.bytes = info.on_disk;
        t.files = 1;
        t.newest = info.mtime;
        return t;
    }
    let mut seen = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(items) = fsinfo::read_dir(&d, false) else {
            continue;
        };
        let parent = d.file_name().map(|n| n.to_string_lossy().into_owned());
        for item in items {
            seen += 1;
            if seen > max_entries {
                t.truncated = true;
                return t;
            }
            let name = item.name.to_string_lossy();
            if t.history.is_none()
                && nfd_core::safety::agent_history_entry(parent.as_deref(), &name).is_some()
            {
                t.history = Some(d.join(&item.name));
            }
            let Ok(info) = item.info else { continue };
            if info.link != LinkKind::None {
                continue;
            }
            if info.is_dir {
                stack.push(d.join(&item.name));
            } else {
                t.bytes += info.on_disk;
                t.files += 1;
                t.newest = t.newest.max(info.mtime);
            }
        }
    }
    t
}

/// The entries of `dir` (links left out): `(name, is_dir)`.
#[must_use]
pub fn children(dir: &Path) -> Vec<(String, bool)> {
    fsinfo::read_dir(dir, false)
        .map(|items| {
            items
                .into_iter()
                .filter_map(|i| {
                    let info = i.info.ok()?;
                    (info.link == LinkKind::None)
                        .then(|| (i.name.to_string_lossy().into_owned(), info.is_dir))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Last write of `path` (Unix seconds, 0 when unknown).
#[must_use]
pub fn mtime(path: &Path) -> i64 {
    fsinfo::file_info(path).map_or(0, |i| i.mtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_count_files_and_notice_history() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::create_dir_all(dir.path().join("a/.codex/sessions")).expect("dirs");
        std::fs::write(dir.path().join("a/x.bin"), vec![1u8; 10]).expect("write");
        std::fs::write(dir.path().join("y.log"), b"hello").expect("write");
        let t = totals(dir.path(), 1000);
        assert_eq!(t.files, 2);
        assert!(t.history.is_some());
        let clean = tempfile::tempdir().expect("tmp");
        std::fs::write(clean.path().join("z"), b"1").expect("write");
        assert!(totals(clean.path(), 1000).history.is_none());
        assert!(totals(clean.path(), 0).truncated);
        let mut kids = children(dir.path());
        kids.sort();
        assert_eq!(
            kids,
            vec![("a".to_owned(), true), ("y.log".to_owned(), false)]
        );
    }
}
