//! Suggestions: the units of a scan grouped the way a person thinks about them
//! ("80 GB of AI agent worktrees in NANOFreeDisk\.claude", "35 GB of Rust build
//! output in 12 folders", "Windows — keep"), biggest first within each verdict.

use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::sync::Arc;

use nfd_core::RiskLevel;
use nfd_core::model::EntryId;
use nfd_core::scanner::ScanTree;
use nfd_rules::matcher::MatchReport;
use serde::Serialize;

use crate::knowledge::Knowledge;
use crate::text::Text;
use crate::{Action, Advice, ByVerdict, Reason, Verdict, key_of, rel_path, within};

/// Units shown per group.
const ITEMS: usize = 20;
/// Groups shown per verdict that cannot be acted on (Guide, Keep, Unknown)…
const INFO_GROUPS: usize = 12;
/// …and the least they must hold to be worth a line.
const INFO_MIN: u64 = 1 << 20;

/// One member of a group.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupItem {
    /// Entry of the scan (`None` outside it).
    pub id: Option<u32>,
    /// Relative to the scanned folder (absolute outside it).
    pub path: String,
    pub bytes: u64,
    pub files: u64,
    pub blocked: Option<String>,
}

/// Units that read as one suggestion.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Stable id of the group for this advice (actions refer to it).
    pub key: String,
    pub verdict: Verdict,
    pub reason: Reason,
    pub level: Option<RiskLevel>,
    pub source: Option<String>,
    pub app: Option<String>,
    pub title: Option<Arc<Text>>,
    pub impact: Arc<Text>,
    pub untouched: Option<Arc<Text>>,
    pub action: Action,
    pub blocked: Option<String>,
    pub store: Option<String>,
    pub requires_closed: Vec<String>,
    /// Bytes of the group's verdict.
    pub bytes: u64,
    /// Bytes the units hold but that stay for now (written too recently).
    pub recent: u64,
    pub files: u64,
    pub count: usize,
    /// Pre-selected (🟢, nothing blocked, not Advanced).
    pub selected: bool,
    /// The application can run its action.
    pub actionable: bool,
    /// Common folder of the units (relative to the scanned folder).
    pub location: Option<String>,
    /// Found by the rule packs outside the scanned folder.
    pub elsewhere: bool,
    pub items: Vec<GroupItem>,
}

/// Everything the Suggestions panel shows.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestions {
    /// Bytes of the whole scan by verdict.
    pub totals: ByVerdict,
    /// What unblocked actions can free.
    pub reclaim: u64,
    pub groups: Vec<Group>,
    pub enriched: bool,
}

/// Common leading components of relative paths.
fn common_prefix(paths: &[String]) -> Option<String> {
    let first = paths.first()?;
    let mut common: Vec<Component<'_>> = Path::new(first).components().collect();
    for p in &paths[1..] {
        let comps: Vec<Component<'_>> = Path::new(p).components().collect();
        let n = common
            .iter()
            .zip(&comps)
            .take_while(|(a, b)| {
                a.as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
            })
            .count();
        common.truncate(n);
    }
    let out: std::path::PathBuf = common.iter().collect();
    let s = out.to_string_lossy().into_owned();
    (!s.is_empty()).then_some(s)
}

impl Advice {
    /// Group key of unit `u`.
    pub(crate) fn group_key(&self, tree: &ScanTree, u: usize) -> String {
        let x = &self.units[u];
        let split = match x.reason {
            Reason::System
            | Reason::Programs
            | Reason::Protected
            | Reason::Personal
            | Reason::AgentHistory => Some(x.entry),
            Reason::Explained if !x.verdict.reclaimable() && x.verdict != Verdict::Keep => {
                Some(x.entry)
            }
            Reason::AgentWorktree => tree.entries[x.entry as usize].parent.map(|p| p.0),
            _ => None,
        };
        format!(
            "{}|{}|{}|{}|{:?}|{}",
            x.verdict.as_str(),
            x.reason.as_str(),
            x.source.as_deref().unwrap_or_default(),
            x.blocked.as_deref().unwrap_or_default(),
            x.action,
            split.map(|s| s.to_string()).unwrap_or_default()
        )
    }

    /// Units by group key, every unit (actions use it to find what a group covers).
    #[must_use]
    pub fn group_units(&self, tree: &ScanTree) -> BTreeMap<String, Vec<usize>> {
        let mut out: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for u in 0..self.units.len() {
            out.entry(self.group_key(tree, u)).or_default().push(u);
        }
        out
    }

