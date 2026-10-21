//! Carrying out a leftover removal (`docs/UNINSTALLER.md` §2.4 "Thực thi").
//!
//! The ticked leftovers are split three ways:
//! * this user's files and folders → a [`CleanPlan`] for Planner → SafetyGate →
//!   Executor (quarantine, origin [`Origin::Leftover`]);
//! * this user's registry keys and values → [`RegJob`]s: exported to a `.reg`
//!   backup, then deleted ([`run_registry`]);
//! * everything all-users (Program Files, ProgramData, the all-users Start Menu
//!   and Desktop, `HKEY_LOCAL_MACHINE`) → one [`elevated::Request`] for a single
//!   administrator prompt.
//!
//! Every result is an [`ItemResult`] so the three parts journal as one operation.

use std::path::{Path, PathBuf};

use nfd_core::exec::{ItemResult, ItemStatus};
use nfd_core::plan::{self, CleanPlan, DeleteMode, PlanItem, PlanOptions};
use nfd_core::safety::Origin;
use nfd_platform::registry::{self as reg, Hive};
use serde::{Deserialize, Serialize};

use crate::Scope;
use crate::elevated::{self, FileJob};
use crate::inventory::norm;
use crate::leftovers::{Kind, Leftover, Report, registry_refusal};

/// A registry key or value to back up and delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegJob {
    /// `HKCU` / `HKLM`.
    pub hive: String,
    pub key: String,
    pub value: Option<String>,
    /// Where the `.reg` backup goes.
    pub backup: PathBuf,
    /// Why it is a leftover (journal: `leftover.<reason>`).
    pub reason: String,
}

impl RegJob {
    /// `HKEY_…\key` (`\value` for a value), as journaled.
    #[must_use]
    pub fn display(&self) -> String {
        let hive =
            Hive::parse(&self.hive).map_or_else(|| self.hive.clone(), |h| h.name().to_owned());
        match &self.value {
            Some(v) => format!(r"{hive}\{}\{v}", self.key),
            None => format!(r"{hive}\{}", self.key),
        }
    }
}

/// A removal, split by who may carry it out.
#[derive(Debug, Clone, Default)]
pub struct Prepared {
    /// This user's files and folders.
    pub plan: CleanPlan,
    /// This user's registry keys and values.
    pub registry: Vec<RegJob>,
    /// All-users items (one administrator prompt).
    pub machine: elevated::Request,
}

impl Prepared {
    /// Nothing to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plan.items.is_empty() && self.registry.is_empty() && self.machine.is_empty()
    }

    /// Bytes the files would free.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.plan.selected_bytes() + self.machine.files.iter().map(|f| f.bytes).sum::<u64>()
    }
}

/// The `picked` ids of `report` (blocked items left out), split.
/// `backup_dir` receives the `.reg` backups (`<n>.reg`); `op_id` names the request.
#[must_use]
pub fn prepare(report: &Report, picked: &[String], backup_dir: &Path, op_id: &str) -> Prepared {
    let wanted: Vec<String> = picked.iter().map(|p| norm(Path::new(p))).collect();
    let chosen: Vec<&Leftover> = report
        .items
        .iter()
        .filter(|i| i.blocked.is_none() && wanted.contains(&norm(Path::new(&i.id))))
        .collect();
    let mut out = Prepared {
        machine: elevated::Request::new(op_id),
        ..Prepared::default()
    };
    let mut files: Vec<PlanItem> = Vec::new();
    let mut n = 0usize;
    for it in chosen {
        match it.kind {
            Kind::Folder | Kind::File | Kind::Shortcut => {
                let path = PathBuf::from(&it.path);
                let is_dir = it.kind == Kind::Folder;
                let info = nfd_platform::fsinfo::file_info(&path).ok();
                let (size, mtime) = if is_dir {
                    (0, it.mtime)
                } else {
                    info.map_or((it.bytes, it.mtime), |i| (i.logical, i.mtime))
                };
                if it.scope == Scope::Machine {
                    out.machine.files.push(FileJob {
                        path,
                        is_dir,
                        bytes: it.bytes,
                        files: it.files,
                        size,
                        mtime,
                        reason: it.reason.clone(),
                    });
                } else {
                    files.push(PlanItem {
                        path,
                        origin: Origin::Leftover,
                        rule_id: Some(format!("leftover.{}", it.reason)),
                        is_dir,
                        bytes: it.bytes,
                        files: it.files,
                        size,
                        mtime,
                        requires_closed: Vec::new(),
                        mode: DeleteMode::Quarantine,
                        selected: true,
                        keeper: None,
                    });
                }
            }
            Kind::RegistryKey | Kind::RegistryValue => {
                let Some((hive, key)) = it.reg_key() else {
                    continue;
                };
                n += 1;
                let job = RegJob {
                    hive: hive.short().to_owned(),
                    key: key.to_owned(),
                    value: it.value.clone(),
                    backup: backup_dir.join(format!("{n}.reg")),
                    reason: it.reason.clone(),
                };
                if hive == Hive::LocalMachine {
                    out.machine.registry.push(job);
                } else {
                    out.registry.push(job);
                }
            }
            Kind::Service => {}
        }
    }
    let mut p = plan::build(files, PlanOptions::default());
    for item in &mut p.items {
        item.mode = DeleteMode::Quarantine;
        item.selected = true;
    }
    out.plan = p;
    out
}

