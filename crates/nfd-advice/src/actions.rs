//! Turning picked suggestions into work for the existing pipelines: rule cleans
//! (the rule's own walk, so `min_age_days`, `keep_newest`, excludes and 🔒 Keep all
//! apply), build output into the quarantine, AI leftovers through their module (git
//! for worktrees), personal picks into the Recycle Bin. Nothing here deletes.

use std::path::PathBuf;

use nfd_core::RiskLevel;
use nfd_core::model::{EntryId, EntryKind};
use nfd_core::plan::{DeleteMode, PlanItem};
use nfd_core::scanner::ScanTree;
use nfd_rules::matcher::MatchReport;

use crate::{Action, Advice, Reason, Verdict, key_of, within};

/// What a set of picked units turns into.
#[derive(Debug, Clone, Default)]
pub struct Prepared {
    /// Rules to walk again (`Matcher::scan` with only these ids)…
    pub rule_ids: Vec<String>,
    /// …keeping only their candidates inside these folders.
    pub rule_roots: Vec<PathBuf>,
    /// Hand-picked folders for the quarantine (build output).
    pub quarantine: Vec<PlanItem>,
    /// Paths for `ai_leftovers::prepare` (worktrees, leftovers, their build output).
    pub ai_paths: Vec<PathBuf>,
    /// Entries for the Recycle Bin.
    pub recycle: Vec<u32>,
    /// Units left out: (entry, why) — `caution-not-confirmed`, `expert-only`,
    /// `contains-kept`, `blocked`.
    pub skipped: Vec<(u32, &'static str)>,
}

impl Advice {
    /// Prepare the units `picked` (indices into [`Advice::units`]). 🟡 units need
    /// `caution` (the user ticked them knowingly), 🟠 Advanced ones `expert`.
    #[must_use]
    pub fn prepare(
        &self,
        tree: &ScanTree,
        ends: &[u32],
        picked: &[usize],
        caution: bool,
        expert: bool,
    ) -> Prepared {
        let mut out = Prepared::default();
        for &u in picked {
            let Some(x) = self.units.get(u) else {
                continue;
            };
            if !x.actionable() {
                out.skipped.push((x.entry, "blocked"));
                continue;
            }
            if x.verdict == Verdict::Caution && !caution {
                out.skipped.push((x.entry, "caution-not-confirmed"));
                continue;
            }
            if x.level == Some(RiskLevel::Advanced) && !expert {
                out.skipped.push((x.entry, "expert-only"));
                continue;
            }
            let path = tree.path(EntryId(x.entry));
            match x.action {
                Action::Clean => {
                    if let Some(rule) = &x.source {
                        if !out.rule_ids.contains(rule) {
                            out.rule_ids.push(rule.clone());
                        }
                    }
                    out.rule_roots.push(path);
                }
                Action::Quarantine => {
                    if self.holds_kept(ends, x.entry) {
                        out.skipped.push((x.entry, "contains-kept"));
                        continue;
                    }
                    let e = tree.entry(EntryId(x.entry));
                    let mut item = PlanItem::manual(
                        path,
                        e.kind == EntryKind::Dir,
                        e.counted().1,
                        tree.file_counts[x.entry as usize],
                        e.mtime,
                    );
                    if e.kind != EntryKind::Dir {
                        item.size = e.logical;
                    }
                    item.mode = DeleteMode::Quarantine;
                    out.quarantine.push(item);
                }
                Action::AiLeftovers => out.ai_paths.push(path),
                Action::RecycleBin => {
                    if self.holds_kept(ends, x.entry) {
                        out.skipped.push((x.entry, "contains-kept"));
                        continue;
                    }
                    out.recycle.push(x.entry);
                }
                Action::Store | Action::Duplicates | Action::Guide | Action::None => {
                    out.skipped.push((x.entry, "blocked"));
                }
            }
        }
        out
    }

    /// `true` when a kept or explained-only unit starts strictly inside `id`: the
    /// folder must not go as a whole.
    fn holds_kept(&self, ends: &[u32], id: u32) -> bool {
        let end = ends.get(id as usize).copied().unwrap_or(id + 1);
        self.units.iter().any(|u| {
            u.entry > id
                && u.entry < end
                && (!u.verdict.reclaimable() && u.reason != Reason::Personal)
        })
    }
}

/// The candidates of a rule walk (`report`, made with [`Prepared::rule_ids`]) that
/// lie inside [`Prepared::rule_roots`], as planner inputs.
#[must_use]
pub fn rule_items(report: &MatchReport, roots: &[PathBuf]) -> Vec<PlanItem> {
    let roots: Vec<String> = roots.iter().map(|r| key_of(&r.to_string_lossy())).collect();
    report
        .candidates
        .iter()
        .filter(|c| {
            let k = key_of(&c.path.to_string_lossy());
            roots.iter().any(|r| within(&k, r))
        })
        .map(|c| c.to_plan_item(report.requires_closed(&c.rule_id)))
        .collect()
}

/// The candidates of a rule walk outside `root` (junk elsewhere), for the rules
/// `rule_ids`.
#[must_use]
pub fn elsewhere_items(
    report: &MatchReport,
    root: &std::path::Path,
    rule_ids: &[String],
) -> Vec<PlanItem> {
    let root = key_of(&root.to_string_lossy());
    report
        .candidates
        .iter()
        .filter(|c| rule_ids.contains(&c.rule_id))
        .filter(|c| !within(&key_of(&c.path.to_string_lossy()), &root))
        .filter(|c| c.level != RiskLevel::Protected)
        .map(|c| c.to_plan_item(report.requires_closed(&c.rule_id)))
        .collect()
}
