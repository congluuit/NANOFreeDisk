//! Advice for every path of a scan: **can I delete this, and what happens if I do?**
//!
//! [`advise`] walks a finished scan once (top-down, the arena is in pre-order) and
//! marks *units*: the outermost entries something the application already knows
//! about claims as a whole — a rule pack match, a project's build output (C9), an AI
//! agent's worktree (C10), a protected location of the SafetyGate, a Windows special
//! folder of the Folder Encyclopedia, a personal media folder, an old installer…
//! Everything below a unit belongs to it unless something stricter claims a part of
//! it (conversation history inside a cache folder stays 🔒, `Windows\Temp` inside the
//! protected Windows folder is a rule's). A second, bottom-up pass adds the bytes of
//! every folder by [`Verdict`], so a folder such as a project's `.claude` reads
//! "80 GB can be freed safely, 2 GB must be kept".
//!
//! Nothing here touches the disk beyond what the scan already read (a few
//! existence checks for encyclopedia markers): the engine is cheap enough to run over
//! the whole tree after every scan. Facts that need more — the git state of agent
//! worktrees, the sessions behind them, duplicate contents — come from the modules
//! that own them and refine the result ([`Advice::apply_ai`],
//! [`Advice::apply_duplicates`]).
//!
//! Advice only *suggests*: every action still goes Planner → SafetyGate → Executor →
//! UndoJournal (or git / the tool's own command), which checks everything again.

mod actions;
mod classify;
mod enrich;
mod knowledge;
mod suggest;
mod text;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nfd_core::RiskLevel;
use nfd_core::model::{ChildIndex, EntryId, EntryKind};
use nfd_core::scanner::ScanTree;
use serde::Serialize;

pub use actions::{Prepared, elsewhere_items, rule_items};
pub use classify::advise;
pub use enrich::in_scan;
pub use knowledge::{Knowledge, current_platform};
pub use suggest::{Group, GroupItem, Suggestions, elsewhere};
pub use text::Text;

/// No unit (an entry nothing claims).
pub const NONE: u32 = u32::MAX;

/// What may happen to a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// "Có thể xóa an toàn": recreated or not needed; one click.
    Safe,
    /// "Thận trọng": may go, but something is lost (history, a re-download, a long
    /// rebuild) — needs an explicit tick.
    Caution,
    /// "Chỉ hướng dẫn": only the proper system tool shrinks it; the app explains how.
    Guide,
    /// "Không được xóa": system, programs, credentials, conversation history, 🔒 Keep.
    Keep,
    /// "Chưa rõ": nothing known about it — the user decides (personal files included).
    Unknown,
}

impl Verdict {
    pub const ALL: [Self; 5] = [
        Self::Safe,
        Self::Caution,
        Self::Guide,
        Self::Keep,
        Self::Unknown,
    ];

    /// Freed by an action of the application.
    #[must_use]
    pub const fn reclaimable(self) -> bool {
        matches!(self, Self::Safe | Self::Caution)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Caution => "caution",
            Self::Guide => "guide",
            Self::Keep => "keep",
            Self::Unknown => "unknown",
        }
    }

    /// Small number for compact arrays (treemap colors): 0 = safe … 4 = unknown.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Safe => 0,
            Self::Caution => 1,
            Self::Guide => 2,
            Self::Keep => 3,
            Self::Unknown => 4,
        }
    }

    const fn of_level(level: RiskLevel) -> Self {
        match level {
            RiskLevel::Safe => Self::Safe,
            RiskLevel::Caution | RiskLevel::Advanced => Self::Caution,
            RiskLevel::Protected => Self::Keep,
        }
    }
}

/// Bytes on disk by verdict.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ByVerdict {
    pub safe: u64,
    pub caution: u64,
    pub guide: u64,
    pub keep: u64,
    pub unknown: u64,
}

impl ByVerdict {
    pub const fn add(&mut self, v: Verdict, bytes: u64) {
        match v {
            Verdict::Safe => self.safe += bytes,
            Verdict::Caution => self.caution += bytes,
            Verdict::Guide => self.guide += bytes,
            Verdict::Keep => self.keep += bytes,
            Verdict::Unknown => self.unknown += bytes,
        }
    }

