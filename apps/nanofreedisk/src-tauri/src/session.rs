//! A finished scan held in memory and the read-only views the UI asks for.
//!
//! Everything here is plain Rust over [`ScanTree`] so it can be unit-tested without a
//! window; `commands.rs` wraps it for Tauri.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nfd_advice::{Advice, NodeAdvice};

use nfd_core::insights::{self, OldLargeOptions};
use nfd_core::model::{ChildIndex, Entry, EntryId, EntryKind, subtree_ends};
use nfd_core::plan::{DeleteMode, PlanItem};
use nfd_core::query::{self, Filter, SearchResult};
use nfd_core::safety::{self, Blocked};
use nfd_core::scanner::ScanTree;
use nfd_core::shortcut::{self, Status, Target};
use serde::Serialize;

/// Synthetic id for the "small items" remainder tile of a directory in a [`Slice`].
pub const REST_ID: u32 = u32::MAX;

/// A completed scan plus the indexes the views need.
#[derive(Debug)]
pub struct Session {
    pub tree: ScanTree,
    pub children: ChildIndex,
    pub ends: Vec<u32>,
    /// Size of the volume the scan lives on (0 when unknown), for size sanity checks.
    pub volume_total: u64,
    pub volume_free: u64,
    /// The scan this session came from (advice computed in the background checks it
    /// before storing its result).
    pub seq: u32,
    /// What can go and what must stay ([`nfd_advice`]), once computed.
    pub advice: Option<Arc<Advice>>,
}

/// One entry as the UI sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: u32,
    pub name: String,
    pub kind: EntryKind,
    pub logical: u64,
    pub on_disk: u64,
    pub files: u64,
    pub mtime: i64,
    pub attrs: Vec<&'static str>,
    pub has_children: bool,
    pub hardlink_dup: bool,
    /// Verdict and reclaimable bytes, once the advice is computed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advice: Option<NodeAdvice>,
}

/// A node with its full path (search results, top files).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Located {
    #[serde(flatten)]
    pub node: Node,
    pub path: String,
}

/// Breadcrumb element.
#[derive(Debug, Clone, Serialize)]
pub struct Crumb {
    pub id: u32,
    pub name: String,
}

/// Details of one entry: the node, its path and its ancestors (root first).
#[derive(Debug, Clone, Serialize)]
pub struct Detail {
    #[serde(flatten)]
    pub located: Located,
    pub ancestors: Vec<Crumb>,
    /// Share of the scanned root's on-disk total, 0..=1.
    pub share: f64,
}

/// Flattened subtree for the treemap and sunburst: parallel arrays, parents before
/// children, `parents[0] == -1`. Kinds: 0 file, 1 dir, 2 link, 3 "small items".
#[derive(Debug, Clone, Default, Serialize)]
pub struct Slice {
    pub ids: Vec<u32>,
    pub parents: Vec<i32>,
    pub sizes: Vec<u64>,
    pub kinds: Vec<u8>,
    pub names: Vec<String>,
    pub depths: Vec<u8>,
    /// `true` when `max_nodes` cut the slice short.
    pub truncated: bool,
    /// Verdict code per node ([`nfd_advice::Verdict::code`], 255 = none / mixed);
    /// empty before the advice is computed.
    pub verdicts: Vec<u8>,
}

/// Per-extension totals under one directory.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtRow {
    pub ext: String,
    pub files: u64,
    pub on_disk: u64,
    pub logical: u64,
}

/// Search response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRows {
    pub rows: Vec<Located>,
    pub matched: u64,
    pub matched_on_disk: u64,
}

/// Old & Large lists (A2).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OldLargeRows {
    pub large: SearchRows,
    pub stale: SearchRows,
    pub installers: SearchRows,
    pub distinct_files: u64,
    pub distinct_on_disk: u64,
}

/// An outermost empty folder (A3).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyRow {
    #[serde(flatten)]
    pub located: Located,
    /// Empty sub-folders inside it.
    pub nested: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyRows {
    pub rows: Vec<EmptyRow>,
    pub matched: u64,
    pub total_dirs: u64,
}

/// A shortcut whose target is gone (A3).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRow {
    #[serde(flatten)]
    pub located: Located,
    pub target: String,
    pub status: Status,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRows {
    pub rows: Vec<ShortcutRow>,
    /// `.lnk` files examined.
    pub checked: u64,
    /// Shortcuts pointing to network shares or shell items (not verified).
    pub unchecked: u64,
    /// `.lnk` files that could not be read or parsed.
    pub unreadable: u64,
}

