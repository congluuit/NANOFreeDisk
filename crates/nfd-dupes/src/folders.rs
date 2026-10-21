//! Duplicate folders (PHASE5_DUPLICATES §2.4): folders whose whole contents —
//! names, sub-folders and file bytes — are identical.
//!
//! Once every file has its full hash, a folder's signature is the BLAKE3 of its
//! children sorted by (lower-case) name: `f name size hash` for a file, `d name
//! signature` for a sub-folder. A folder takes part only when every file below it
//! was hashed (a file left out of the search — protected, a placeholder, a link… —
//! or a file whose contents are unique keeps it and all its parents out), and only
//! the top-most duplicates are reported: a group whose folders all sit inside
//! folders of another group says nothing new.
//!
//! [`live_signature`] computes the same signature from the disk, for the check
//! right before a folder is removed.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use nfd_core::model::{ChildIndex, EntryId, EntryKind};
use nfd_core::safety::{Origin, SafetyGate};
use nfd_core::scanner::ScanTree;
use serde::{Deserialize, Serialize};

use crate::CopyFlags;
use crate::hash::{self, Ctx, Source};

/// One copy of a duplicated folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderCopy {
    pub path: PathBuf,
    /// Newest last write of a file inside (Unix seconds).
    pub mtime: i64,
    pub flags: CopyFlags,
    /// The SafetyGate refuses to remove this folder itself (it may still stay).
    #[serde(default)]
    pub blocked: bool,
}

/// Folders with identical contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderGroup {
    /// Continues the numbering of the file groups.
    pub id: u32,
    /// Signature of the contents, hex.
    pub hash: String,
    /// Bytes in each copy.
    pub size: u64,
    /// Files in each copy.
    pub files: u64,
    /// `size × (copies − 1)`.
    pub wasted: u64,
    pub folders: Vec<FolderCopy>,
}

/// BLAKE3 of empty contents (empty files are never read).
fn empty() -> [u8; 32] {
    *blake3::hash(b"").as_bytes()
}

enum Child {
    File { size: u64, hash: [u8; 32] },
    Dir { sig: [u8; 32] },
}

fn signature(children: &mut [(String, Child)]) -> [u8; 32] {
    children.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = blake3::Hasher::new();
    for (name, child) in children.iter() {
        match child {
            Child::File { size, hash } => {
                h.update(b"f");
                h.update(name.as_bytes());
                h.update(&[0]);
                h.update(&size.to_le_bytes());
                h.update(hash);
            }
            Child::Dir { sig } => {
                h.update(b"d");
                h.update(name.as_bytes());
                h.update(&[0]);
                h.update(sig);
            }
        }
    }
    *h.finalize().as_bytes()
}

/// What is known about one file entry of a scan: its full hash and flags.
pub(crate) type FileFacts = HashMap<(usize, u32), ([u8; 32], CopyFlags)>;

#[derive(Clone, Copy)]
struct DirFacts {
    sig: [u8; 32],
    size: u64,
    files: u64,
    newest: i64,
    flags: CopyFlags,
}

fn or_flags(a: CopyFlags, b: CopyFlags) -> CopyFlags {
    CopyFlags {
        cloud: a.cloud || b.cloud,
        git: a.git || b.git,
        app_dir: a.app_dir || b.app_dir,
        readonly: a.readonly || b.readonly,
        clone: a.clone || b.clone,
    }
}