    #[must_use]
    pub const fn get(&self, v: Verdict) -> u64 {
        match v {
            Verdict::Safe => self.safe,
            Verdict::Caution => self.caution,
            Verdict::Guide => self.guide,
            Verdict::Keep => self.keep,
            Verdict::Unknown => self.unknown,
        }
    }

    pub const fn merge(&mut self, o: &Self) {
        self.safe += o.safe;
        self.caution += o.caution;
        self.guide += o.guide;
        self.keep += o.keep;
        self.unknown += o.unknown;
    }

    #[must_use]
    pub const fn total(&self) -> u64 {
        self.safe + self.caution + self.guide + self.keep + self.unknown
    }

    /// What the application can free: 🟢 + 🟡.
    #[must_use]
    pub const fn reclaimable(&self) -> u64 {
        self.safe + self.caution
    }

    /// The verdict holding every byte, if there is a single one.
    #[must_use]
    pub fn only(&self) -> Option<Verdict> {
        let mut found = None;
        for v in Verdict::ALL {
            if self.get(v) > 0 {
                if found.is_some() {
                    return None;
                }
                found = Some(v);
            }
        }
        found
    }
}

/// Why a unit has its verdict. Doubles as the UI message id suffix
/// (`advice-reason-<code>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// A cleanup rule of the rule packs (Clean, DevCache).
    Rule,
    /// A 🔒 Keep rule (Protected level).
    KeepRule,
    /// A project's build output (C9).
    BuildOutput,
    /// A git worktree an AI agent worked in (C10).
    AgentWorktree,
    /// Another AI agent leftover (scratchpad, browser snapshots, test output…).
    AgentLeftover,
    /// AI conversation history (PLAN §5.1).
    AgentHistory,
    /// Part of Windows.
    System,
    /// Installed programs.
    Programs,
    /// Another protected location of the SafetyGate (credentials, tool settings…).
    Protected,
    /// Photos, videos, music: the user's own.
    Personal,
    /// A Folder Encyclopedia article (`source` = its id).
    Explained,
    /// Large and untouched for over a year.
    OldLarge,
    /// An installer or disk image left in Downloads.
    OldInstaller,
    /// An extra copy of a file kept elsewhere.
    Duplicate,
}

impl Reason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::KeepRule => "keep-rule",
            Self::BuildOutput => "build-output",
            Self::AgentWorktree => "agent-worktree",
            Self::AgentLeftover => "agent-leftover",
            Self::AgentHistory => "agent-history",
            Self::System => "system",
            Self::Programs => "programs",
            Self::Protected => "protected",
            Self::Personal => "personal",
            Self::Explained => "explained",
            Self::OldLarge => "old-large",
            Self::OldInstaller => "old-installer",
            Self::Duplicate => "duplicate",
        }
    }

    /// Never overridden by a looser claim of an outer unit (enrichment keeps them).
    const fn strict(self) -> bool {
        matches!(
            self,
            Self::KeepRule | Self::AgentHistory | Self::System | Self::Programs | Self::Protected
        )
    }
}

/// The right way to free a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// Clean with the rule (its own walk), into the quarantine.
    Clean,
    /// Move into the quarantine as a hand-picked item (build output).
    Quarantine,
    /// AI leftovers: worktrees by `git worktree remove`, folders into the quarantine.
    AiLeftovers,
    /// The Windows Recycle Bin (personal files the user picks).
    RecycleBin,
    /// The package manager's own cleanup command (DevCache → Stores).
    Store,
    /// Review in the Duplicates view.
    Duplicates,
    /// Only explained: the proper system tool does it.
    Guide,
    /// Nothing to do: kept.
    None,
}

impl Action {
    /// The application can run it from a suggestion.
    #[must_use]
    pub const fn runs_here(self) -> bool {
        matches!(
            self,
            Self::Clean | Self::Quarantine | Self::AiLeftovers | Self::RecycleBin
        )
    }
}