/// One row of the age heatmap (A9).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgeRowDto {
    pub id: u32,
    pub name: String,
    pub kind: EntryKind,
    /// The folder's own files (row for the folder itself).
    pub direct: bool,
    pub total: u64,
    pub bytes: Vec<u64>,
    pub files: Vec<u64>,
}

/// Bytes by last-modified age below a folder (A9).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgeDto {
    /// Upper bounds of the bounded columns, in days.
    pub bucket_days: Vec<i64>,
    pub rows: Vec<AgeRowDto>,
    pub other: Vec<u64>,
    pub totals: Vec<u64>,
    pub files: Vec<u64>,
}

/// Verdict for one shortcut file, computed without holding the session.
#[derive(Debug, Clone)]
pub struct ShortcutCheck {
    pub id: u32,
    pub target: Option<Target>,
    pub status: Option<Status>,
}

/// Verdict for one item of a pending Recycle Bin operation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteItem {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    pub on_disk: u64,
    pub files: u64,
    pub allowed: bool,
    /// [`Blocked::code`] when refused.
    pub reason: Option<&'static str>,
    pub template: Option<String>,
}

/// What the confirmation dialog shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePlan {
    pub items: Vec<DeleteItem>,
    /// Counted bytes of the allowed items (nested selections counted once).
    pub on_disk: u64,
    pub files: u64,
    /// PLAN §5.2 size sanity: ask for an extra confirmation.
    pub extra_confirm: bool,
}

impl Session {
    #[must_use]
    pub fn new(tree: ScanTree, volume_total: u64, volume_free: u64) -> Self {
        let children = ChildIndex::build(&tree.entries);
        let ends = subtree_ends(&tree.entries);
        Self {
            tree,
            children,
            ends,
            volume_total,
            volume_free,
            seq: 0,
            advice: None,
        }
    }

    fn get(&self, id: u32) -> Option<&Entry> {
        self.tree.entries.get(id as usize).filter(|e| !e.removed)
    }

