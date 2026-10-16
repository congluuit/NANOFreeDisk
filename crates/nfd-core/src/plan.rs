//! The Planner (PHASE3_CLEAN §2.3): turns rule matches or hand-picked items into a
//! [`CleanPlan`] — nested items merged, totals computed, risk levels turned into
//! default selections — which then goes through the
//! [`SafetyGate`](crate::safety::SafetyGate) and the executor.
//!
//! Pure: no filesystem access, so every property (no two nested items, Protected
//! never selectable, …) is tested directly.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::RiskLevel;
use crate::safety::gate::{Expected, Origin};
use crate::safety::{comparable, is_within};

/// How an item leaves the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeleteMode {
    /// Windows Recycle Bin (items picked by hand; restored from Explorer or here).
    RecycleBin,
    /// Renamed into `<volume>\.nfd-quarantine\<op>\` (rule matches); restorable for
    /// 7 days, then purged. Falls back to the Recycle Bin on another volume or on
    /// FAT/exFAT/network drives.
    Quarantine,
    /// Gone for good. Expert only, 🟢 Safe items only, double confirmation.
    Permanent,
}

impl DeleteMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecycleBin => "recycle-bin",
            Self::Quarantine => "quarantine",
            Self::Permanent => "permanent",
        }
    }
}

/// One thing to delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub path: PathBuf,
    pub origin: Origin,
    /// Rule that proposed it (`None` for hand-picked items).
    pub rule_id: Option<String>,
    pub is_dir: bool,
    /// Bytes on disk freed by deleting it.
    pub bytes: u64,
    pub files: u64,
    /// Logical size and last write when planned (files; checked again before acting).
    pub size: u64,
    pub mtime: i64,
    /// Apps (exe names) that should be closed first.
    #[serde(default)]
    pub requires_closed: Vec<String>,
    pub mode: DeleteMode,
    pub selected: bool,
    /// Duplicates: the copy that stays, checked again (and, for links, linked to)
    /// right before this one is acted on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keeper: Option<PathBuf>,
}

impl PlanItem {
    /// A hand-picked item (Analyze, Old & Large, Tidy up): Recycle Bin, selected.
    #[must_use]
    pub fn manual(path: PathBuf, is_dir: bool, bytes: u64, files: u64, mtime: i64) -> Self {
        Self {
            path,
            origin: Origin::Manual,
            rule_id: None,
            is_dir,
            bytes,
            files,
            size: if is_dir { 0 } else { bytes },
            mtime,
            requires_closed: Vec::new(),
            mode: DeleteMode::RecycleBin,
            selected: true,
            keeper: None,
        }
    }

    /// Risk level (hand-picked items count as Safe: the user chose them).
    #[must_use]
    pub const fn level(&self) -> RiskLevel {
        match self.origin {
            Origin::Manual | Origin::Duplicate { .. } => RiskLevel::Safe,
            Origin::Rule { level } => level,
        }
    }

    /// What the gate compares with the disk right before acting.
    #[must_use]
    pub const fn expected(&self) -> Expected {
        Expected {
            is_dir: self.is_dir,
            size: self.size,
            mtime: self.mtime,
        }
    }
}

/// A list of items to delete, at most one per subtree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanPlan {
    pub items: Vec<PlanItem>,
}

impl CleanPlan {
    /// Selected items.
    pub fn selected(&self) -> impl Iterator<Item = &PlanItem> + '_ {
        self.items.iter().filter(|i| i.selected)
    }

    /// "Will free" total of the selected items.
    #[must_use]
    pub fn selected_bytes(&self) -> u64 {
        self.selected().map(|i| i.bytes).sum()
    }

    /// Total bytes per risk level of all items (selected or not).
    #[must_use]
    pub fn bytes_by_level(&self) -> [(RiskLevel, u64); 3] {
        let mut out = [
            (RiskLevel::Safe, 0),
            (RiskLevel::Caution, 0),
            (RiskLevel::Advanced, 0),
        ];
        for item in &self.items {
            if let Some(slot) = out.iter_mut().find(|(l, _)| *l == item.level()) {
                slot.1 += item.bytes;
            }
        }
        out
    }
}

/// Planner options.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOptions {
    /// Expert mode: 🟠 Advanced items are listed (never pre-selected).
    pub expert: bool,
    /// Pre-select 🟡 Caution items as well (`--level caution`).
    pub include_caution: bool,
    /// Delete permanently instead of Recycle Bin / Quarantine. Only honoured in
    /// Expert mode and only for 🟢 Safe rule items; everything else keeps its mode.
    pub permanent: bool,
}

