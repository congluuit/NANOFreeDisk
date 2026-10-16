//! Step 1 of the pipeline: which files of a scan may take part at all.
//!
//! One pass over each scan arena (pre-order, so a folder is always seen before its
//! contents) carries per-folder facts down the tree: inside a cloud-sync folder,
//! inside a git working tree, inside an application folder, refused by the
//! SafetyGate. Files are then left out for being empty or small, placeholders,
//! reparse points, SYSTEM files, protected (Windows, Program Files, credentials, AI
//! conversation history…) or inside an application folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nfd_core::model::{Attrs, EntryKind};
use nfd_core::safety::{Blocked, Origin, SafetyGate, normalize};
use nfd_core::scanner::ScanTree;

use crate::hash::Source;
use crate::{CopyFlags, DupeOptions, SkipReason, Skipped};

/// Folder names whose contents an application needs complete: a "duplicate" in
/// there is part of an install, an environment or a package store.
pub const APP_DIR_NAMES: &[&str] = &[
    "node_modules",
    ".git",
    ".venv",
    "site-packages",
    "steamapps",
    "windowsapps",
    "epic games",
    "gog games",
    "gog galaxy",
    ".nfd-quarantine",
    ".rustup",
    ".cargo",
    ".nuget",
    ".gradle",
    ".m2",
    ".pnpm-store",
];

/// Folder names of cloud-sync clients: deleting a copy there deletes it in the
/// cloud and on every other synced PC.
const CLOUD_NAMES: &[&str] = &[
    "onedrive",
    "dropbox",
    "google drive",
    "my drive",
    "iclouddrive",
    "icloud drive",
    "box",
    "mega",
    "pcloud drive",
];

fn is_cloud_name(name: &str) -> bool {
    CLOUD_NAMES.contains(&name) || name.starts_with("onedrive - ")
}

/// Application folders by location: `%LOCALAPPDATA%\Programs`, Program Files,
/// ProgramData (most are protected anyway).
#[must_use]
pub fn default_app_dirs(resolve: &impl Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    [
        ("LOCALAPPDATA", "/Programs"),
        ("PROGRAMFILES", ""),
        ("PROGRAMFILES_X86", ""),
        ("PROGRAMDATA", ""),
    ]
    .iter()
    .filter_map(|(var, tail)| resolve(var).map(|base| PathBuf::from(format!("{base}{tail}"))))
    .collect()
}

/// Lower-case `/` form without a `\\?\` prefix, for prefix tests.
pub(crate) fn key(path: &Path) -> String {
    let s = normalize(&path.to_string_lossy());
    s.strip_prefix("//?/").map(str::to_owned).unwrap_or(s)
}

fn under(key: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|p| {
        key == p
            || (key.len() > p.len()
                && key.starts_with(p.as_str())
                && key.as_bytes()[p.len()] == b'/')
    })
}

/// A file that may be a duplicate.
#[derive(Debug, Clone)]
pub(crate) struct Cand {
    /// Scan (index in the trees given) and entry it comes from.
    pub tree: usize,
    pub entry: u32,
    pub path: PathBuf,
    pub size: u64,
    pub mtime: i64,
    pub volume: u64,
    pub file_id: Option<u128>,
    pub flags: CopyFlags,
}

/// Skip counters and a few examples.
#[derive(Debug, Default)]
pub(crate) struct Skips {
    pub counts: BTreeMap<SkipReason, u64>,
    pub samples: Vec<Skipped>,
}

/// Examples kept per run (counts are always complete).
const SAMPLES: usize = 200;

impl Skips {
    pub fn add(&mut self, reason: SkipReason, path: impl FnOnce() -> PathBuf) {
        *self.counts.entry(reason).or_default() += 1;
        // small and empty files are too common to be worth an example
        if self.samples.len() < SAMPLES && !matches!(reason, SkipReason::Small | SkipReason::Empty)
        {
            self.samples.push(Skipped {
                path: path(),
                reason,
            });
        }
    }
}

#[derive(Debug, Clone, Default)]
struct DirState {
    path: PathBuf,
    flags: CopyFlags,
    blocked: bool,
}

/// `true` when the gate's answer for a *folder* applies to everything inside it.
fn blocks_subtree(b: &Blocked) -> bool {
    !matches!(
        b,
        Blocked::ContainsProtected { .. } | Blocked::VolumeRoot | Blocked::ContainsMountPoint
    )
}

fn has_git_above(root: &Path) -> bool {
    root.ancestors().skip(1).any(|a| a.join(".git").exists())
}