    fn live_children(&self, id: EntryId) -> impl Iterator<Item = &Entry> + '_ {
        self.children
            .children(id)
            .iter()
            .map(|c| self.tree.entry(*c))
            .filter(|e| !e.removed)
    }

    #[must_use]
    pub fn node(&self, e: &Entry) -> Node {
        let (logical, on_disk) = if e.kind == EntryKind::Dir || !e.hardlink_dup {
            (e.logical, e.on_disk)
        } else {
            e.counted()
        };
        Node {
            id: e.id.0,
            name: if e.parent.is_none() {
                self.tree.root.to_string_lossy().into_owned()
            } else {
                e.name.to_string()
            },
            kind: e.kind,
            logical,
            on_disk,
            files: self.tree.file_counts[e.id.index()],
            mtime: e.mtime,
            attrs: e.attrs.names(),
            has_children: e.kind == EntryKind::Dir && self.live_children(e.id).next().is_some(),
            hardlink_dup: e.hardlink_dup,
            advice: self.advice.as_ref().map(|a| a.node(&self.tree, e.id.0)),
        }
    }

    fn located(&self, e: &Entry) -> Located {
        Located {
            node: self.node(e),
            path: self.tree.path(e.id).to_string_lossy().into_owned(),
        }
    }

    /// Children of `id`, largest first (then by name).
    #[must_use]
    pub fn children_of(&self, id: u32) -> Vec<Node> {
        if self.get(id).is_none() {
            return Vec::new();
        }
        let mut kids: Vec<&Entry> = self.live_children(EntryId(id)).collect();
        kids.sort_by(|a, b| {
            b.counted()
                .1
                .cmp(&a.counted().1)
                .then_with(|| a.name.cmp(&b.name))
        });
        kids.into_iter().map(|e| self.node(e)).collect()
    }

    #[must_use]
    pub fn detail(&self, id: u32) -> Option<Detail> {
        let e = self.get(id)?;
        let mut ancestors = Vec::new();
        let mut cur = Some(e.id);
        while let Some(c) = cur {
            let a = self.tree.entry(c);
            ancestors.push(Crumb {
                id: c.0,
                name: if a.parent.is_none() {
                    self.tree.root.to_string_lossy().into_owned()
                } else {
                    a.name.to_string()
                },
            });
            cur = a.parent;
        }
        ancestors.reverse();
        let root = self.tree.entries[0].on_disk;
        Some(Detail {
            located: self.located(e),
            ancestors,
            share: if root == 0 {
                0.0
            } else {
                e.counted().1 as f64 / root as f64
            },
        })
    }

    /// Breadth-first slice below `id` for the treemap/sunburst: children smaller than
    /// `min_bytes` are folded into one "small items" tile per directory; at most
    /// `max_depth` levels and `max_nodes` nodes.
    #[must_use]
    pub fn slice(&self, id: u32, max_depth: u8, min_bytes: u64, max_nodes: usize) -> Slice {
        let mut s = Slice::default();
        let Some(root) = self.get(id) else {
            return s;
        };
        let push = |s: &mut Slice, id: u32, parent: i32, size: u64, kind: u8, name: &str, d: u8| {
            s.ids.push(id);
            s.parents.push(parent);
            s.sizes.push(size);
            s.kinds.push(kind);
            s.names.push(name.to_owned());
            s.depths.push(d);
        };
        push(
            &mut s,
            root.id.0,
            -1,
            root.counted().1,
            kind_code(root.kind),
            &root.name,
            0,
        );
        let mut queue = VecDeque::from([(root.id, 0usize, 0u8)]);
        while let Some((dir, index, depth)) = queue.pop_front() {
            if depth >= max_depth || self.tree.entry(dir).kind != EntryKind::Dir {
                continue;
            }
            let mut kids: Vec<&Entry> = self.live_children(dir).collect();
            kids.sort_by_key(|k| std::cmp::Reverse(k.counted().1));
            let mut rest = 0u64;
            for k in kids {
                let size = k.counted().1;
                if size == 0 {
                    continue;
                }
                if size < min_bytes || s.ids.len() >= max_nodes {
                    s.truncated |= size >= min_bytes;
                    rest += size;
                    continue;
                }
                let child_index = s.ids.len();
                push(
                    &mut s,
                    k.id.0,
                    index as i32,
                    size,
                    kind_code(k.kind),
                    &k.name,
                    depth + 1,
                );
                if k.kind == EntryKind::Dir {
                    queue.push_back((k.id, child_index, depth + 1));
                }
            }
            if rest > 0 {
                push(&mut s, REST_ID, index as i32, rest, 3, "", depth + 1);
            }
        }
        if let Some(a) = &self.advice {
            s.verdicts = a.codes(&s.ids);
        }
        s
    }

    /// Largest files below `id`.
    #[must_use]
    pub fn top_files(&self, id: u32, limit: usize) -> Vec<Located> {
        let filter = Filter {
            kind: query::KindFilter::Files,
            ..Filter::default()
        };
        self.search(id, &filter, limit).rows
    }

    #[must_use]
    pub fn search(&self, id: u32, filter: &Filter, limit: usize) -> SearchRows {
        if self.get(id).is_none() {
            return SearchRows {
                rows: Vec::new(),
                matched: 0,
                matched_on_disk: 0,
            };
        }
        let r = query::search(&self.tree, &self.ends, EntryId(id), filter, limit);
        self.rows(&r)
    }

    fn rows(&self, r: &SearchResult) -> SearchRows {
        SearchRows {
            rows: r
                .ids
                .iter()
                .map(|&i| self.located(self.tree.entry(i)))
                .collect(),
            matched: r.matched,
            matched_on_disk: r.matched_on_disk,
        }
    }

    /// Large, old and leftover-installer files below `id` (A2).
    #[must_use]
    pub fn old_and_large(&self, id: u32, options: &OldLargeOptions) -> Option<OldLargeRows> {
        self.get(id)?;
        let r = insights::old_and_large(&self.tree, &self.ends, EntryId(id), options);
        Some(OldLargeRows {
            large: self.rows(&r.large),
            stale: self.rows(&r.stale),
            installers: self.rows(&r.installers),
            distinct_files: r.distinct_files,
            distinct_on_disk: r.distinct_on_disk,
        })
    }

    /// Outermost empty folders below `id` (A3). Folders the Recycle Bin action
    /// would refuse (Windows, Program Files, the user folders themselves…) are not
    /// listed; empty folders inside them are.
    #[must_use]
    pub fn empty_folders(
        &self,
        id: u32,
        limit: usize,
        resolve: &impl Fn(&str) -> Option<String>,
    ) -> Option<EmptyRows> {
        self.get(id)?;
        let keep_out =
            |e: EntryId| safety::check_manual_delete(&self.tree.path(e), resolve).is_err();
        let r = insights::empty_folders(&self.tree, &self.ends, EntryId(id), limit, &keep_out);
        Some(EmptyRows {
            rows: r
                .folders
                .iter()
                .map(|f| EmptyRow {
                    located: self.located(self.tree.entry(f.id)),
                    nested: f.nested,
                })
                .collect(),
            matched: r.matched,
            total_dirs: r.total_dirs,
        })
    }

    /// Age heatmap of the folder `id` (A9) at `now`, `limit` biggest children.
    #[must_use]
    pub fn age(&self, id: u32, now: i64, limit: usize) -> Option<AgeDto> {
        self.get(id)?;
        let m = insights::age_matrix(&self.tree, &self.ends, EntryId(id), now, limit);
        Some(AgeDto {
            bucket_days: insights::AGE_BUCKET_DAYS.to_vec(),
            rows: m
                .rows
                .into_iter()
                .map(|r| {
                    let e = self.tree.entry(r.id);
                    AgeRowDto {
                        id: r.id.0,
                        name: self.node(e).name,
                        kind: e.kind,
                        direct: r.id.0 == id,
                        total: r.total,
                        bytes: r.bytes,
                        files: r.files,
                    }
                })
                .collect(),
            other: m.other,
            totals: m.totals,
            files: m.files,
        })
    }

    /// Every live junction and symbolic link below `id` with its path, for
    /// [`check_links`].
    #[must_use]
    pub fn link_entries(&self, id: u32) -> Vec<(u32, PathBuf)> {
        let Some(e) = self.get(id) else {
            return Vec::new();
        };
        let range = e.id.index() + 1..self.ends[e.id.index()] as usize;
        self.tree.entries[range]
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::Junction | EntryKind::Symlink) && !e.removed)
            .map(|e| (e.id.0, self.tree.path(e.id)))
            .collect()
    }

    /// Every live `.lnk` file below `id` with its path, for [`check_shortcuts`].
    #[must_use]
    pub fn shortcut_files(&self, id: u32) -> Vec<(u32, PathBuf)> {
        let Some(e) = self.get(id) else {
            return Vec::new();
        };
        let range = e.id.index() + 1..self.ends[e.id.index()] as usize;
        self.tree.entries[range]
            .iter()
            .filter(|e| {
                e.kind == EntryKind::File && !e.removed && query::extension(&e.name) == "lnk"
            })
            .map(|e| (e.id.0, self.tree.path(e.id)))
            .collect()
    }

    /// Turn [`check_shortcuts`] verdicts into rows: broken ones first (missing
    /// target, then missing drive), at most `limit`.
    #[must_use]
    pub fn shortcut_rows(&self, checks: &[ShortcutCheck], limit: usize) -> ShortcutRows {
        let mut out = ShortcutRows {
            checked: checks.len() as u64,
            ..ShortcutRows::default()
        };
        let mut broken: Vec<&ShortcutCheck> = Vec::new();
        for c in checks {
            match (&c.target, c.status) {
                (None, _) | (_, None) => out.unreadable += 1,
                (_, Some(Status::Unchecked)) => out.unchecked += 1,
                (_, Some(Status::Missing | Status::DriveMissing)) => broken.push(c),
                (_, Some(Status::Ok)) => {}
            }
        }
        broken.sort_by_key(|c| (c.status != Some(Status::Missing), c.id));
        out.rows = broken
            .into_iter()
            .filter_map(|c| {
                let e = self.get(c.id)?;
                let target = match &c.target {
                    Some(Target::Local(p) | Target::Network(p)) => p.clone(),
                    _ => String::new(),
                };
                Some(ShortcutRow {
                    located: self.located(e),
                    target,
                    status: c.status?,
                })
            })
            .take(limit)
            .collect();
        out
    }

    /// Totals per extension below `id`, largest first.
    #[must_use]
    pub fn extensions(&self, id: u32, limit: usize) -> Vec<ExtRow> {
        let Some(e) = self.get(id) else {
            return Vec::new();
        };
        let start = e.id.index();
        let end = self.ends[start] as usize;
        let mut map: HashMap<String, ExtRow> = HashMap::new();
        for e in &self.tree.entries[start..end] {
            if e.kind != EntryKind::File || e.hardlink_dup || e.removed {
                continue;
            }
            let ext = query::extension(&e.name);
            let row = map.entry(ext.clone()).or_insert_with(|| ExtRow {
                ext,
                files: 0,
                on_disk: 0,
                logical: 0,
            });
            row.files += 1;
            row.on_disk += e.on_disk;
            row.logical += e.logical;
        }
        let mut rows: Vec<ExtRow> = map.into_values().collect();
        rows.sort_by(|a, b| b.on_disk.cmp(&a.on_disk).then_with(|| a.ext.cmp(&b.ext)));
        rows.truncate(limit);
        rows
    }

    /// Live entry at `rel` (a path relative to the scanned folder, `\` or `/`
    /// separated, case-insensitive); `Some(0)` for an empty path.
    #[must_use]
    pub fn find(&self, rel: &str) -> Option<u32> {
        let mut cur = EntryId(0);
        self.get(0)?;
        for part in rel.split(['\\', '/']).filter(|p| !p.is_empty()) {
            let part = part.to_lowercase();
            cur = self
                .live_children(cur)
                .find(|e| e.name.to_lowercase() == part)?
                .id;
        }
        Some(cur.0)
    }

    /// Path of a live entry.
    #[must_use]
    pub fn path(&self, id: u32) -> Option<PathBuf> {
        self.get(id).map(|e| self.tree.path(e.id))
    }

    /// Check every selected id for the Recycle Bin. Items nested inside another
    /// selected item are dropped (the parent covers them).
    pub fn plan_delete(
        &self,
        ids: &[u32],
        resolve: &impl Fn(&str) -> Option<String>,
    ) -> DeletePlan {
        let mut ids: Vec<u32> = ids
            .iter()
            .copied()
            // the scanned folder itself is never offered for deletion
            .filter(|&i| i != 0 && self.get(i).is_some())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        // pre-order: an ancestor sorts before its descendants
        let mut kept: Vec<u32> = Vec::new();
        for id in ids {
            let nested = kept.iter().any(|&k| id > k && id < self.ends[k as usize]);
            if !nested {
                kept.push(id);
            }
        }
        let items: Vec<DeleteItem> = kept
            .into_iter()
            .map(|id| self.check_item(id, resolve))
            .collect();
        let (on_disk, files) = items
            .iter()
            .filter(|i| i.allowed)
            .fold((0, 0), |(b, f), i| (b + i.on_disk, f + i.files));
        DeletePlan {
            extra_confirm: safety::needs_extra_confirmation(on_disk, self.volume_total),
            items,
            on_disk,
            files,
        }
    }

    fn check_item(&self, id: u32, resolve: &impl Fn(&str) -> Option<String>) -> DeleteItem {
        let e = self.tree.entry(EntryId(id));
        let path = self.tree.path(e.id);
        let verdict = self.verdict(e, &path, resolve);
        DeleteItem {
            id,
            name: e.name.to_string(),
            path: path.to_string_lossy().into_owned(),
            kind: e.kind,
            on_disk: e.counted().1,
            files: self.tree.file_counts[e.id.index()],
            allowed: verdict.is_ok(),
            reason: verdict.as_ref().err().map(Blocked::code),
            template: verdict
                .as_ref()
                .err()
                .and_then(|b| b.template().map(str::to_owned)),
        }
    }

    fn verdict(
        &self,
        e: &Entry,
        path: &Path,
        resolve: &impl Fn(&str) -> Option<String>,
    ) -> Result<(), Blocked> {
        match e.kind {
            EntryKind::Symlink | EntryKind::Junction | EntryKind::MountPoint => {
                return Err(Blocked::Link);
            }
            EntryKind::Dir => {
                let range = e.id.index() + 1..self.ends[e.id.index()] as usize;
                if self.tree.entries[range]
                    .iter()
                    .any(|c| c.kind == EntryKind::MountPoint && !c.removed)
                {
                    return Err(Blocked::ContainsMountPoint);
                }
            }
            EntryKind::File | EntryKind::Other => {}
        }
        safety::check_manual_delete(path, resolve)
    }

    /// Planner input for an entry picked by hand, to be deleted with `mode`. The
    /// SafetyGate checks it again on disk (links, final path, changes since the scan)
    /// right before the executor acts.
    #[must_use]
    pub fn plan_item(&self, id: u32, mode: DeleteMode) -> Option<PlanItem> {
        let e = self.get(id)?;
        let is_dir = e.kind == EntryKind::Dir;
        let mut item = PlanItem::manual(
            self.tree.path(e.id),
            is_dir,
            e.counted().1,
            self.tree.file_counts[e.id.index()],
            e.mtime,
        );
        item.size = if is_dir { 0 } else { e.logical };
        item.mode = mode;
        Some(item)
    }

    /// Forget a deleted entry (sizes leave every ancestor).
    pub fn mark_removed(&mut self, id: u32) {
        self.tree.remove(EntryId(id), &self.ends);
    }
}