/// Group the folders of `trees` by contents. `files` holds every file entry that
/// was hashed (tree index, entry index); ids start at `first_id`.
pub(crate) fn group(
    trees: &[&ScanTree],
    files: &FileFacts,
    gate: &SafetyGate,
    first_id: u32,
) -> Vec<FolderGroup> {
    // signature → members (tree, entry)
    let mut by_sig: HashMap<[u8; 32], Vec<(usize, EntryId, DirFacts)>> = HashMap::new();
    for (t, tree) in trees.iter().enumerate() {
        let kids = ChildIndex::build(&tree.entries);
        let mut facts: Vec<Option<DirFacts>> = vec![None; tree.entries.len()];
        for e in tree.entries.iter().rev() {
            if e.kind != EntryKind::Dir || e.removed {
                continue;
            }
            let mut children = Vec::new();
            let mut ok = true;
            let mut d = DirFacts {
                sig: [0; 32],
                size: 0,
                files: 0,
                newest: i64::MIN,
                flags: CopyFlags::default(),
            };
            for &c in kids.children(e.id) {
                let child = &tree.entries[c.index()];
                if child.removed {
                    continue;
                }
                let name = child.name.to_lowercase();
                match child.kind {
                    EntryKind::File if child.logical == 0 => {
                        children.push((
                            name,
                            Child::File {
                                size: 0,
                                hash: empty(),
                            },
                        ));
                    }
                    EntryKind::File => match files.get(&(t, c.0)) {
                        Some((h, f)) => {
                            children.push((
                                name,
                                Child::File {
                                    size: child.logical,
                                    hash: *h,
                                },
                            ));
                            d.size += child.logical;
                            d.files += 1;
                            d.newest = d.newest.max(child.mtime);
                            d.flags = or_flags(d.flags, *f);
                        }
                        None => ok = false,
                    },
                    EntryKind::Dir => match facts[c.index()] {
                        Some(sub) => {
                            children.push((name, Child::Dir { sig: sub.sig }));
                            d.size += sub.size;
                            d.files += sub.files;
                            d.newest = d.newest.max(sub.newest);
                            d.flags = or_flags(d.flags, sub.flags);
                        }
                        None => ok = false,
                    },
                    _ => ok = false,
                }
                if !ok {
                    break;
                }
            }
            if !ok {
                continue;
            }
            d.sig = signature(&mut children);
            facts[e.id.index()] = Some(d);
            if d.files > 0 {
                by_sig.entry(d.sig).or_default().push((t, e.id, d));
            }
        }
    }
    let members: HashSet<(usize, u32)> = by_sig
        .values()
        .filter(|m| m.len() > 1)
        .flatten()
        .map(|(t, id, _)| (*t, id.0))
        .collect();
    let mut out: Vec<FolderGroup> = by_sig
        .into_iter()
        .filter(|(_, m)| m.len() > 1)
        // a group whose folders all sit in duplicated folders adds nothing
        .filter(|(_, m)| {
            !m.iter().all(|(t, id, _)| {
                trees[*t].entries[id.index()]
                    .parent
                    .is_some_and(|p| members.contains(&(*t, p.0)))
            })
        })
        .filter_map(|(sig, m)| {
            let mut seen = HashSet::new();
            let mut folders: Vec<FolderCopy> = m
                .iter()
                .filter_map(|(t, id, d)| {
                    let path = trees[*t].path(*id);
                    seen.insert(crate::collect::key(&path)).then(|| FolderCopy {
                        blocked: gate
                            .check_path(&path, Origin::Duplicate { group: 0 })
                            .is_err(),
                        path,
                        mtime: d.newest,
                        flags: d.flags,
                    })
                })
                .collect();
            if folders.len() < 2 {
                return None;
            }
            folders.sort_by(|a, b| a.path.cmp(&b.path));
            let d = m[0].2;
            Some(FolderGroup {
                id: 0,
                hash: hash::hex(&sig),
                size: d.size,
                files: d.files,
                wasted: d.size * (folders.len() as u64 - 1),
                folders,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.wasted
            .cmp(&a.wasted)
            .then_with(|| a.folders[0].path.cmp(&b.folders[0].path))
    });
    for (n, g) in out.iter_mut().enumerate() {
        g.id = first_id.saturating_add(u32::try_from(n).unwrap_or(u32::MAX));
    }
    out
}

/// The signature of `dir` as it is on disk now (every file read in full), or
/// `None` when something inside is a link, a cloud placeholder or unreadable.
///
/// # Errors
/// Only cancellation ([`io::ErrorKind::Interrupted`]); anything else is `Ok(None)`.
pub fn live_signature(
    source: &dyn Source,
    dir: &Path,
    ctx: &Ctx<'_>,
) -> io::Result<Option<[u8; 32]>> {
    let Ok(items) = nfd_platform::fsinfo::read_dir(dir, false) else {
        return Ok(None);
    };
    let mut children = Vec::new();
    for item in items {
        let Ok(info) = item.info else { return Ok(None) };
        if info.link != nfd_platform::fsinfo::LinkKind::None
            || info
                .flags
                .contains(nfd_platform::fsinfo::FileFlags::OFFLINE)
        {
            return Ok(None);
        }
        let name = item.name.to_string_lossy().to_lowercase();
        let path = dir.join(&item.name);
        if info.is_dir {
            match live_signature(source, &path, ctx)? {
                Some(sig) => children.push((name, Child::Dir { sig })),
                None => return Ok(None),
            }
        } else if info.logical == 0 {
            children.push((
                name,
                Child::File {
                    size: 0,
                    hash: empty(),
                },
            ));
        } else {
            match hash::full(source, &path, info.logical, info.mtime, ctx) {
                Ok(h) => children.push((
                    name,
                    Child::File {
                        size: info.logical,
                        hash: h,
                    },
                )),
                Err(hash::Refused::Cancelled) => {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
                }
                Err(hash::Refused::Skip(_)) => return Ok(None),
            }
        }
    }
    Ok(Some(signature(&mut children)))
}
