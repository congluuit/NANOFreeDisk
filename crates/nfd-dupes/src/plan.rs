//! Turning a selection into a [`CleanPlan`] for the Phase 3 path (SafetyGate →
//! Executor → UndoJournal), and the check run right before each item is acted on.
//!
//! The planner refuses any selection that would leave a group without a copy, and
//! the [`Verifier`] refuses an item unless, at that very moment, both the copy and
//! the keeper still exist with the size and last write seen by the search **and**
//! hash (in full) to the group's hash. A keeper that changed or vanished skips every
//! remaining copy of its group.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use nfd_core::exec::Refusal;
use nfd_core::plan::{CleanPlan, DeleteMode, PlanItem};
use nfd_core::safety::Origin;
use serde::{Deserialize, Serialize};

use crate::hash::{self, Ctx, Source};
use crate::keep::Selection;
use crate::{DupeGroup, FileCopy};

/// What happens to the copies that go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// Windows Recycle Bin (default).
    #[default]
    Recycle,
    /// The app's quarantine, restorable for 7 days (Expert; faster for many files).
    Quarantine,
}

impl Action {
    #[must_use]
    pub const fn mode(self) -> DeleteMode {
        match self {
            Self::Recycle => DeleteMode::RecycleBin,
            Self::Quarantine => DeleteMode::Quarantine,
        }
    }
}

/// Why a selection cannot become a plan.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("group {0} is not in the report")]
    UnknownGroup(u32),
    #[error("group {0} is selected twice")]
    SelectedTwice(u32),
    #[error("{path} is not a copy of group {group}")]
    NotInGroup { group: u32, path: PathBuf },
    #[error("group {0} would lose every copy: one must stay")]
    NothingKept(u32),
}

/// Index of the copy `path` names (its path or one of its hard links).
fn copy_index(group: &DupeGroup, path: &Path) -> Option<usize> {
    group
        .copies
        .iter()
        .position(|c| c.path == path || c.links.iter().any(|l| l == path))
}

/// Build the plan for `selections` of `groups`.
///
/// Every removed copy becomes one item per name (its hard links included, or no
/// space would be freed); the keeper is recorded on each item.
///
/// # Errors
/// [`PlanError`] for a selection that names something outside its group, selects a
/// group twice, or keeps nothing.
pub fn plan(
    groups: &[DupeGroup],
    selections: &[Selection],
    action: Action,
) -> Result<CleanPlan, PlanError> {
    let by_id: HashMap<u32, &DupeGroup> = groups.iter().map(|g| (g.id, g)).collect();
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for s in selections {
        let g = by_id
            .get(&s.group)
            .copied()
            .ok_or(PlanError::UnknownGroup(s.group))?;
        if !seen.insert(s.group) {
            return Err(PlanError::SelectedTwice(s.group));
        }
        let keep = copy_index(g, &s.keep).ok_or_else(|| PlanError::NotInGroup {
            group: s.group,
            path: s.keep.clone(),
        })?;
        let mut remove: Vec<usize> = Vec::new();
        for p in &s.remove {
            let i = copy_index(g, p).ok_or_else(|| PlanError::NotInGroup {
                group: s.group,
                path: p.clone(),
            })?;
            if i == keep {
                return Err(PlanError::NothingKept(s.group));
            }
            if !remove.contains(&i) {
                remove.push(i);
            }
        }
        if remove.len() >= g.copies.len() {
            return Err(PlanError::NothingKept(s.group));
        }
        let keeper = g.copies[keep].path.clone();
        for i in remove {
            let c = &g.copies[i];
            for (n, path) in std::iter::once(&c.path).chain(&c.links).enumerate() {
                items.push(PlanItem {
                    path: path.clone(),
                    origin: Origin::Duplicate { group: g.id },
                    rule_id: None,
                    is_dir: false,
                    bytes: if n == 0 { g.size } else { 0 },
                    files: u64::from(n == 0),
                    size: g.size,
                    mtime: c.mtime,
                    requires_closed: Vec::new(),
                    mode: action.mode(),
                    selected: true,
                    keeper: Some(keeper.clone()),
                });
            }
        }
    }
    Ok(CleanPlan { items })
}

/// The last check before a copy is touched (see the module documentation).
pub struct Verifier<'a> {
    source: &'a dyn Source,
    groups: HashMap<u32, &'a DupeGroup>,
    /// Keepers already hashed in this run, and the answer.
    keepers: HashMap<PathBuf, Result<(), Refusal>>,
    ctx: Ctx<'a>,
}

impl std::fmt::Debug for Verifier<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verifier")
            .field("groups", &self.groups.len())
            .field("keepers", &self.keepers)
            .finish_non_exhaustive()
    }
}

fn refusal(code: &'static str, message: impl Into<String>) -> Refusal {
    Refusal {
        code,
        message: message.into(),
    }
}

impl<'a> Verifier<'a> {
    #[must_use]
    pub fn new(source: &'a dyn Source, groups: &'a [DupeGroup]) -> Self {
        Self {
            source,
            groups: groups.iter().map(|g| (g.id, g)).collect(),
            keepers: HashMap::new(),
            ctx: Ctx::default(),
        }
    }

    /// Hash `copy` (named `path`) in full and compare with the group.
    fn same(&self, g: &DupeGroup, path: &Path, copy: &FileCopy) -> bool {
        hash::full(self.source, path, g.size, copy.mtime, &self.ctx)
            .is_ok_and(|d| hash::hex(&d) == g.hash)
    }

    /// `Ok` when `item` may be acted on now. Items that are not duplicates pass.
    ///
    /// # Errors
    /// `keeper-changed` (the copy that stays is gone, changed or unreadable — the
    /// whole group is skipped), `copy-changed`, or `not-a-duplicate` for an item that
    /// does not match the search.
    pub fn check(&mut self, item: &PlanItem) -> Result<(), Refusal> {
        let Origin::Duplicate { group } = item.origin else {
            return Ok(());
        };
        let g = *self
            .groups
            .get(&group)
            .ok_or_else(|| refusal("not-a-duplicate", "the group is not in the search result"))?;
        let keeper = item
            .keeper
            .as_deref()
            .ok_or_else(|| refusal("not-a-duplicate", "no copy is kept"))?;
        let (Some(k), Some(i)) = (copy_index(g, keeper), copy_index(g, &item.path)) else {
            return Err(refusal(
                "not-a-duplicate",
                "the item or its keeper is not a copy of the group",
            ));
        };
        if k == i || item.size != g.size {
            return Err(refusal(
                "not-a-duplicate",
                "the item is the copy that stays",
            ));
        }
        let kc = &g.copies[k];
        let answer = match self.keepers.get(keeper) {
            Some(done) => done.clone().and_then(|()| {
                // hashed already: it must still be there, unchanged
                hash::check(self.source, keeper, g.size, kc.mtime)
                    .map(|_| ())
                    .map_err(|_| {
                        refusal("keeper-changed", "the copy that stays changed or is gone")
                    })
            }),
            None => {
                let r = if self.same(g, keeper, kc) {
                    Ok(())
                } else {
                    Err(refusal(
                        "keeper-changed",
                        "the copy that stays changed or is gone",
                    ))
                };
                self.keepers.insert(keeper.to_path_buf(), r.clone());
                r
            }
        };
        answer?;
        if self.same(g, &item.path, &g.copies[i]) {
            Ok(())
        } else {
            Err(refusal(
                "copy-changed",
                "this copy changed since the search",
            ))
        }
    }
}
