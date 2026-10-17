//! Facts from the modules that look further than the scan: the AI leftovers check
//! (git state of agent worktrees, the sessions behind them) and the duplicate search.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nfd_core::RiskLevel;
use nfd_core::model::ChildIndex;
use nfd_core::scanner::ScanTree;
use nfd_devtools::ai_leftovers::catalog::Kind;
use nfd_devtools::ai_leftovers::{self, Item, Report};

use crate::classify::{Class, class_of};
use crate::text::{self, Text};
use crate::{Action, Advice, NONE, Reason, Unit, Verdict, locate};

impl Advice {
    /// Give the unit `u` to the subtree of `id`, except the stricter units that start
    /// strictly inside it (conversation history, protected locations, 🔒 Keep).
    fn claim(&mut self, ends: &[u32], id: u32, u: u32) {
        let end = ends.get(id as usize).copied().unwrap_or(id + 1) as usize;
        for j in id as usize..end.min(self.unit_of.len()) {
            let cur = self.unit_of[j];
            if cur != NONE {
                let c = &self.units[cur as usize];
                let strict = c.reason.strict()
                    || (c.reason == Reason::Explained && !c.verdict.reclaimable());
                if strict && c.entry > id {
                    continue;
                }
            }
            self.unit_of[j] = u;
        }
    }

    fn push(&mut self, unit: Unit) -> u32 {
        self.units.push(unit);
        u32::try_from(self.units.len() - 1).unwrap_or(NONE)
    }

    /// Refine agent worktrees and leftovers with an AI leftovers `report`
    /// (`nfd_devtools::ai_leftovers::scan`): git state, open / live sessions, what
    /// git ignores. Items outside the scan are ignored.
    pub fn apply_ai(
        &mut self,
        tree: &ScanTree,
        children: &ChildIndex,
        ends: &[u32],
        report: &Report,
    ) {
        for item in &report.items {
            let Some(id) = locate(tree, children, &item.path) else {
                continue;
            };
            let u = self.push(ai_unit(id, item));
            self.claim(ends, id, u);
            for a in item.worktree.iter().flat_map(|w| &w.artifacts) {
                let Some(aid) = locate(tree, children, &a.path) else {
                    continue;
                };
                let verdict = if a.blocked.is_some() {
                    Verdict::Keep
                } else {
                    crate::Verdict::of_level(a.level)
                };
                let unit = Unit {
                    entry: aid,
                    verdict,
                    reason: Reason::BuildOutput,
                    level: Some(a.level),
                    source: Some(a.kind.to_owned()),
                    app: None,
                    title: None,
                    impact: text::build_output(a.kind),
                    untouched: Some(text::build_output_untouched()),
                    action: Action::AiLeftovers,
                    blocked: a.blocked.clone(),
                    selected: a.selected && a.blocked.is_none(),
                    store: None,
                    requires_closed: Vec::new(),
                    cutoff: None,
                    hard: a
                        .blocked
                        .as_deref()
                        .is_some_and(|b| !ai_leftovers::soft_block(b)),
                };
                let au = self.push(unit);
                self.claim(ends, aid, au);
            }
        }
        // worktrees the check did not report on stay blocked, now for good
        for u in &mut self.units {
            if u.blocked.as_deref() == Some("git-check") {
                u.blocked = Some("unverified".to_owned());
            }
        }
        self.enriched = true;
        self.compute_totals(tree);
    }

    /// Mark extra copies found by a duplicate search: `(copy, kept copy)` pairs. Only
    /// files nothing else claims (or personal media) are marked.
    pub fn apply_duplicates(
        &mut self,
        tree: &ScanTree,
        children: &ChildIndex,
        copies: &[(PathBuf, PathBuf)],
    ) {
        let mut changed = false;
        for (copy, keeper) in copies {
            let Some(id) = locate(tree, children, copy) else {
                continue;
            };
            let class = class_of(self.unit(id));
            if !matches!(class, Class::Open | Class::Personal)
                || self.unit(id).is_some_and(|u| u.entry == id)
            {
                continue;
            }
            let u = self.push(Unit {
                entry: id,
                verdict: Verdict::Caution,
                reason: Reason::Duplicate,
                level: Some(RiskLevel::Caution),
                source: None,
                app: None,
                title: None,
                impact: text::duplicate(&keeper.to_string_lossy()),
                untouched: None,
                action: Action::Duplicates,
                blocked: None,
                selected: false,
                store: None,
                requires_closed: Vec::new(),
                cutoff: None,
                hard: false,
            });
            self.unit_of[id as usize] = u;
            changed = true;
        }
        if changed {
            self.compute_totals(tree);
        }
    }

    /// Paths of the units of reason `reason` (e.g. agent worktrees to check).
    #[must_use]
    pub fn paths_of(&self, tree: &ScanTree, reason: Reason) -> Vec<PathBuf> {
        self.units
            .iter()
            .filter(|u| u.reason == reason)
            .map(|u| tree.path(nfd_core::model::EntryId(u.entry)))
            .collect()
    }
}

/// The unit of one AI leftovers item.
fn ai_unit(id: u32, item: &Item) -> Unit {
    // a worktree git may not remove (dirty, not merged…) is kept as a whole
    let blocked = item.blocked.clone().or_else(|| {
        item.worktree
            .as_ref()
            .filter(|w| w.registered && !w.removable)
            .map(|_| "not-removable".to_owned())
    });
    let verdict = if blocked.is_some() {
        Verdict::Keep
    } else {
        Verdict::of_level(item.level)
    };
    let worktree = item.kind == Kind::Worktree;
    Unit {
        entry: id,
        verdict,
        reason: if worktree {
            Reason::AgentWorktree
        } else {
            Reason::AgentLeftover
        },
        level: Some(item.level),
        source: Some(item.entry.to_owned()),
        app: Some(item.tool.to_owned()),
        title: None,
        impact: Arc::new(Text::new(item.impact, item.impact_vi)),
        untouched: Some(if worktree {
            text::worktree_untouched()
        } else {
            text::leftover_untouched()
        }),
        action: Action::AiLeftovers,
        selected: item.selected && blocked.is_none(),
        blocked,
        store: None,
        requires_closed: Vec::new(),
        cutoff: None,
        hard: item
            .blocked
            .as_deref()
            .is_some_and(|b| !ai_leftovers::soft_block(b)),
    }
}

/// `true` when `path` lies inside the scanned folder.
#[must_use]
pub fn in_scan(tree: &ScanTree, path: &Path) -> bool {
    let root = crate::key_of(&tree.root.to_string_lossy());
    crate::within(&crate::key_of(&path.to_string_lossy()), &root)
}