/// Read and resolve shortcut files (no session lock needed). `read` returns the
/// file's bytes, `env` expands `%VAR%`, `probe` says whether a local path exists or
/// `None` when it must not be probed (mapped network drives, optical drives).
pub fn check_shortcuts(
    files: &[(u32, PathBuf)],
    read: &impl Fn(&Path) -> Option<Vec<u8>>,
    probe: &impl Fn(&Path) -> Option<bool>,
    env: &impl Fn(&str) -> Option<String>,
) -> Vec<ShortcutCheck> {
    files
        .iter()
        .map(|(id, path)| {
            let dir = path.parent().unwrap_or(path);
            let target = read(path).and_then(|bytes| shortcut::parse(&bytes, dir, env).ok());
            let status = target.as_ref().map(|t| match t {
                Target::Local(p) if probe(Path::new(p)).is_none() => Status::Unchecked,
                _ => shortcut::status(t, &|p| probe(p).unwrap_or(true)),
            });
            ShortcutCheck {
                id: *id,
                target,
                status,
            }
        })
        .collect()
}

/// Resolve junctions / symbolic links (A3 "orphan junctions"): `read_link` returns
/// the stored target, `probe` as in [`check_shortcuts`]. Relative symlink targets are
/// resolved against the link's folder.
pub fn check_links(
    links: &[(u32, PathBuf)],
    read_link: &impl Fn(&Path) -> Option<PathBuf>,
    probe: &impl Fn(&Path) -> Option<bool>,
) -> Vec<ShortcutCheck> {
    links
        .iter()
        .map(|(id, path)| {
            let target = read_link(path).map(|t| {
                let t = if t.is_relative() {
                    path.parent().unwrap_or(path).join(t)
                } else {
                    t
                };
                let s = strip_verbatim(&t).to_string_lossy().replace('/', "\\");
                let s = s.strip_prefix(r"\??\").map_or(s.clone(), str::to_owned);
                if s.starts_with(r"\\") {
                    Target::Network(s)
                } else if s.as_bytes().get(1) == Some(&b':') {
                    Target::Local(s)
                } else {
                    Target::Unknown
                }
            });
            let status = target.as_ref().map(|t| match t {
                Target::Local(p) if probe(Path::new(p)).is_none() => Status::Unchecked,
                _ => shortcut::status(t, &|p| probe(p).unwrap_or(true)),
            });
            ShortcutCheck {
                id: *id,
                target,
                status,
            }
        })
        .collect()
}

fn kind_code(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::File | EntryKind::Other => 0,
        EntryKind::Dir => 1,
        EntryKind::Symlink | EntryKind::Junction | EntryKind::MountPoint => 2,
    }
}