    /// The suggestions of the whole scan.
    #[must_use]
    pub fn suggestions(&self, tree: &ScanTree) -> Suggestions {
        let mut groups: Vec<Group> = Vec::new();
        for (key, members) in self.group_units(tree) {
            let first = &self.units[members[0]];
            let verdict = first.verdict;
            let mut bytes = 0;
            let mut recent = 0;
            let mut files = 0;
            let mut sized: Vec<(usize, u64, u64)> = Vec::with_capacity(members.len());
            for &u in &members {
                let (t, f) = self.unit_bytes(u);
                let b = t.get(verdict);
                bytes += b;
                if verdict.reclaimable() {
                    recent += t.keep;
                }
                files += f;
                sized.push((u, b, f));
            }
            if bytes == 0 {
                continue;
            }
            sized.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let items: Vec<GroupItem> = sized
                .iter()
                .filter(|s| s.1 > 0)
                .take(ITEMS)
                .map(|&(u, b, f)| GroupItem {
                    id: Some(self.units[u].entry),
                    path: rel_path(tree, self.units[u].entry),
                    bytes: b,
                    files: f,
                    blocked: self.units[u].blocked.clone(),
                })
                .collect();
            let paths: Vec<String> = sized
                .iter()
                .filter(|s| s.1 > 0)
                .map(|&(u, _, _)| rel_path(tree, self.units[u].entry))
                .collect();
            let actionable = members.iter().all(|&u| self.units[u].actionable());
            groups.push(Group {
                key,
                verdict,
                reason: first.reason,
                level: members.iter().filter_map(|&u| self.units[u].level).max(),
                source: first.source.clone(),
                app: first.app.clone(),
                title: first.title.clone(),
                impact: first.impact.clone(),
                untouched: first.untouched.clone(),
                action: first.action,
                blocked: first.blocked.clone(),
                store: first.store.clone(),
                requires_closed: first.requires_closed.clone(),
                bytes,
                recent,
                files,
                count: paths.len(),
                selected: actionable && members.iter().all(|&u| self.units[u].selected),
                actionable,
                location: common_prefix(&paths),
                elsewhere: false,
                items,
            });
        }
        groups.sort_by(|a, b| {
            a.verdict
                .cmp(&b.verdict)
                .then(b.bytes.cmp(&a.bytes))
                .then(a.key.cmp(&b.key))
        });
        let mut shown = [0usize; 5];
        groups.retain(|g| {
            let n = &mut shown[g.verdict.code() as usize];
            if g.verdict.reclaimable() || g.bytes >= INFO_MIN {
                *n += 1;
            }
            g.verdict.reclaimable() || (*n <= INFO_GROUPS && g.bytes >= INFO_MIN)
        });
        Suggestions {
            totals: self.totals(tree, 0),
            reclaim: self.reclaim(tree, 0),
            groups,
            enriched: self.enriched,
        }
    }

    /// The units of `keys` that may be acted on: group keys of [`Self::suggestions`],
    /// `entry|<id>` (the unit starting at that entry) or `within|<id>` (every unit
    /// inside that folder).
    #[must_use]
    pub fn actionable_units(&self, tree: &ScanTree, ends: &[u32], keys: &[String]) -> Vec<usize> {
        let groups = self.group_units(tree);
        let mut out: Vec<usize> = Vec::new();
        for k in keys {
            let id = |prefix: &str| k.strip_prefix(prefix).and_then(|n| n.parse::<u32>().ok());
            if let Some(id) = id("entry|") {
                out.extend(self.units.iter().position(|u| u.entry == id));
            } else if let Some(id) = id("within|") {
                out.extend(self.units_within(ends, id));
            } else if let Some(g) = groups.get(k) {
                out.extend(g);
            }
        }
        out.retain(|&u| self.units[u].actionable());
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Path of a unit.
    #[must_use]
    pub fn unit_path(&self, tree: &ScanTree, u: usize) -> std::path::PathBuf {
        tree.path(EntryId(self.units[u].entry))
    }
}

/// What the rule packs found outside the scanned folder `root` (the Clean view's
/// last scan), one group per rule: junk elsewhere on the machine.
#[must_use]
pub fn elsewhere(report: &MatchReport, root: &Path, k: &Knowledge) -> Vec<Group> {
    let root = key_of(&root.to_string_lossy());
    let mut by_rule: BTreeMap<&str, Vec<&nfd_rules::matcher::Candidate>> = BTreeMap::new();
    for c in &report.candidates {
        if within(&key_of(&c.path.to_string_lossy()), &root) {
            continue;
        }
        by_rule.entry(c.rule_id.as_str()).or_default().push(c);
    }
    let mut out: Vec<Group> = by_rule
        .into_iter()
        .filter_map(|(rule_id, mut cands)| {
            let info = k.rule_info.get(rule_id)?;
            let level = cands.iter().map(|c| c.level).max()?;
            if level == RiskLevel::Protected {
                return None;
            }
            cands.sort_by_key(|a| std::cmp::Reverse(a.bytes));
            let bytes: u64 = cands.iter().map(|c| c.bytes).sum();
            let files: u64 = cands.iter().map(|c| c.files).sum();
            let verdict = Verdict::of_level(level);
            let store = cands
                .first()
                .and_then(|c| k.store_for(&key_of(&c.path.to_string_lossy())));
            let action = match store {
                Some((_, true)) => Action::Store,
                _ => Action::Clean,
            };
            Some(Group {
                key: format!("elsewhere|{rule_id}"),
                verdict,
                reason: Reason::Rule,
                level: Some(level),
                source: Some(rule_id.to_owned()),
                app: Some(info.rule.app.clone()),
                title: None,
                impact: info.impact.clone(),
                untouched: Some(info.untouched.clone()),
                action,
                blocked: None,
                store: store.map(|(s, _)| s.to_owned()),
                requires_closed: info.rule.requires_closed.clone(),
                bytes,
                recent: 0,
                files,
                count: cands.len(),
                selected: verdict == Verdict::Safe
                    && level != RiskLevel::Advanced
                    && action == Action::Clean,
                actionable: action == Action::Clean,
                location: None,
                elsewhere: true,
                items: cands
                    .iter()
                    .take(ITEMS)
                    .map(|c| GroupItem {
                        id: None,
                        path: c.path.to_string_lossy().into_owned(),
                        bytes: c.bytes,
                        files: c.files,
                        blocked: None,
                    })
                    .collect(),
            })
        })
        .filter(|g| g.bytes > 0)
        .collect();
    out.sort_by(|a, b| a.verdict.cmp(&b.verdict).then(b.bytes.cmp(&a.bytes)));
    out
}
