//! Removing look-alikes the user picked.
//!
//! Similar is not identical, so nothing is chosen for the user and the exact
//! search's [`crate::plan::Verifier`] (which hashes keeper and copy byte for byte)
//! does not apply. A selection becomes an ordinary hand-picked deletion instead:
//!
//! * every photo must belong to its group and pass the SafetyGate's manual rules
//!   (what `safety::check_manual_delete` checks for a file removed from Analyze);
//! * a selection that removes every photo of a group is refused unless the user
//!   ticked "keep none" for that group;
//! * each item carries the size and last write seen by the search: the gate refuses
//!   it right before the move if either changed, and [`SimilarVerifier`] refuses a
//!   cloud placeholder and an item whose group no longer has a photo that stays.
//!
//! Recycle Bin or quarantine only: replacing a look-alike by a link to another
//! picture would change what the user sees.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use nfd_core::exec::Refusal;
use nfd_core::plan::{CleanPlan, DeleteMode, PlanItem};
use nfd_core::safety::{Origin, SafetyGate};
use serde::{Deserialize, Serialize};

use super::{Photo, SimilarGroup};
use crate::hash::Source;
use crate::plan::{Action, PlanError};

/// The photos of one group the user wants gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarSelection {
    pub group: u32,
    pub remove: Vec<PathBuf>,
    /// The user confirmed that no photo of this group stays.
    #[serde(default)]
    pub keep_none: bool,
}

/// The photo of `group` named `path` (its path or one of its links).
fn photo_index(group: &SimilarGroup, path: &Path) -> Option<usize> {
    group
        .photos
        .iter()
        .position(|p| p.path == path || p.links.iter().any(|l| l == path))
}

/// The photos that stay when `selection` is carried out, in group order (empty
/// with `keep_none`, or for a selection naming photos outside the group).
#[must_use]
pub fn kept<'a>(group: &'a SimilarGroup, remove: &[PathBuf]) -> Vec<&'a Photo> {
    let gone: HashSet<usize> = remove
        .iter()
        .filter_map(|p| photo_index(group, p))
        .collect();
    group
        .photos
        .iter()
        .enumerate()
        .filter(|(i, _)| !gone.contains(i))
        .map(|(_, p)| p)
        .collect()
}

/// Build the plan for `selections` of `groups`: one hand-picked item per name of
/// each photo (its hard links too, or no space would be freed).
///
/// # Errors
/// [`PlanError`] for an unknown group, a group selected twice, a photo outside its
/// group, a group that would lose every photo without `keep_none`, a photo the gate
/// refuses ([`PlanError::Protected`]) and [`Action::Link`].
pub fn plan(
    groups: &[SimilarGroup],
    selections: &[SimilarSelection],
    action: Action,
    gate: &SafetyGate,
) -> Result<CleanPlan, PlanError> {
    let by_id: HashMap<u32, &SimilarGroup> = groups.iter().map(|g| (g.id, g)).collect();
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
        let mode = match action {
            Action::Recycle => DeleteMode::RecycleBin,
            Action::Quarantine => DeleteMode::Quarantine,
            Action::Link => {
                return Err(PlanError::CannotLink {
                    group: g.id,
                    path: s.remove.first().cloned().unwrap_or_default(),
                    why: "similar",
                });
            }
        };
        let mut remove: Vec<usize> = Vec::new();
        for p in &s.remove {
            let i = photo_index(g, p).ok_or_else(|| PlanError::NotInGroup {
                group: s.group,
                path: p.clone(),
            })?;
            if !remove.contains(&i) {
                remove.push(i);
            }
        }
        if remove.len() >= g.photos.len() && !s.keep_none {
            return Err(PlanError::NothingKept(s.group));
        }
        for i in remove {
            let photo = &g.photos[i];
            for (n, path) in std::iter::once(&photo.path).chain(&photo.links).enumerate() {
                // the rules a file removed by hand in Analyze is held to
                if gate.check_path(path, Origin::Manual).is_err() {
                    return Err(PlanError::Protected {
                        group: g.id,
                        path: path.clone(),
                    });
                }
                items.push(PlanItem {
                    path: path.clone(),
                    origin: Origin::Manual,
                    rule_id: None,
                    is_dir: false,
                    bytes: if n == 0 { photo.size } else { 0 },
                    files: u64::from(n == 0),
                    // checked against the disk right before the move
                    size: photo.size,
                    mtime: photo.mtime,
                    requires_closed: Vec::new(),
                    mode,
                    selected: true,
                    keeper: None,
                });
            }
        }
    }
    Ok(CleanPlan { items })
}