/// One classified subtree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    /// Entry the unit starts at.
    pub entry: u32,
    pub verdict: Verdict,
    pub reason: Reason,
    /// Level of the rule / build output / leftover that claims it.
    pub level: Option<RiskLevel>,
    /// Rule id, build-output kind, article id, protected template, catalog id…
    pub source: Option<String>,
    /// The program it belongs to (rule app, AI tool).
    pub app: Option<String>,
    /// A display title when the source has one (encyclopedia article).
    pub title: Option<Arc<Text>>,
    /// "If you delete this…"
    pub impact: Arc<Text>,
    /// What stays even when it is cleaned.
    pub untouched: Option<Arc<Text>>,
    pub action: Action,
    /// Why it cannot be acted on now: `git-check`, `session-live`, `dirty`,
    /// `not-merged`, `newest`, a SafetyGate code…
    pub blocked: Option<String>,
    /// Pre-selected in the suggestions (🟢, not blocked, not Advanced).
    pub selected: bool,
    /// Official cleanup command id (`npm-cache-clean`…) when there is one.
    pub store: Option<String>,
    /// Apps to close first (rules).
    pub requires_closed: Vec<String>,
    /// Files written after this time (Unix seconds) stay for now (rule `min_age_days`).
    pub cutoff: Option<i64>,
}

impl Unit {
    /// The application may act on it (with the right mode / tick).
    #[must_use]
    pub const fn actionable(&self) -> bool {
        self.verdict.reclaimable() && self.blocked.is_none() && self.action.runs_here()
    }

    /// Verdict of a file of this unit last written at `mtime`.
    const fn verdict_at(&self, mtime: i64) -> Verdict {
        match self.cutoff {
            Some(c) if mtime > c && self.verdict.reclaimable() => Verdict::Keep,
            _ => self.verdict,
        }
    }
}

/// Advice of a whole scan. Index-aligned with the scan arena it was made from.
#[derive(Debug, Clone, Default)]
pub struct Advice {
    pub units: Vec<Unit>,
    /// Unit of every entry ([`NONE`] = unclaimed).
    unit_of: Vec<u32>,
    /// Index into `dir_totals` of every directory entry ([`NONE`] for others).
    slot: Vec<u32>,
    dir_totals: Vec<ByVerdict>,
    /// Bytes below each directory an unblocked action can free.
    dir_reclaim: Vec<u64>,
    /// Bytes of each unit, nested units left out.
    unit_totals: Vec<ByVerdict>,
    unit_files: Vec<u64>,
    /// Git facts of agent worktrees were added ([`Self::apply_ai`]).
    pub enriched: bool,
    pub computed_at: i64,
    /// Projects with agent worktrees (`<project>/.claude/worktrees`): where the AI
    /// leftovers check should look.
    pub agent_projects: Vec<PathBuf>,
}

/// What the tree list shows for one entry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeAdvice {
    /// The unit's verdict when the entry belongs to one (else the folder is mixed or
    /// unknown: see `inside`).
    pub verdict: Option<Verdict>,
    pub reason: Option<Reason>,
    /// The entry starts its unit.
    pub root: bool,
    pub inside: ByVerdict,
    /// 🟢 + 🟡 bytes that can be freed through an action that is not blocked.
    pub reclaim: u64,
}

/// A classified part inside a folder ([`EntryAdvice::parts`]).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub id: u32,
    pub path: String,
    pub verdict: Verdict,
    pub reason: Reason,
    pub source: Option<String>,
    pub app: Option<String>,
    pub title: Option<Arc<Text>>,
    pub blocked: Option<String>,
    pub bytes: u64,
}

/// Everything the details panel shows for one entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryAdvice {
    pub id: u32,
    pub node: NodeAdvice,
    /// The unit the entry belongs to.
    pub unit: Option<Unit>,
    /// Where that unit starts (relative to the scanned folder), when not here.
    pub unit_path: Option<String>,
    /// The biggest classified parts below the entry.
    pub parts: Vec<Part>,
    pub enriched: bool,
}

