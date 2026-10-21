//! The one elevated step of a leftover removal (`docs/UNINSTALLER.md` §2.4):
//! all-users leftovers — folders in Program Files / ProgramData, shortcuts of the
//! all-users Start Menu and Desktop, `HKEY_LOCAL_MACHINE` keys — need
//! administrator rights. The app writes a [`Request`] to a file and starts itself
//! once through UAC with [`FLAG`]; that process re-checks **every** item on its own
//! (SafetyGate with [`Origin::Leftover`], all-users locations only, registry keys
//! of the allowed shapes, `.reg` files that only name such keys), carries them
//! out, writes a [`Response`] and exits.
//!
//! Files go to the Recycle Bin (restorable from Explorer, which asks for the
//! rights itself); keys are exported to `.reg` before they are deleted. There is no
//! long-lived elevated process and no way to reach this without the UAC prompt.

use std::path::{Path, PathBuf};

use nfd_core::exec::{self, ExecOptions, ItemResult, ItemStatus, QuarantineRoots, RestoreStatus};
use nfd_core::plan::{CleanPlan, DeleteMode, PlanItem};
use nfd_core::safety::{LiveProbe, Origin, SafetyGate};
use nfd_platform::registry::{self as reg, Hive};
use serde::{Deserialize, Serialize};

use crate::inventory::within;
use crate::leftovers::{Roots, registry_refusal};
use crate::remove::{RegJob, run_registry_one};

/// Command-line switch of the elevated step: `--elevated-leftovers <request> <response>`.
pub const FLAG: &str = "--elevated-leftovers";
/// Wire version; a mismatch is refused (an older app left a request behind).
pub const PROTOCOL: u32 = 1;

/// An all-users file or folder to remove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileJob {
    pub path: PathBuf,
    pub is_dir: bool,
    pub bytes: u64,
    pub files: u64,
    /// Logical size and last write of a file when planned.
    pub size: u64,
    pub mtime: i64,
    pub reason: String,
}

/// An all-users item to put back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreJob {
    /// Journal item number.
    pub n: u64,
    /// The original path (`HKEY_…` for registry items).
    pub path: String,
    pub mode: DeleteMode,
    pub stored: Option<PathBuf>,
    /// A registry key (else one value: `path` = `key\value`).
    pub is_key: bool,
    pub deleted_after: i64,
}

/// What the app asks the elevated step to do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub protocol: u32,
    pub op_id: String,
    pub files: Vec<FileJob>,
    pub registry: Vec<RegJob>,
    pub restore: Vec<RestoreJob>,
}

impl Request {
    #[must_use]
    pub fn new(op_id: &str) -> Self {
        Self {
            protocol: PROTOCOL,
            op_id: op_id.to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.registry.is_empty() && self.restore.is_empty()
    }
}

/// One restored item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Restored {
    pub n: u64,
    pub status: RestoreStatus,
    pub message: Option<String>,
}

/// What the elevated step did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub protocol: u32,
    /// Removal results, numbered from 0 (files first, then registry).
    pub items: Vec<ItemResult>,
    pub restored: Vec<Restored>,
    /// The whole request was refused.
    pub error: Option<String>,
}

/// All-users locations: Program Files, ProgramData, the all-users Start Menu and
/// the public Desktop.
fn machine_roots(roots: &Roots) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = roots.program_files.clone();
    out.extend(roots.program_data.iter().cloned());
    out.extend(
        roots
            .start_menus
            .iter()
            .chain(roots.desktops.iter())
            .filter(|(_, s)| *s == crate::Scope::Machine)
            .map(|(p, _)| p.clone()),
    );
    out
}

/// Why the elevated step refuses a file job, if it does.
#[must_use]
pub fn refuse_file(job: &FileJob, gate: &SafetyGate, roots: &Roots) -> Option<String> {
    let machine = machine_roots(roots);
    if !machine
        .iter()
        .any(|r| within(&job.path, r) && !within(r, &job.path))
    {
        return Some("not-all-users".into());
    }
    gate.check_path(&job.path, Origin::Leftover)
        .err()
        .map(|b| b.code().to_owned())
}