fn refusal(code: &'static str, message: &str) -> Refusal {
    Refusal {
        code,
        message: message.to_owned(),
    }
}

/// One group as the verifier sees it.
#[derive(Debug)]
struct Watched {
    group: SimilarGroup,
    /// Names of the photos to remove.
    remove: HashSet<PathBuf>,
    keep_none: bool,
}

/// The last check before a selected photo is touched (see the module docs).
pub struct SimilarVerifier<'a> {
    source: &'a dyn Source,
    groups: Vec<Watched>,
    /// Group (index into `groups`) of every name to remove.
    by_path: HashMap<PathBuf, usize>,
}

impl std::fmt::Debug for SimilarVerifier<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimilarVerifier")
            .field("groups", &self.groups.len())
            .finish_non_exhaustive()
    }
}

impl<'a> SimilarVerifier<'a> {
    /// A verifier for `selections` of `groups` (it keeps its own copy, so it can
    /// travel to a worker thread).
    #[must_use]
    pub fn new(
        source: &'a dyn Source,
        groups: &[SimilarGroup],
        selections: &[SimilarSelection],
    ) -> Self {
        let mut watched = Vec::new();
        let mut by_path = HashMap::new();
        for s in selections {
            let Some(g) = groups.iter().find(|g| g.id == s.group) else {
                continue;
            };
            let mut remove = HashSet::new();
            for p in &s.remove {
                if let Some(i) = photo_index(g, p) {
                    let photo = &g.photos[i];
                    for name in std::iter::once(&photo.path).chain(&photo.links) {
                        remove.insert(name.clone());
                        by_path.insert(name.clone(), watched.len());
                    }
                }
            }
            watched.push(Watched {
                group: g.clone(),
                remove,
                keep_none: s.keep_none,
            });
        }
        Self {
            source,
            groups: watched,
            by_path,
        }
    }

    /// `true` when `photo` is on disk right now as the search saw it.
    fn unchanged(&self, photo: &Photo) -> bool {
        self.source
            .info(&photo.path)
            .is_ok_and(|i| !i.link && !i.is_dir && i.size == photo.size && i.mtime == photo.mtime)
    }

    /// `Ok` when `item` may be removed now.
    ///
    /// # Errors
    /// `not-similar` (not part of the selection), `cloud-file` (it became a cloud
    /// placeholder: removing it would remove it from the cloud too), `copy-changed`,
    /// or `keeper-changed` when no photo of its group that stays is still there
    /// unchanged (unless the group was confirmed with "keep none").
    pub fn check(&mut self, item: &PlanItem) -> Result<(), Refusal> {
        let Some(&g) = self.by_path.get(&item.path) else {
            return Err(refusal(
                "not-similar",
                "the item is not a selected look-alike",
            ));
        };
        let w = &self.groups[g];
        let info = self
            .source
            .info(&item.path)
            .map_err(|_| refusal("copy-changed", "this photo is gone"))?;
        if info.cloud {
            return Err(refusal(
                "cloud-file",
                "this photo is only in the cloud now: removing it would remove it there too",
            ));
        }
        if info.link || info.is_dir || info.size != item.size || info.mtime != item.mtime {
            return Err(refusal(
                "copy-changed",
                "this photo changed since the search",
            ));
        }
        if w.keep_none {
            return Ok(());
        }
        let stays = w.group.photos.iter().any(|p| {
            !w.remove.contains(&p.path)
                && !p.links.iter().any(|l| w.remove.contains(l))
                && self.unchanged(p)
        });
        if stays {
            Ok(())
        } else {
            Err(refusal(
                "keeper-changed",
                "no photo of this group that stays is still there unchanged",
            ))
        }
    }
}