/// Lower-case, `/`-separated comparison key of a path (no trailing `/` except for a
/// bare root).
#[must_use]
pub fn key_of(path: &str) -> String {
    let mut s: String = path
        .chars()
        .map(|c| if c == '\\' { '/' } else { c })
        .collect::<String>()
        .to_lowercase();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// `key` joined with a child name.
fn join_key(parent: &str, name: &str) -> String {
    let mut s = String::with_capacity(parent.len() + name.len() + 1);
    s.push_str(parent);
    if !parent.ends_with('/') {
        s.push('/');
    }
    s.push_str(&name.to_lowercase());
    s
}

/// `true` when `inner` equals `outer` or lies below it (both [`key_of`] forms).
fn within(inner: &str, outer: &str) -> bool {
    inner == outer
        || (inner.len() > outer.len()
            && inner.starts_with(outer)
            && (outer.ends_with('/') || inner.as_bytes()[outer.len()] == b'/'))
}

impl Advice {
    /// Index of the unit `id` belongs to.
    #[must_use]
    pub fn unit_index(&self, id: u32) -> Option<usize> {
        match self.unit_of.get(id as usize) {
            Some(&u) if u != NONE => Some(u as usize),
            _ => None,
        }
    }

    /// The unit `id` belongs to.
    #[must_use]
    pub fn unit(&self, id: u32) -> Option<&Unit> {
        self.unit_index(id).map(|u| &self.units[u])
    }

    /// Bytes below (and including) `id` by verdict.
    #[must_use]
    pub fn totals(&self, tree: &ScanTree, id: u32) -> ByVerdict {
        let i = id as usize;
        if let Some(&s) = self.slot.get(i) {
            if s != NONE {
                return self.dir_totals[s as usize];
            }
        }
        let mut t = ByVerdict::default();
        if let Some(e) = tree.entries.get(i) {
            let (_, on_disk) = e.counted();
            if on_disk > 0 {
                let v = self
                    .unit(id)
                    .map_or(Verdict::Unknown, |u| u.verdict_at(e.mtime));
                t.add(v, on_disk);
            }
        }
        t
    }

    /// Bytes of a unit (nested units left out) by verdict, and its file count.
    #[must_use]
    pub fn unit_bytes(&self, u: usize) -> (ByVerdict, u64) {
        (
            self.unit_totals.get(u).copied().unwrap_or_default(),
            self.unit_files.get(u).copied().unwrap_or_default(),
        )
    }

    /// What can be freed below `id` through actions that are not blocked.
    #[must_use]
    pub fn reclaim(&self, tree: &ScanTree, id: u32) -> u64 {
        let i = id as usize;
        if let Some(&s) = self.slot.get(i) {
            if s != NONE {
                return self.dir_reclaim[s as usize];
            }
        }
        tree.entries.get(i).map_or(0, |e| {
            let (_, on_disk) = e.counted();
            match self.unit(id) {
                Some(u) if u.actionable() && u.verdict_at(e.mtime).reclaimable() => on_disk,
                _ => 0,
            }
        })
    }

    /// Units starting at or below `id`.
    fn units_within<'a>(&'a self, ends: &'a [u32], id: u32) -> impl Iterator<Item = usize> + 'a {
        let end = ends.get(id as usize).copied().unwrap_or(id + 1);
        self.units
            .iter()
            .enumerate()
            .filter(move |(_, u)| u.entry >= id && u.entry < end)
            .map(|(i, _)| i)
    }

    /// The tree-list view of one entry.
    #[must_use]
    pub fn node(&self, tree: &ScanTree, id: u32) -> NodeAdvice {
        let unit = self.unit(id);
        NodeAdvice {
            verdict: unit.map(|u| u.verdict),
            reason: unit.map(|u| u.reason),
            root: unit.is_some_and(|u| u.entry == id),
            inside: self.totals(tree, id),
            reclaim: self.reclaim(tree, id),
        }
    }

    /// The details-panel view of one entry; `None` for an id outside the scan.
    #[must_use]
    pub fn detail(&self, tree: &ScanTree, ends: &[u32], id: u32) -> Option<EntryAdvice> {
        if id as usize >= tree.entries.len() {
            return None;
        }
        let unit = self.unit_index(id);
        let mut parts: Vec<Part> = self
            .units_within(ends, id)
            .filter(|&u| Some(u) != unit)
            .map(|u| {
                let x = &self.units[u];
                let bytes = self.unit_totals[u].total();
                Part {
                    id: x.entry,
                    path: rel_path(tree, x.entry),
                    verdict: x.verdict,
                    reason: x.reason,
                    source: x.source.clone(),
                    app: x.app.clone(),
                    title: x.title.clone(),
                    blocked: x.blocked.clone(),
                    bytes,
                }
            })
            .filter(|p| p.bytes > 0)
            .collect();
        parts.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.id.cmp(&b.id)));
        parts.truncate(8);
        let u = unit.map(|u| self.units[u].clone());
        Some(EntryAdvice {
            id,
            node: self.node(tree, id),
            unit_path: u
                .as_ref()
                .filter(|u| u.entry != id)
                .map(|u| rel_path(tree, u.entry)),
            unit: u,
            parts,
            enriched: self.enriched,
        })
    }

    /// Verdict code per id for the treemap (`255` = unclaimed / mixed).
    #[must_use]
    pub fn codes(&self, ids: &[u32]) -> Vec<u8> {
        ids.iter()
            .map(|&id| self.unit(id).map_or(255, |u| u.verdict.code()))
            .collect()
    }

    /// Compute every total again (after units changed).
    pub(crate) fn compute_totals(&mut self, tree: &ScanTree) {
        let n = tree.entries.len();
        let mut slot = vec![NONE; n];
        let mut dirs = 0u32;
        for (i, e) in tree.entries.iter().enumerate() {
            if e.kind == EntryKind::Dir && !e.removed {
                slot[i] = dirs;
                dirs += 1;
            }
        }
        let mut dir_totals = vec![ByVerdict::default(); dirs as usize];
        let mut dir_reclaim = vec![0u64; dirs as usize];
        let mut unit_totals = vec![ByVerdict::default(); self.units.len()];
        let mut unit_files = vec![0u64; self.units.len()];
        for (i, e) in tree.entries.iter().enumerate() {
            if e.removed || !matches!(e.kind, EntryKind::File | EntryKind::Other) {
                continue;
            }
            let (_, on_disk) = e.counted();
            let u = self.unit_of[i];
            let v = if u == NONE {
                Verdict::Unknown
            } else {
                self.units[u as usize].verdict_at(e.mtime)
            };
            if u != NONE {
                unit_totals[u as usize].add(v, on_disk);
                if !e.hardlink_dup {
                    unit_files[u as usize] += 1;
                }
            }
            let actionable = u != NONE && self.units[u as usize].actionable() && v.reclaimable();
            if let Some(p) = e.parent {
                let s = slot[p.index()];
                if s != NONE {
                    dir_totals[s as usize].add(v, on_disk);
                    if actionable {
                        dir_reclaim[s as usize] += on_disk;
                    }
                }
            }
        }
        for i in (0..n).rev() {
            let e = &tree.entries[i];
            if slot[i] == NONE {
                continue;
            }
            if let Some(p) = e.parent {
                let ps = slot[p.index()];
                if ps != NONE {
                    let child = dir_totals[slot[i] as usize];
                    dir_totals[ps as usize].merge(&child);
                    dir_reclaim[ps as usize] += dir_reclaim[slot[i] as usize];
                }
            }
        }
        self.slot = slot;
        self.dir_totals = dir_totals;
        self.dir_reclaim = dir_reclaim;
        self.unit_totals = unit_totals;
        self.unit_files = unit_files;
    }
}

/// Path of `id` relative to the scanned folder (`\`-separated on Windows).
#[must_use]
pub fn rel_path(tree: &ScanTree, id: u32) -> String {
    let full = tree.path(EntryId(id));
    full.strip_prefix(&tree.root)
        .map_or_else(|_| full.clone(), Path::to_path_buf)
        .to_string_lossy()
        .into_owned()
}

/// Entry of `path` in the scan (case-insensitive), if it is there and not removed.
#[must_use]
pub fn locate(tree: &ScanTree, children: &ChildIndex, path: &Path) -> Option<u32> {
    let root = key_of(&tree.root.to_string_lossy());
    let target = key_of(&path.to_string_lossy());
    if !within(&target, &root) {
        return None;
    }
    let rest = target[root.len()..].trim_start_matches('/');
    let mut cur = EntryId(0);
    tree.entries.first().filter(|e| !e.removed)?;
    for part in rest.split('/').filter(|p| !p.is_empty()) {
        cur = *children.children(cur).iter().find(|c| {
            let e = tree.entry(**c);
            !e.removed && e.name.to_lowercase() == part
        })?;
    }
    Some(cur.0)
}