/// Why the elevated step refuses a registry job, if it does.
#[must_use]
pub fn refuse_registry(job: &RegJob) -> Option<&'static str> {
    if Hive::parse(&job.hive) != Some(Hive::LocalMachine) {
        return Some("not-all-users");
    }
    if !job
        .backup
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("reg"))
    {
        return Some("registry-protected");
    }
    registry_refusal(Hive::LocalMachine, &job.key, job.value.is_some())
}

/// Why a `.reg` backup may not be imported with administrator rights: every key it
/// writes must be an all-users leftover location (bytes of the file).
///
/// It must hold exactly what the journal says was removed: for a key (`original` =
/// `HKEY_LOCAL_MACHINE\key`), that key and its subkeys; for a value (`original` =
/// `HKEY_LOCAL_MACHINE\key\value`), one value of that key — so a tampered backup
/// cannot add, say, a `Run` entry for every user.
#[must_use]
pub fn refuse_reg_file(bytes: &[u8], original: &str, is_key: bool) -> Option<&'static str> {
    let keys = reg::reg_file_keys(bytes);
    if keys.is_empty() {
        return Some("not-a-backup");
    }
    let Some((hive, path)) = Hive::split(original) else {
        return Some("registry-protected");
    };
    if hive != Hive::LocalMachine {
        return Some("not-all-users");
    }
    let (key, value) = if is_key {
        (path, None)
    } else {
        match path.rsplit_once('\\') {
            Some((k, v)) => (k, Some(v)),
            None => return Some("registry-protected"),
        }
    };
    if registry_refusal(hive, key, value.is_some()).is_some() {
        return Some("registry-protected");
    }
    let key_l = key.to_lowercase();
    for k in &keys {
        let Some((h, p)) = Hive::split(k) else {
            return Some("registry-protected");
        };
        let p = p.to_lowercase();
        let inside = p == key_l || (is_key && p.starts_with(&format!("{key_l}\\")));
        if h != Hive::LocalMachine || !inside {
            return Some("unexpected-key");
        }
    }
    if let Some(v) = value {
        let names = reg::reg_file_value_names(bytes);
        if keys.len() != 1 || names.len() != 1 || !names[0].eq_ignore_ascii_case(v) {
            return Some("unexpected-value");
        }
    }
    None
}

/// Carry out `req` (already elevated). `roots` and `gate` describe this machine.
#[must_use]
pub fn execute(req: &Request, gate: &SafetyGate, roots: &Roots) -> Response {
    let mut out = Response {
        protocol: PROTOCOL,
        ..Response::default()
    };
    if req.protocol != PROTOCOL {
        out.error = Some(format!("protocol {} != {PROTOCOL}", req.protocol));
        return out;
    }
    // files: SafetyGate again, then the Recycle Bin (one by one, through the executor)
    let mut plan = CleanPlan::default();
    let mut refused: Vec<(usize, String)> = Vec::new();
    for (n, f) in req.files.iter().enumerate() {
        if let Some(why) = refuse_file(f, gate, roots) {
            refused.push((n, why));
        }
        plan.items.push(PlanItem {
            path: f.path.clone(),
            origin: Origin::Leftover,
            rule_id: Some(format!("leftover.{}", f.reason)),
            is_dir: f.is_dir,
            bytes: f.bytes,
            files: f.files,
            size: f.size,
            mtime: f.mtime,
            requires_closed: Vec::new(),
            mode: DeleteMode::RecycleBin,
            selected: !refused.iter().any(|(r, _)| *r == n),
            keeper: None,
        });
    }
    if !plan.items.is_empty() {
        let opts = ExecOptions {
            op_id: req.op_id.clone(),
            kind: "uninstall".into(),
            owner: 0,
            roots: QuarantineRoots::default(),
            now: exec::unix_now(),
        };
        let op = exec::execute(&plan, gate, &LiveProbe, &opts, &mut |_, _, _| true);
        for mut item in op.items {
            if let Some((_, why)) = refused.iter().find(|(r, _)| *r == item.n) {
                item.status = ItemStatus::Skipped;
                item.reason = Some(why.clone());
            }
            out.items.push(item);
        }
    }
    // registry: shape checked, backed up, deleted
    let first = out.items.len();
    for (i, job) in req.registry.iter().enumerate() {
        let n = first + i;
        if let Some(why) = refuse_registry(job) {
            let mut r = crate::remove::registry_result(job, n);
            r.reason = Some(why.into());
            out.items.push(r);
            continue;
        }
        out.items.push(run_registry_one(job, n, true));
    }
    // restores
    for job in &req.restore {
        let (status, message) = restore_one(job, gate, roots);
        out.restored.push(Restored {
            n: job.n,
            status,
            message,
        });
    }
    out
}