/// Candidates of every tree, plus the number of files looked at.
pub(crate) fn collect(
    trees: &[&ScanTree],
    gate: &SafetyGate,
    opts: &DupeOptions,
    source: &dyn Source,
    skips: &mut Skips,
) -> (Vec<Cand>, u64) {
    let app_prefixes: Vec<String> = opts.app_dir_paths.iter().map(|p| key(p)).collect();
    let exts: Vec<String> = opts
        .extensions
        .iter()
        .map(|e| e.trim_start_matches('.').to_lowercase())
        .filter(|e| !e.is_empty())
        .collect();
    let min = opts.min_size.max(1);
    let mut out = Vec::new();
    let mut files = 0u64;
    for (t, tree) in trees.iter().enumerate() {
        let Some(root) = tree.entries.first() else {
            continue;
        };
        let volume = source.info(&tree.root).map_or(0, |i| i.volume);
        // folders with a `.git` entry are the top of a working tree
        let mut git_top = vec![false; tree.entries.len()];
        for e in &tree.entries {
            if let Some(p) = e.parent {
                if e.name.eq_ignore_ascii_case(".git") {
                    git_top[p.index()] = true;
                }
            }
        }
        let mut dirs: Vec<Option<DirState>> = vec![None; tree.entries.len()];
        let root_key = key(&tree.root);
        let mut root_state = DirState {
            path: tree.root.clone(),
            flags: CopyFlags::default(),
            blocked: false,
        };
        for part in root_key.split('/') {
            root_state.flags.cloud |= is_cloud_name(part);
            root_state.flags.app_dir |= APP_DIR_NAMES.contains(&part);
        }
        root_state.flags.app_dir |= under(&root_key, &app_prefixes);
        root_state.flags.git = git_top[0] || has_git_above(&tree.root);
        if root.kind == EntryKind::Dir {
            root_state.blocked = gate
                .check_path(&tree.root, Origin::Manual)
                .err()
                .is_some_and(|b| blocks_subtree(&b));
        }
        dirs[0] = Some(root_state);

        for e in tree.entries.iter().skip(1) {
            let Some(parent) = e.parent.and_then(|p| dirs[p.index()].as_ref()) else {
                continue;
            };
            match e.kind {
                EntryKind::Dir => {
                    let name = e.name.to_lowercase();
                    let path = parent.path.join(&*e.name);
                    let mut flags = parent.flags;
                    flags.cloud |= is_cloud_name(&name);
                    flags.app_dir |=
                        APP_DIR_NAMES.contains(&name.as_str()) || under(&key(&path), &app_prefixes);
                    flags.git |= git_top[e.id.index()];
                    let blocked = parent.blocked
                        || gate
                            .check_path(&path, Origin::Manual)
                            .err()
                            .is_some_and(|b| blocks_subtree(&b));
                    dirs[e.id.index()] = Some(DirState {
                        path,
                        flags,
                        blocked,
                    });
                }
                EntryKind::File if !e.removed => {
                    files += 1;
                    let path = || parent.path.join(&*e.name);
                    if e.logical == 0 {
                        skips.add(SkipReason::Empty, path);
                        continue;
                    }
                    if e.logical < min {
                        skips.add(SkipReason::Small, path);
                        continue;
                    }
                    if !exts.is_empty() {
                        let ext = e
                            .name
                            .rsplit_once('.')
                            .map(|(_, x)| x.to_lowercase())
                            .unwrap_or_default();
                        if !exts.contains(&ext) {
                            skips.add(SkipReason::Extension, path);
                            continue;
                        }
                    }
                    let reason = if e.attrs.contains(Attrs::OFFLINE) {
                        Some(SkipReason::Cloud)
                    } else if e.attrs.contains(Attrs::REPARSE) {
                        Some(SkipReason::Reparse)
                    } else if e.attrs.contains(Attrs::SYSTEM) {
                        Some(SkipReason::System)
                    } else if parent.blocked {
                        Some(SkipReason::Protected)
                    } else if parent.flags.app_dir && !opts.include_app_dirs {
                        Some(SkipReason::AppDir)
                    } else {
                        None
                    };
                    if let Some(r) = reason {
                        skips.add(r, path);
                        continue;
                    }
                    let path = path();
                    if gate.check_path(&path, Origin::Manual).is_err() {
                        skips.add(SkipReason::Protected, || path);
                        continue;
                    }
                    let mut flags = parent.flags;
                    flags.readonly = e.attrs.contains(Attrs::READONLY);
                    out.push(Cand {
                        tree: t,
                        entry: e.id.0,
                        path,
                        size: e.logical,
                        mtime: e.mtime,
                        volume,
                        file_id: e.file_id,
                        flags,
                    });
                }
                _ => {}
            }
        }
    }
    (out, files)
}