/// `\\?\C:\x` → `C:\x`, `\\?\UNC\srv\share` → `\\srv\share`.
fn strip_verbatim(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use nfd_core::scanner::{ScanOptions, scan};
    use nfd_platform::Platform;

    use super::*;

    fn session() -> (tempfile::TempDir, Session) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("a/b")).expect("mkdir");
        fs::write(root.join("a/b/big.bin"), vec![1u8; 300_000]).expect("write");
        fs::write(root.join("a/small.txt"), b"hi").expect("write");
        fs::write(root.join("movie.mkv"), vec![2u8; 200_000]).expect("write");
        let tree = scan(root, &ScanOptions::default(), None).expect("scan");
        (dir, Session::new(tree, 0, 0))
    }

    fn id_of(s: &Session, name: &str) -> u32 {
        s.tree
            .entries
            .iter()
            .find(|e| &*e.name == name)
            .map(|e| e.id.0)
            .expect(name)
    }

    #[test]
    fn children_are_sorted_by_size() {
        let (_d, s) = session();
        let kids = s.children_of(0);
        let names: Vec<&str> = kids.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, ["a", "movie.mkv"]);
        assert!(kids[0].has_children);
        assert_eq!(kids[0].files, 2);
    }

    #[test]
    fn slice_folds_small_items() {
        let (_d, s) = session();
        let sl = s.slice(0, 8, 10_000, 1000);
        assert_eq!(sl.parents[0], -1);
        assert!(sl.names.iter().any(|n| n == "big.bin"));
        assert!(!sl.names.iter().any(|n| n == "small.txt"));
        assert!(sl.ids.contains(&REST_ID));
        // every parent precedes its child
        for (i, &p) in sl.parents.iter().enumerate().skip(1) {
            assert!((p as usize) < i);
        }
        let capped = s.slice(0, 8, 0, 2);
        assert!(capped.truncated);
        let shallow = s.slice(0, 1, 0, 1000);
        assert!(shallow.depths.iter().all(|&d| d <= 1));
    }

    #[test]
    fn detail_has_breadcrumbs() {
        let (_d, s) = session();
        let d = s.detail(id_of(&s, "big.bin")).expect("detail");
        let crumbs: Vec<&str> = d.ancestors.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(&crumbs[1..], ["a", "b", "big.bin"]);
        assert!(d.share > 0.5 && d.share < 1.0);
    }

    #[cfg(windows)]
    #[test]
    fn orphan_junctions_are_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("target-ok")).expect("mkdir");
        fs::create_dir_all(root.join("target-gone")).expect("mkdir");
        for (link, target) in [("ok-link", "target-ok"), ("orphan", "target-gone")] {
            let status = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(root.join(link))
                .arg(root.join(target))
                .output()
                .expect("mklink");
            assert!(status.status.success(), "{status:?}");
        }
        fs::remove_dir(root.join("target-gone")).expect("rmdir");
        let tree = scan(root, &ScanOptions::default(), None).expect("scan");
        let s = Session::new(tree, 0, 0);
        let links = s.link_entries(0);
        assert_eq!(links.len(), 2);
        let checks = check_links(&links, &|p| fs::read_link(p).ok(), &|p| Some(p.exists()));
        let rows = s.shortcut_rows(&checks, 10);
        let names: Vec<&str> = rows
            .rows
            .iter()
            .map(|r| r.located.node.name.as_str())
            .collect();
        assert_eq!(names, ["orphan"]);
        assert!(
            rows.rows[0].target.ends_with("target-gone"),
            "{}",
            rows.rows[0].target
        );
        assert!(!rows.rows[0].target.starts_with(r"\\?\"));
        assert_eq!(rows.rows[0].status, Status::Missing);

        let age = s.age(0, i64::MAX / 2, 10).expect("age");
        assert_eq!(age.bucket_days.len() + 2, age.totals.len());
    }

    #[test]
    fn find_relative_paths() {
        let (_d, s) = session();
        assert_eq!(s.find(""), Some(0));
        let big = id_of(&s, "big.bin");
        assert_eq!(s.find("a/b/big.bin"), Some(big));
        assert_eq!(s.find("A\\B\\BIG.BIN"), Some(big));
        assert_eq!(s.find("a/nope"), None);
    }

    #[test]
    fn extensions_and_top_files() {
        let (_d, s) = session();
        let ext = s.extensions(0, 10);
        assert_eq!(ext[0].ext, "bin");
        let top = s.top_files(0, 2);
        assert_eq!(top[0].node.name, "big.bin");
        assert_eq!(top.len(), 2);
    }

    /// Minimal `.lnk` whose LinkInfo points at `target` (ASCII).
    fn lnk(target: &str) -> Vec<u8> {
        let mut d = vec![0u8; 0x4C];
        d[0] = 0x4C;
        d[4..20].copy_from_slice(&[
            0x01, 0x14, 0x02, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46,
        ]);
        d[0x14] = 2; // HasLinkInfo
        let base = 0x1Cu32;
        let suffix = base + target.len() as u32 + 1;
        let total = suffix + 1;
        for x in [total, 0x1C, 1, 0, base, 0, suffix] {
            d.extend(x.to_le_bytes());
        }
        d.extend(target.as_bytes());
        d.extend([0, 0, 0, 0, 0, 0]);
        d
    }

    #[test]
    fn old_large_empty_folders_and_shortcuts() {
        let (dir, _) = session();
        let root = dir.path();
        let year_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(400 * 86_400);
        fs::create_dir_all(root.join("Downloads")).expect("mkdir");
        let old = fs::File::create(root.join("Downloads/setup.msi")).expect("create");
        old.set_modified(year_ago).expect("mtime");
        drop(old);
        fs::create_dir_all(root.join("empty/inner")).expect("mkdir");
        let live = root.join("movie.mkv");
        fs::write(root.join("ok.lnk"), lnk(&live.to_string_lossy())).expect("write");
        let gone = root.join("gone.exe");
        fs::write(root.join("broken.lnk"), lnk(&gone.to_string_lossy())).expect("write");
        fs::write(root.join("junk.lnk"), b"not a link").expect("write");
        let tree = scan(root, &ScanOptions::default(), None).expect("scan");
        let s = Session::new(tree, 0, 0);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let ol = s
            .old_and_large(
                0,
                &OldLargeOptions {
                    large_min: 250_000,
                    ..OldLargeOptions::at(now)
                },
            )
            .expect("old & large");
        assert_eq!(ol.large.rows[0].node.name, "big.bin");
        assert_eq!(ol.installers.rows.len(), 1);
        assert_eq!(ol.installers.rows[0].node.name, "setup.msi");
        assert!(
            ol.stale.rows.is_empty(),
            "0-byte file is below the stale minimum"
        );
        assert_eq!(ol.distinct_files, 2);

        let platform = nfd_platform::Native;
        let empty = s.empty_folders(0, 10, &platform.resolver()).expect("empty");
        let names: Vec<&str> = empty
            .rows
            .iter()
            .map(|r| r.located.node.name.as_str())
            .collect();
        assert_eq!(names, ["empty"]);
        assert_eq!(empty.rows[0].nested, 1);
        assert_eq!(empty.total_dirs, 2);

        let files = s.shortcut_files(0);
        assert_eq!(files.len(), 3);
        let checks = check_shortcuts(&files, &|p| fs::read(p).ok(), &|p| Some(p.exists()), &|k| {
            std::env::var(k).ok()
        });
        let rows = s.shortcut_rows(&checks, 10);
        assert_eq!(rows.checked, 3);
        assert_eq!(rows.unreadable, 1);
        // `.lnk` shortcuts point at Windows paths: only there is a target "missing"
        if cfg!(windows) {
            assert_eq!(rows.rows.len(), 1, "{rows:?}");
            assert_eq!(rows.rows[0].located.node.name, "broken.lnk");
            assert_eq!(rows.rows[0].status, Status::Missing);
            assert!(rows.rows[0].target.ends_with("gone.exe"));
        }
    }

    #[test]
    fn delete_plan_and_recycle_flow() {
        let (_d, mut s) = session();
        let platform = nfd_platform::Native;
        let resolve = platform.resolver();
        let a = id_of(&s, "a");
        let big = id_of(&s, "big.bin");
        // nested selections collapse into their parent; the root is never offered
        let plan = s.plan_delete(&[big, a, a, 0], &resolve);
        let ids: Vec<u32> = plan.items.iter().map(|i| i.id).collect();
        assert_eq!(ids, [a]);
        assert!(plan.items[0].allowed, "{:?}", plan.items[0]);
        assert_eq!(plan.files, 2);
        assert!(!plan.extra_confirm);

        let item = s
            .plan_item(a, DeleteMode::RecycleBin)
            .expect("planner input");
        assert!(item.path.ends_with("a") && item.is_dir);
        let gate = safety::SafetyGate::new(&resolve);
        gate.check_live(
            &item.path,
            item.origin,
            Some(item.expected()),
            &safety::LiveProbe,
        )
        .expect("still deletable");
        let file = s.plan_item(big, DeleteMode::RecycleBin).expect("file");
        gate.check_live(
            &file.path,
            file.origin,
            Some(file.expected()),
            &safety::LiveProbe,
        )
        .expect("the scan's size and date match the disk");
        let before = s.tree.entries[0].on_disk;
        s.mark_removed(a);
        assert!(s.tree.entries[0].on_disk < before);
        assert!(s.children_of(a).is_empty());
        assert!(s.detail(a).is_none());
        assert_eq!(s.children_of(0).len(), 1);
    }
}