/// Build a plan from candidates.
///
/// * 🔴 Protected items are dropped (the UI only explains them).
/// * 🟠 Advanced items are dropped unless `expert`, and never pre-selected.
/// * 🟢 Safe and hand-picked items are pre-selected; 🟡 Caution only with
///   `include_caution`.
/// * An item inside another item is merged into it (the outer one wins); duplicates
///   collapse. Rule matches go to Quarantine, hand-picked items to the Recycle Bin.
#[must_use]
pub fn build(candidates: Vec<PlanItem>, opts: PlanOptions) -> CleanPlan {
    let mut items: Vec<(String, PlanItem)> = candidates
        .into_iter()
        .filter(|c| match c.level() {
            RiskLevel::Protected => false,
            RiskLevel::Advanced => opts.expert,
            _ => true,
        })
        .filter_map(|mut c| {
            let key = comparable(&c.path)?;
            c.selected = match c.origin {
                Origin::Manual | Origin::Duplicate { .. } => true,
                Origin::Rule { level } => match level {
                    RiskLevel::Safe => true,
                    RiskLevel::Caution => opts.include_caution,
                    RiskLevel::Advanced | RiskLevel::Protected => false,
                },
            };
            c.mode = match c.origin {
                Origin::Manual | Origin::Duplicate { .. } => DeleteMode::RecycleBin,
                Origin::Rule {
                    level: RiskLevel::Safe,
                } if opts.permanent && opts.expert => DeleteMode::Permanent,
                Origin::Rule { .. } => DeleteMode::Quarantine,
            };
            Some((key, c))
        })
        .collect();
    // Sorting the comparable keys puts every folder right before its contents
    // ("a/b" < "a/b/c"), but "a/b c" sorts between them, so compare against the
    // whole stack of kept ancestors rather than only the previous key.
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut kept: Vec<(String, PlanItem)> = Vec::with_capacity(items.len());
    for (key, item) in items {
        let inside = kept.iter().rev().any(|(k, _)| is_within(&key, k));
        if !inside {
            kept.push((key, item));
        }
    }
    CleanPlan {
        items: kept.into_iter().map(|(_, i)| i).collect(),
    }
}

/// `true` when `exe` (case-insensitive, `.exe` optional) is among `running`.
#[must_use]
pub fn is_running(running: &[String], exe: &str) -> bool {
    nfd_platform::locks::is_running(running, exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(path: &str) -> PathBuf {
        PathBuf::from(if cfg!(windows) {
            format!("C:{path}")
        } else {
            path.to_owned()
        })
    }

    fn rule(path: &str, level: RiskLevel, bytes: u64) -> PlanItem {
        PlanItem {
            path: abs(path),
            origin: Origin::Rule { level },
            rule_id: Some("t.rule".into()),
            is_dir: true,
            bytes,
            files: 1,
            size: 0,
            mtime: 0,
            requires_closed: Vec::new(),
            mode: DeleteMode::RecycleBin,
            selected: false,
            keeper: None,
        }
    }

    #[test]
    fn levels_decide_visibility_and_selection() {
        let input = vec![
            rule("/c/safe", RiskLevel::Safe, 10),
            rule("/c/caution", RiskLevel::Caution, 20),
            rule("/c/adv", RiskLevel::Advanced, 30),
            rule("/c/prot", RiskLevel::Protected, 40),
        ];
        let plan = build(input.clone(), PlanOptions::default());
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.selected_bytes(), 10);
        assert!(plan.items.iter().all(|i| i.mode == DeleteMode::Quarantine));

        let expert = build(
            input,
            PlanOptions {
                expert: true,
                include_caution: true,
                permanent: true,
            },
        );
        assert_eq!(expert.items.len(), 3);
        assert_eq!(expert.selected_bytes(), 30);
        let modes: Vec<_> = expert.items.iter().map(|i| (i.level(), i.mode)).collect();
        assert!(modes.contains(&(RiskLevel::Safe, DeleteMode::Permanent)));
        assert!(modes.contains(&(RiskLevel::Caution, DeleteMode::Quarantine)));
    }

    #[test]
    fn nested_items_merge_into_the_outer_one() {
        let plan = build(
            vec![
                rule("/c/cache/a/b", RiskLevel::Safe, 1),
                rule("/c/cache", RiskLevel::Safe, 5),
                rule("/c/cache b", RiskLevel::Safe, 7),
                rule("/c/cache/z", RiskLevel::Safe, 1),
                rule("/c/cache/", RiskLevel::Safe, 5),
            ],
            PlanOptions::default(),
        );
        let paths: Vec<_> = plan.items.iter().map(|i| i.path.clone()).collect();
        assert_eq!(paths, vec![abs("/c/cache"), abs("/c/cache b")]);
        assert_eq!(plan.selected_bytes(), 12);
    }

    #[test]
    fn manual_items_go_to_the_recycle_bin() {
        let plan = build(
            vec![PlanItem::manual(abs("/d/x.iso"), false, 9, 1, 0)],
            PlanOptions {
                expert: true,
                include_caution: false,
                permanent: true,
            },
        );
        assert_eq!(plan.items[0].mode, DeleteMode::RecycleBin);
        assert!(plan.items[0].selected);
    }

    #[test]
    fn relative_paths_are_dropped() {
        let plan = build(
            vec![rule("relative/x", RiskLevel::Safe, 1)],
            PlanOptions::default(),
        );
        assert!(plan.items.is_empty());
    }
}
