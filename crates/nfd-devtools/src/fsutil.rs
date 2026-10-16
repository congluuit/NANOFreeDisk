//! Folder totals for the Dev Workspace lists: never follows links (junctions, symlinks,
//! mount points are counted as nothing), stops after a number of entries.

use std::path::Path;

use nfd_platform::fsinfo::{self, LinkKind};
use serde::Serialize;

/// Size of a folder tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    /// Bytes on disk.
    pub bytes: u64,
    pub files: u64,
    /// Newest last-write time of a file inside (Unix seconds, 0 = none).
    pub newest: i64,
    /// The walk stopped at the entry limit: the numbers are a lower bound.
    pub truncated: bool,
}

/// Total of `dir` (or of a single file), visiting at most `max_entries` entries.
#[must_use]
pub fn totals(dir: &Path, max_entries: u64) -> Totals {
    let mut t = Totals::default();
    let Ok(info) = fsinfo::file_info(dir) else {
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
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(items) = fsinfo::read_dir(&d, false) else {
            continue;
        };
        for item in items {
            seen += 1;
            if seen > max_entries {
                t.truncated = true;
                return t;
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

/// Names of the sub-folders of `dir` (links left out).
#[must_use]
pub fn subdirs(dir: &Path) -> Vec<String> {
    fsinfo::read_dir(dir, false)
        .map(|items| {
            items
                .into_iter()
                .filter(|i| {
                    i.info
                        .as_ref()
                        .is_ok_and(|f| f.is_dir && f.link == LinkKind::None)
                })
                .map(|i| i.name.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_files_and_stops_at_the_limit() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::create_dir_all(dir.path().join("a/b")).expect("mkdir");
        std::fs::write(dir.path().join("a/b/x"), [0u8; 10]).expect("w");
        std::fs::write(dir.path().join("y"), [0u8; 10]).expect("w");
        let t = totals(dir.path(), 100);
        assert_eq!(t.files, 2);
        assert!(!t.truncated);
        assert!(t.newest > 0);
        assert!(totals(dir.path(), 1).truncated);
        assert_eq!(subdirs(dir.path()), ["a"]);
    }
}