fn restore_one(
    job: &RestoreJob,
    gate: &SafetyGate,
    roots: &Roots,
) -> (RestoreStatus, Option<String>) {
    match job.mode {
        DeleteMode::Registry => {
            let Some(stored) = job.stored.as_deref() else {
                return (RestoreStatus::Missing, None);
            };
            let Ok(bytes) = std::fs::read(stored) else {
                return (RestoreStatus::Missing, None);
            };
            if let Some(why) = refuse_reg_file(&bytes, &job.path, job.is_key) {
                return (RestoreStatus::Failed, Some(why.into()));
            }
            exec::restore(
                Path::new(&job.path),
                Some(stored),
                DeleteMode::Registry,
                job.deleted_after,
            )
        }
        DeleteMode::RecycleBin => {
            let fake = FileJob {
                path: PathBuf::from(&job.path),
                is_dir: true,
                bytes: 0,
                files: 0,
                size: 0,
                mtime: 0,
                reason: String::new(),
            };
            if let Some(why) = refuse_file(&fake, gate, roots) {
                return (RestoreStatus::Failed, Some(why));
            }
            exec::restore(
                Path::new(&job.path),
                None,
                DeleteMode::RecycleBin,
                job.deleted_after,
            )
        }
        _ => (RestoreStatus::Failed, Some("unsupported".into())),
    }
}

/// `--elevated-leftovers <request> <response>` → the paths.
#[must_use]
pub fn args(argv: &[String]) -> Option<(PathBuf, PathBuf)> {
    let i = argv.iter().position(|a| a == FLAG)?;
    let req = argv.get(i + 1)?;
    let resp = argv.get(i + 2)?;
    let (req, resp) = (PathBuf::from(req), PathBuf::from(resp));
    let json = |p: &Path| {
        p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    };
    (req.is_absolute() && resp.is_absolute() && json(&req) && json(&resp)).then_some((req, resp))
}

/// Entry point of the elevated step (the app's `main` calls it for [`FLAG`]):
/// read the request, carry it out, write the response. Exit code 0 when a response
/// was written.
#[must_use]
pub fn run(argv: &[String]) -> i32 {
    let Some((req_path, resp_path)) = args(argv) else {
        return 2;
    };
    let resolve = {
        use nfd_platform::Platform;
        let native = nfd_platform::Native;
        move |v: &str| native.resolver()(v)
    };
    let env = |k: &str| std::env::var(k).ok();
    let gate = SafetyGate::new(&resolve);
    let roots = Roots::detect(&resolve, &env);
    let request = std::fs::read_to_string(&req_path)
        .ok()
        .and_then(|t| serde_json::from_str::<Request>(&t).ok());
    let refused = |why: &str| Response {
        protocol: PROTOCOL,
        error: Some(why.to_owned()),
        ..Response::default()
    };
    let response = match request {
        None => refused("unreadable request"),
        Some(_) if !reg::is_elevated() => refused("not elevated"),
        Some(req) => execute(&req, &gate, &roots),
    };
    match serde_json::to_vec_pretty(&response) {
        Ok(bytes) if std::fs::write(&resp_path, &bytes).is_ok() => 0,
        _ => 1,
    }
}