/// The journal item of a registry job, not carried out yet (status `skipped`).
#[must_use]
pub fn registry_result(job: &RegJob, n: usize) -> ItemResult {
    ItemResult {
        n,
        path: PathBuf::from(job.display()),
        rule_id: Some(format!("leftover.{}", job.reason)),
        mode: DeleteMode::Registry,
        is_dir: job.value.is_none(),
        bytes: 0,
        files: 0,
        freed: 0,
        status: ItemStatus::Skipped,
        reason: None,
        message: None,
        stored: None,
        warnings: Vec::new(),
    }
}

/// Back up and delete one registry key or value. `n` numbers the journal item.
/// Refused unless it is a leftover location ([`registry_refusal`]); `allow_machine`
/// lets `HKEY_LOCAL_MACHINE` through (the elevated run).
#[must_use]
pub fn run_registry_one(job: &RegJob, n: usize, allow_machine: bool) -> ItemResult {
    let mut r = registry_result(job, n);
    let Some(hive) = Hive::parse(&job.hive) else {
        r.reason = Some("registry-protected".into());
        return r;
    };
    if (hive == Hive::LocalMachine && !allow_machine)
        || registry_refusal(hive, &job.key, job.value.is_some()).is_some()
        || !job
            .backup
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("reg"))
    {
        r.reason = Some("registry-protected".into());
        return r;
    }
    if !reg::exists(hive, &job.key) {
        r.reason = Some("missing".into());
        return r;
    }
    if let Some(parent) = job.backup.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            r.status = ItemStatus::Failed;
            r.reason = Some("backup".into());
            r.message = Some(e.to_string());
            return r;
        }
    }
    let backup = match &job.value {
        Some(v) => reg::export_value(hive, &job.key, v, &job.backup),
        None => reg::export_key(hive, &job.key, &job.backup),
    };
    if let Err(e) = backup {
        // no backup, no delete
        r.status = ItemStatus::Failed;
        r.reason = Some("backup".into());
        r.message = Some(e.to_string());
        return r;
    }
    let deleted = match &job.value {
        Some(v) => reg::delete_value(hive, &job.key, v),
        None => reg::delete_tree(hive, &job.key),
    };
    match deleted {
        Ok(()) => {
            r.status = ItemStatus::Done;
            r.stored = Some(job.backup.clone());
        }
        Err(e) => {
            r.status = ItemStatus::Failed;
            r.reason = Some(if e.kind() == std::io::ErrorKind::PermissionDenied {
                "access-denied".into()
            } else {
                "delete".into()
            });
            r.message = Some(e.to_string());
            let _ = std::fs::remove_file(&job.backup);
        }
    }
    r
}

