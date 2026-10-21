//! Which copy stays (PHASE5_DUPLICATES §2.2).
//!
//! Whatever the strategy, a copy that is risky to delete — in an application
//! folder, a cloud-sync folder or a git working tree, or read-only — is kept before a
//! plain one, and is never pre-selected for removal: removing it stays a decision
//! the user makes copy by copy. Ties end on the path, so the choice is stable.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::collect::key;
use crate::folders::{FolderCopy, FolderGroup};
use crate::{DupeGroup, FileCopy};

/// How the copy to keep is chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "dirs", rename_all = "kebab-case")]
pub enum KeepStrategy {
    /// Outside Downloads / Temp / Desktop first, then the oldest (usually the
    /// original).
    #[default]
    Auto,
    Oldest,
    Newest,
    /// The shortest path.
    Shortest,
    /// Inside one of these folders first, then [`KeepStrategy::Auto`].
    Prefer(Vec<PathBuf>),
}

impl FromStr for KeepStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "oldest" => Ok(Self::Oldest),
            "newest" => Ok(Self::Newest),
            "shortest" => Ok(Self::Shortest),
            _ => match s.split_once(':') {
                Some((k, dir)) if k.eq_ignore_ascii_case("prefer") && !dir.is_empty() => {
                    Ok(Self::Prefer(dir.split(';').map(PathBuf::from).collect()))
                }
                _ => Err(format!(
                    "unknown keep strategy `{s}` (auto, oldest, newest, shortest, prefer:<dir>)"
                )),
            },
        }
    }
}

/// Folder names whose copies are usually the stray ones.
const DISPOSABLE: &[&str] = &["downloads", "temp", "tmp", "desktop"];

/// In Downloads, Temp or on the Desktop.
#[must_use]
pub fn disposable(path: &Path) -> bool {
    disposable_below(path, 0, false)
}

/// [`disposable`], looking only at the folders below the first `skip` components
/// (the part all copies of a group share tells them apart from nothing).
fn disposable_below(path: &Path, skip: usize, is_dir: bool) -> bool {
    let k = key(path);
    let parts: Vec<&str> = k.split('/').collect();
    let folders = if is_dir {
        &parts[..]
    } else {
        &parts[..parts.len().saturating_sub(1)]
    };
    folders
        .iter()
        .skip(skip)
        .any(|part| DISPOSABLE.contains(part))
}

/// Number of leading folders every path of a group shares.
fn shared_folders<'a>(paths: impl Iterator<Item = &'a Path>) -> usize {
    let keys: Vec<String> = paths.map(key).collect();
    let split: Vec<Vec<&str>> = keys.iter().map(|k| k.split('/').collect()).collect();
    let Some(first) = split.first() else { return 0 };
    let mut n = 0;
    while n + 1 < first.len() && split.iter().all(|p| n + 1 < p.len() && p[n] == first[n]) {
        n += 1;
    }
    n
}

/// Risky (or pointless: an APFS clone frees nothing) to delete: never pre-selected
/// for removal.
#[must_use]
pub const fn sticky(c: &FileCopy) -> bool {
    c.flags.app_dir || c.flags.cloud || c.flags.git || c.flags.readonly || c.flags.clone
}

fn inside(path: &Path, dirs: &[PathBuf]) -> bool {
    let p = key(path);
    dirs.iter().any(|d| {
        let d = key(d);
        p.len() > d.len() && p.starts_with(&d) && p.as_bytes()[d.len()] == b'/'
    })
}

/// Index of the copy to keep in `group` (never `None` for a real group).
#[must_use]
pub fn keeper(group: &DupeGroup, strategy: &KeepStrategy) -> Option<usize> {
    let items: Vec<(&Path, i64, bool)> = group
        .copies
        .iter()
        .map(|c| (c.path.as_path(), c.mtime, sticky(c)))
        .collect();
    pick(&items, strategy, false)
}

/// Risky to delete: a folder the gate refuses, or one holding risky files.
#[must_use]
pub const fn sticky_folder(f: &FolderCopy) -> bool {
    f.blocked || f.flags.app_dir || f.flags.cloud || f.flags.git || f.flags.readonly
}

/// Index of the folder to keep in `group`.
#[must_use]
pub fn folder_keeper(group: &FolderGroup, strategy: &KeepStrategy) -> Option<usize> {
    let items: Vec<(&Path, i64, bool)> = group
        .folders
        .iter()
        .map(|f| (f.path.as_path(), f.mtime, sticky_folder(f)))
        .collect();
    pick(&items, strategy, true)
}

/// The item to keep among (path, last write, risky) by `strategy`.
fn pick(items: &[(&Path, i64, bool)], strategy: &KeepStrategy, dirs: bool) -> Option<usize> {
    let shared = shared_folders(items.iter().map(|i| i.0));
    let rank = |&(path, mtime, risky): &(&Path, i64, bool)| {
        let preferred = match strategy {
            KeepStrategy::Prefer(dirs) => inside(path, dirs),
            _ => false,
        };
        let by_strategy = match strategy {
            KeepStrategy::Auto | KeepStrategy::Prefer(_) => {
                (i64::from(disposable_below(path, shared, dirs)), mtime, 0)
            }
            KeepStrategy::Oldest => (0, mtime, 0),
            KeepStrategy::Newest => (0, -mtime, 0),
            KeepStrategy::Shortest => (0, 0, path.as_os_str().len() as i64),
        };
        (!preferred, !risky, by_strategy)
    };
    items
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| rank(a).cmp(&rank(b)).then_with(|| a.0.cmp(b.0)))
        .map(|(i, _)| i)
}

/// What to do with one group: the copy that stays and the copies that go (paths as
/// in [`FileCopy::path`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub group: u32,
    pub keep: PathBuf,
    pub remove: Vec<PathBuf>,
}

/// The default selection of every group: one keeper by `strategy`, and every other
/// copy that is not risky to delete. Groups with nothing to remove are left out.
#[must_use]
pub fn auto_select(groups: &[DupeGroup], strategy: &KeepStrategy) -> Vec<Selection> {
    groups
        .iter()
        .filter_map(|g| {
            let k = keeper(g, strategy)?;
            let remove: Vec<PathBuf> = g
                .copies
                .iter()
                .enumerate()
                .filter(|(i, c)| *i != k && !sticky(c))
                .map(|(_, c)| c.path.clone())
                .collect();
            (!remove.is_empty()).then(|| Selection {
                group: g.id,
                keep: g.copies[k].path.clone(),
                remove,
            })
        })
        .collect()
}

/// [`auto_select`] for duplicate folders: risky folders (refused by the gate, or
/// holding cloud-sync, git, application or read-only files) are never selected.
#[must_use]
pub fn auto_select_folders(groups: &[FolderGroup], strategy: &KeepStrategy) -> Vec<Selection> {
    groups
        .iter()
        .filter_map(|g| {
            let k = folder_keeper(g, strategy)?;
            let remove: Vec<PathBuf> = g
                .folders
                .iter()
                .enumerate()
                .filter(|(i, f)| *i != k && !sticky_folder(f))
                .map(|(_, f)| f.path.clone())
                .collect();
            (!remove.is_empty()).then(|| Selection {
                group: g.id,
                keep: g.folders[k].path.clone(),
                remove,
            })
        })
        .collect()
}