// Windows paths, registry layouts and Store packages: the tests are Windows-only
// like the Programs view itself (hidden on macOS, docs/MACOS.md §4).
#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Roots, SafetyGate) {
        let dir = tempfile::tempdir().expect("tmp");
        let b = dir.path().to_path_buf();
        for d in [
            "pf",
            "pd/Microsoft/Windows/Start Menu/Programs",
            "home/AppData/Roaming",
            "win",
            "public/Desktop",
        ] {
            std::fs::create_dir_all(b.join(d)).expect("mkdir");
        }
        let resolve = |v: &str| -> Option<String> {
            let p = |s: &str| Some(b.join(s).to_string_lossy().into_owned());
            match v {
                "PROGRAMFILES" => p("pf"),
                "PROGRAMDATA" => p("pd"),
                "SYSTEMROOT" => p("win"),
                "APPDATA" => p("home/AppData/Roaming"),
                "HOME" => p("home"),
                _ => None,
            }
        };
        let env =
            |v: &str| (v == "PUBLIC").then(|| b.join("public").to_string_lossy().into_owned());
        let roots = Roots::detect(&resolve, &env);
        let gate = SafetyGate::new(&resolve);
        (dir, roots, gate)
    }

    fn job(path: PathBuf) -> FileJob {
        FileJob {
            path,
            is_dir: true,
            bytes: 0,
            files: 0,
            size: 0,
            mtime: 0,
            reason: "same-name".into(),
        }
    }

    #[test]
    fn only_all_users_locations_are_accepted() {
        let (dir, roots, gate) = fixture();
        let b = dir.path();
        assert_eq!(refuse_file(&job(b.join("pf/Acme")), &gate, &roots), None);
        assert_eq!(refuse_file(&job(b.join("pd/Acme")), &gate, &roots), None);
        assert_eq!(
            refuse_file(
                &job(b.join("pd/Microsoft/Windows/Start Menu/Programs/Acme")),
                &gate,
                &roots
            ),
            None
        );
        assert_eq!(
            refuse_file(&job(b.join("public/Desktop/Acme.lnk")), &gate, &roots),
            None
        );
        assert_eq!(
            refuse_file(&job(b.join("pf")), &gate, &roots).as_deref(),
            Some("not-all-users")
        );
        assert_eq!(
            refuse_file(&job(b.join("home/AppData/Roaming/Acme")), &gate, &roots).as_deref(),
            Some("not-all-users")
        );
        assert_eq!(
            refuse_file(&job(b.join("pf/Common Files")), &gate, &roots).as_deref(),
            Some("protected")
        );
        assert_eq!(
            refuse_file(&job(b.join("pd/Microsoft/Crypto")), &gate, &roots).as_deref(),
            Some("protected")
        );
        assert_eq!(
            refuse_file(&job(b.join("win/Acme")), &gate, &roots).as_deref(),
            Some("not-all-users")
        );
    }

    #[test]
    fn registry_jobs_must_be_all_users_leftover_keys() {
        let ok = RegJob {
            hive: "HKLM".into(),
            key: r"SOFTWARE\Acme\Tool".into(),
            value: None,
            backup: PathBuf::from(r"C:\b\1.reg"),
            reason: "software-key".into(),
        };
        assert_eq!(refuse_registry(&ok), None);
        assert_eq!(
            refuse_registry(&RegJob {
                hive: "HKCU".into(),
                ..ok.clone()
            }),
            Some("not-all-users")
        );
        assert_eq!(
            refuse_registry(&RegJob {
                key: r"SOFTWARE\Microsoft\Cryptography".into(),
                ..ok.clone()
            }),
            Some("registry-protected")
        );
        assert_eq!(
            refuse_registry(&RegJob {
                key: r"SYSTEM\CurrentControlSet".into(),
                ..ok.clone()
            }),
            Some("registry-protected")
        );
        assert_eq!(
            refuse_registry(&RegJob {
                backup: PathBuf::from(r"C:\b\1.bat"),
                ..ok
            }),
            Some("registry-protected")
        );
    }

    #[test]
    fn reg_files_hold_exactly_what_was_removed() {
        let file = |sections: &[(&str, &[&str])]| {
            let mut t = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
            for (k, values) in sections {
                t.push_str(&format!("[{k}]\r\n"));
                for v in *values {
                    t.push_str(&format!("\"{v}\"=\"y\"\r\n"));
                }
                t.push_str("\r\n");
            }
            reg::utf16_file(&t)
        };
        let acme = r"HKEY_LOCAL_MACHINE\SOFTWARE\Acme\Tool";
        let run = r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
        // a key with its subkeys
        let ok = file(&[(acme, &["x"]), (&format!(r"{acme}\Sub\Deep"), &["y"])]);
        assert_eq!(refuse_reg_file(&ok, acme, true), None);
        // one value of a Run key
        assert_eq!(
            refuse_reg_file(
                &file(&[(run, &["AcmeTray"])]),
                &format!(r"{run}\AcmeTray"),
                false
            ),
            None
        );
        // a tampered backup: another key, another value, more values
        let extra = file(&[
            (acme, &["x"]),
            (r"HKEY_LOCAL_MACHINE\SOFTWARE\Other", &["z"]),
        ]);
        assert_eq!(refuse_reg_file(&extra, acme, true), Some("unexpected-key"));
        assert_eq!(
            refuse_reg_file(
                &file(&[(run, &["Evil"])]),
                &format!(r"{run}\AcmeTray"),
                false
            ),
            Some("unexpected-value")
        );
        assert_eq!(
            refuse_reg_file(
                &file(&[(run, &["AcmeTray", "Evil"])]),
                &format!(r"{run}\AcmeTray"),
                false
            ),
            Some("unexpected-value")
        );
        assert_eq!(
            refuse_reg_file(&file(&[(run, &["AcmeTray"])]), run, true),
            Some("registry-protected")
        );
        let winlogon = r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon";
        assert_eq!(
            refuse_reg_file(&file(&[(winlogon, &["Shell"])]), winlogon, true),
            Some("registry-protected")
        );
        let user = r"HKEY_CURRENT_USER\Software\Acme";
        assert_eq!(
            refuse_reg_file(&file(&[(user, &["x"])]), user, true),
            Some("not-all-users")
        );
        assert_eq!(
            refuse_reg_file(b"echo hi", acme, true),
            Some("not-a-backup")
        );
    }

    #[test]
    fn a_stale_request_is_refused_whole() {
        let (_dir, roots, gate) = fixture();
        let mut req = Request::new("op");
        req.protocol = 0;
        let resp = execute(&req, &gate, &roots);
        assert!(resp.error.is_some() && resp.items.is_empty());
    }

    #[test]
    fn refused_items_are_never_touched() {
        let (dir, roots, gate) = fixture();
        let user = dir.path().join("home/AppData/Roaming/Acme");
        std::fs::create_dir_all(&user).expect("mkdir");
        let mut req = Request::new("op");
        req.files.push(job(user.clone()));
        req.registry.push(RegJob {
            hive: "HKLM".into(),
            key: r"SOFTWARE\Microsoft\Windows".into(),
            value: None,
            backup: dir.path().join("1.reg"),
            reason: "software-key".into(),
        });
        let resp = execute(&req, &gate, &roots);
        assert_eq!(resp.items.len(), 2);
        assert!(
            resp.items
                .iter()
                .all(|i| i.status == ItemStatus::Skipped || i.status == ItemStatus::NotSelected),
            "{:?}",
            resp.items
        );
        assert!(
            user.is_dir(),
            "a user folder is never removed by the elevated step"
        );
        assert_eq!(resp.items[1].reason.as_deref(), Some("registry-protected"));
    }

    #[test]
    fn arguments() {
        let a = |v: &[&str]| args(&v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert!(a(&["app.exe", FLAG, r"C:\a\req.json", r"C:\a\resp.json"]).is_some());
        assert!(a(&["app.exe", FLAG, "req.json", r"C:\a\resp.json"]).is_none());
        assert!(a(&["app.exe", FLAG, r"C:\a\req.txt", r"C:\a\resp.json"]).is_none());
        assert!(a(&["app.exe", FLAG]).is_none());
    }
}