/// [`run_registry_one`] for every job, numbering from `first`.
#[must_use]
pub fn run_registry(jobs: &[RegJob], first: usize, allow_machine: bool) -> Vec<ItemResult> {
    jobs.iter()
        .enumerate()
        .map(|(i, j)| run_registry_one(j, first + i, allow_machine))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Program;
    use crate::leftovers::Mode;
    use nfd_core::RiskLevel;

    fn left(id: &str, kind: Kind, scope: Scope, value: Option<&str>) -> Leftover {
        Leftover {
            id: id.to_owned(),
            kind,
            path: id.split('|').next().unwrap_or(id).to_owned(),
            value: value.map(str::to_owned),
            scope,
            level: RiskLevel::Safe,
            reason: "same-name".into(),
            detail: None,
            also: None,
            bytes: 10,
            files: 1,
            mtime: 0,
            selected: true,
            blocked: None,
            large: false,
        }
    }

    fn report(items: Vec<Leftover>) -> Report {
        let program: Program = serde_json::from_str(
            r#"{"id":"hklm:x","name":"X","publisher":null,"version":null,"source":"win32","scope":"machine","arch":null,"installLocation":null,"uninstall":null,"quietUninstall":null,"icon":null,"installedAt":null,"size":0,"sizeMeasured":false,"productCode":null,"key":null,"package":null,"url":null,"caution":null,"blocked":null,"broken":false}"#,
        )
        .expect("program json");
        Report {
            program,
            mode: Mode::AfterUninstall,
            items,
            selected_bytes: 0,
            total_bytes: 0,
        }
    }

    #[test]
    fn splits_by_owner() {
        let r = report(vec![
            left(
                r"C:\Users\nano\AppData\Roaming\X",
                Kind::Folder,
                Scope::User,
                None,
            ),
            left(r"C:\Program Files\X", Kind::Folder, Scope::Machine, None),
            left(
                r"HKEY_CURRENT_USER\Software\X",
                Kind::RegistryKey,
                Scope::User,
                None,
            ),
            left(
                r"HKEY_LOCAL_MACHINE\SOFTWARE\X",
                Kind::RegistryKey,
                Scope::Machine,
                None,
            ),
            left(
                r"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run|X",
                Kind::RegistryValue,
                Scope::User,
                Some("X"),
            ),
            left("service:X", Kind::Service, Scope::Machine, None),
        ]);
        let ids: Vec<String> = r.items.iter().map(|i| i.id.clone()).collect();
        let p = prepare(&r, &ids, Path::new(r"C:\backup"), "op");
        assert_eq!(p.plan.items.len(), 1);
        assert_eq!(p.plan.items[0].origin, Origin::Leftover);
        assert_eq!(p.plan.items[0].mode, DeleteMode::Quarantine);
        assert_eq!(p.machine.files.len(), 1);
        assert_eq!(p.registry.len(), 2);
        assert_eq!(p.registry[1].value.as_deref(), Some("X"));
        assert_eq!(p.machine.registry.len(), 1);
        assert_eq!(p.machine.registry[0].hive, "HKLM");
        assert!(
            p.registry
                .iter()
                .chain(&p.machine.registry)
                .all(|j| j.backup.extension().is_some_and(|e| e == "reg"))
        );
        // only what was ticked
        let p = prepare(
            &r,
            &[r"C:\Program Files\X".to_owned()],
            Path::new(r"C:\b"),
            "op",
        );
        assert!(p.plan.items.is_empty() && p.registry.is_empty() && p.machine.files.len() == 1);
    }

    #[test]
    fn protected_keys_are_refused_before_anything_happens() {
        let job = RegJob {
            hive: "HKCU".into(),
            key: r"Software\Microsoft\Windows".into(),
            value: None,
            backup: PathBuf::from(r"C:\nope\1.reg"),
            reason: "software-key".into(),
        };
        let r = run_registry_one(&job, 0, false);
        assert_eq!(r.status, ItemStatus::Skipped);
        assert_eq!(r.reason.as_deref(), Some("registry-protected"));
        let lm = RegJob {
            hive: "HKLM".into(),
            key: r"SOFTWARE\Acme".into(),
            ..job.clone()
        };
        assert_eq!(
            run_registry_one(&lm, 0, false).reason.as_deref(),
            Some("registry-protected")
        );
        let not_reg = RegJob {
            key: r"Software\Acme".into(),
            backup: PathBuf::from(r"C:\x.txt"),
            ..job
        };
        assert_eq!(
            run_registry_one(&not_reg, 0, false).reason.as_deref(),
            Some("registry-protected")
        );
    }

    #[cfg(windows)]
    #[test]
    fn removes_a_user_key_with_a_backup_and_restores_it() {
        let name = format!("NANOFreeDiskLeftoverTest{}", std::process::id());
        let key = format!(r"Software\{name}");
        let dir = tempfile::tempdir().expect("tmp");
        let seed = dir.path().join("seed.reg");
        std::fs::write(
            &seed,
            reg::utf16_file(&reg::reg_text_for_value(
                Hive::CurrentUser,
                &format!(r"{key}\Sub"),
                "A",
                &reg::Value::Dword(1),
            )),
        )
        .expect("seed");
        reg::import(&seed).expect("import");
        let job = RegJob {
            hive: "HKCU".into(),
            key: key.clone(),
            value: None,
            backup: dir.path().join("b").join("1.reg"),
            reason: "software-key".into(),
        };
        let r = run_registry_one(&job, 3, false);
        assert_eq!(r.status, ItemStatus::Done, "{r:?}");
        assert_eq!(r.n, 3);
        assert_eq!(r.mode, DeleteMode::Registry);
        assert!(!reg::exists(Hive::CurrentUser, &key));
        let (status, msg) =
            nfd_core::exec::restore(&r.path, r.stored.as_deref(), DeleteMode::Registry, 0);
        assert_eq!(status, nfd_core::exec::RestoreStatus::Restored, "{msg:?}");
        assert!(reg::exists(Hive::CurrentUser, &format!(r"{key}\Sub")));
        reg::delete_tree(Hive::CurrentUser, &key).expect("cleanup");
        // a key that is gone is skipped, not failed
        assert_eq!(
            run_registry_one(&job, 0, false).reason.as_deref(),
            Some("missing")
        );
    }
}
