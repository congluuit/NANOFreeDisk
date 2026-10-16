//! The SafetyGate (PLAN §3.4, PHASE3_CLEAN §2.1): the one check every destructive
//! action goes through, between the Planner and the Executor.
//!
//! Two layers:
//!
//! * [`SafetyGate::check_path`] is **lexical**: it compares a path with the protected
//!   templates resolved on this machine. It never touches the filesystem, so it can be
//!   tested exhaustively (`tests/safety_gate.rs`, property tests).
//! * [`SafetyGate::check_live`] runs right before an item is deleted. Through a
//!   [`Probe`] it looks at the real filesystem: the item must still exist and not be a
//!   link; its *final* path (links among the parents followed, 8.3 short names made
//!   long, case and `\\?\` prefixes normalized — `GetFinalPathNameByHandleW`) must pass
//!   the lexical check too; it must sit on the volume its path names; SYSTEM files in
//!   Windows/program folders, program images inside the Windows folder and cloud
//!   placeholders are refused; and it must not have changed since it was planned.
//!   Processes holding it open (Restart Manager) and apps a rule wants closed turn
//!   into [`Warning`]s — the application never closes them itself.
//!
//! Where the original design asked for an Authenticode check on files inside system
//! folders, the gate is stricter instead: *every* program image (`.exe`, `.dll`,
//! `.sys`, …) below `%SystemRoot%` is refused, signed or not.

use std::io;
use std::path::{Component, Path, PathBuf, Prefix};

use serde::{Deserialize, Serialize};

use super::{
    Blocked, CONTAINER_TEMPLATES, PROTECTED_TEMPLATES, USER_DATA_TEMPLATES, VOLUME_ROOT_PROTECTED,
    comparable, expand_template, is_within,
};
use crate::RiskLevel;
use crate::plan::CleanPlan;

/// Where a deletion request comes from. Rules and the user's own picks are held to
/// different standards: a rule may clean `%SystemRoot%\Temp` but never touches
/// Documents; the user may pick a file in Documents but nothing in `%SystemRoot%`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Origin {
    /// Picked by hand (Analyze, Old & Large, Tidy up).
    Manual,
    /// Proposed by a rule of the given level.
    Rule { level: RiskLevel },
}

/// Why an allowed item still deserves the user's attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "names", rename_all = "kebab-case")]
pub enum Warning {
    /// 🟡 rule: deleting loses history or forces a re-download.
    Caution,
    /// 🟠 rule: may break repair or features (Expert only).
    Advanced,
    /// Apps the rule wants closed are running (exe names).
    AppRunning(Vec<String>),
    /// Processes hold the item (or files inside it) open (Restart Manager names).
    InUse(Vec<String>),
    /// A cloud placeholder picked by hand: deleting it may delete the cloud copy.
    CloudPlaceholder,
}

/// The gate's answer for one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    Warn(Vec<Warning>),
    Blocked(Blocked),
}

impl Verdict {
    /// `true` unless blocked.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        !matches!(self, Self::Blocked(_))
    }

    fn from_warnings(warnings: Vec<Warning>) -> Self {
        if warnings.is_empty() {
            Self::Allowed
        } else {
            Self::Warn(warnings)
        }
    }
}

/// Verdicts for a whole [`CleanPlan`], index-aligned with `plan.items`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateReport {
    pub verdicts: Vec<Verdict>,
    /// Bytes of the items that are not blocked.
    pub allowed_bytes: u64,
}

/// What the gate needs to know about an entry on disk (never following links).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    pub is_dir: bool,
    /// Symlink, junction or mount point.
    pub link: bool,
    /// SYSTEM attribute.
    pub system: bool,
    /// Cloud placeholder / offline data.
    pub cloud: bool,
    /// Logical size (files).
    pub size: u64,
    /// Last write, Unix seconds.
    pub mtime: i64,
    pub volume_id: u64,
}

/// What the plan remembered about an item; anything different on disk means the
/// item changed since and it is skipped ([`Blocked::Changed`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expected {
    pub is_dir: bool,
    /// Compared for files only (a folder's size changes as programs use it).
    pub size: u64,
    /// Compared for files only.
    pub mtime: i64,
}

/// The filesystem as the gate sees it. [`LiveProbe`] is the real one; tests use fakes.
pub trait Probe {
    /// Metadata of the entry itself (links not followed).
    ///
    /// # Errors
    /// The entry is gone or cannot be read.
    fn facts(&self, path: &Path) -> io::Result<Facts>;
    /// The final path, every link followed and every name in its long form.
    ///
    /// # Errors
    /// The path cannot be opened.
    fn final_path(&self, path: &Path) -> io::Result<PathBuf>;
    /// Volume identifier of the volume root `root` (`C:\`), or `None` when the
    /// check does not apply on this platform.
    fn root_volume(&self, root: &Path) -> Option<u64>;
    /// Names of processes holding the item (or a sample of the files inside a folder)
    /// open.
    fn lockers(&self, path: &Path, is_dir: bool) -> Vec<String>;
    /// Lower-case exe names of running processes.
    fn running(&self) -> Vec<String>;
}

/// Program images refused anywhere below `%SystemRoot%`.
const SYSTEM_IMAGE_EXTS: &[&str] = &[
    "exe", "dll", "sys", "drv", "ocx", "cpl", "scr", "efi", "mui", "cat", "msi", "msp", "com",
    "winmd", "manifest",
];

/// The only places below `%SystemRoot%` a rule may clean (relative, lower case).
/// `true` = the entry itself may go (a file); `false` = only items strictly inside.
const SYSTEMROOT_RULE_ALLOW: &[(&str, bool)] = &[
    ("temp", false),
    ("logs/cbs", false),
    ("logs/dism", false),
    ("minidump", false),
    ("livekernelreports", false),
    ("softwaredistribution/download", false),
    ("memory.dmp", true),
];

/// Reserved DOS device names; a path component named like one (with any extension)
/// refers to a device, not a file.
const DEVICE_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "conin$", "conout$", "com0", "com1", "com2", "com3", "com4",
    "com5", "com6", "com7", "com8", "com9", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6",
    "lpt7", "lpt8", "lpt9", "com¹", "com²", "com³", "lpt¹", "lpt²", "lpt³",
];

/// The protected templates of [`super`], resolved once for this machine.
#[derive(Debug, Clone)]
pub struct SafetyGate {
    system_root: Option<String>,
    program_data: Option<String>,
    /// Folders where SYSTEM-attributed entries are refused.
    system_dirs: Vec<String>,
    /// Protected templates except the user-data ones: (template, comparable path).
    subtree: Vec<(&'static str, String)>,
    user_data: Vec<(&'static str, String)>,
    containers: Vec<(&'static str, String)>,
}

impl SafetyGate {
    /// Resolve the templates with `resolve` (`nfd_platform::Platform::resolver`).
    /// Variables that do not resolve are skipped.
    #[must_use]
    pub fn new(resolve: &impl Fn(&str) -> Option<String>) -> Self {
        let resolved = |list: &[&'static str], skip: &[&str]| -> Vec<(&'static str, String)> {
            list.iter()
                .filter(|t| !skip.contains(t))
                .filter_map(|t| expand_template(t, resolve).map(|p| (*t, p)))
                .collect()
        };
        let system_root = expand_template("${SYSTEMROOT}", resolve);
        let system_dirs = ["${SYSTEMROOT}", "${PROGRAMFILES}", "${PROGRAMFILES_X86}"]
            .iter()
            .filter_map(|t| expand_template(t, resolve))
            .collect();
        Self {
            program_data: expand_template("${PROGRAMDATA}", resolve),
            system_root,
            system_dirs,
            subtree: resolved(PROTECTED_TEMPLATES, USER_DATA_TEMPLATES),
            user_data: resolved(USER_DATA_TEMPLATES, &[]),
            containers: resolved(CONTAINER_TEMPLATES, &[]),
        }
    }

    /// Lexical check of `path` for a request from `origin` (no filesystem access).
    ///
    /// # Errors
    /// Why the path must not be deleted.
    pub fn check_path(&self, path: &Path, origin: Origin) -> Result<(), Blocked> {
        lexical_traps(path)?;
        if matches!(
            origin,
            Origin::Rule {
                level: RiskLevel::Protected
            }
        ) {
            return Err(Blocked::ProtectedLevel);
        }
        let Some(cand) = comparable(path) else {
            return Err(Blocked::Unresolvable(path.display().to_string()));
        };
        if cand.starts_with("//./") {
            return Err(Blocked::Device);
        }
        let parts: Vec<&str> = cand.split('/').filter(|p| !p.is_empty()).collect();
        // `c:` is one component, `//server/share` two; a Unix `/x` has none (the
        // tests map `C:\x` to `/x`, so the same rules apply one level up).
        let volume_depth = if cand.starts_with("//") {
            2
        } else if cand.starts_with('/') {
            0
        } else {
            1
        };
        if parts.len() <= volume_depth {
            return Err(Blocked::VolumeRoot);
        }
        let in_system_root = self
            .system_root
            .as_deref()
            .is_some_and(|root| is_within(&cand, root));
        let rule = matches!(origin, Origin::Rule { .. });
        if in_system_root {
            if !rule {
                return Err(Blocked::SystemDir);
            }
            self.check_system_root_rule(&cand)?;
        }
        let in_program_data = self
            .program_data
            .as_deref()
            .is_some_and(|root| is_within(&cand, root) && cand != root);
        let top = parts[volume_depth];
        // A rule may reach into %SystemRoot% and %ProgramData% (the templates below
        // still protect what matters there); anything else named like a Windows
        // folder at the root of any volume is off limits.
        let exempt = rule
            && ((in_system_root && top == "windows") || (in_program_data && top == "programdata"));
        if VOLUME_ROOT_PROTECTED.contains(&top) && !exempt {
            return Err(Blocked::Protected {
                template: format!("<volume>/{top}"),
            });
        }
        if top == "users" && parts.len() == volume_depth + 1 {
            return Err(Blocked::ContainsProtected {
                template: "<volume>/Users".to_owned(),
            });
        }

        for (template, root) in &self.subtree {
            if is_within(&cand, root) {
                return Err(Blocked::Protected {
                    template: (*template).to_owned(),
                });
            }
            if is_within(root, &cand) {
                return Err(Blocked::ContainsProtected {
                    template: (*template).to_owned(),
                });
            }
        }
        for (template, root) in &self.user_data {
            // Rules never touch user data; the user may pick items inside it.
            if rule && is_within(&cand, root) {
                return Err(Blocked::Protected {
                    template: (*template).to_owned(),
                });
            }
            if is_within(root, &cand) {
                return Err(Blocked::ContainsProtected {
                    template: (*template).to_owned(),
                });
            }
        }
        for (template, root) in &self.containers {
            if is_within(root, &cand) {
                return Err(Blocked::ContainsProtected {
                    template: (*template).to_owned(),
                });
            }
        }
        Ok(())
    }

    fn check_system_root_rule(&self, cand: &str) -> Result<(), Blocked> {
        let Some(root) = self.system_root.as_deref() else {
            return Err(Blocked::SystemDir);
        };
        let rel = cand[root.len()..].trim_start_matches('/');
        let ok = SYSTEMROOT_RULE_ALLOW.iter().any(|(allowed, self_ok)| {
            if *self_ok {
                rel == *allowed
            } else {
                rel.len() > allowed.len()
                    && rel.starts_with(allowed)
                    && rel.as_bytes()[allowed.len()] == b'/'
            }
        });
        if ok { Ok(()) } else { Err(Blocked::SystemDir) }
    }

    /// Full check right before deleting `path`: [`Self::check_path`] on the path as
    /// given **and** on its final form, then the on-disk checks described in the
    /// module documentation. `expected` is what the plan saw (`None` = no
    /// comparison). Returns the warnings of an allowed item.
    ///
    /// # Errors
    /// Why the item must not be deleted now.
    pub fn check_live(
        &self,
        path: &Path,
        origin: Origin,
        expected: Option<Expected>,
        probe: &dyn Probe,
    ) -> Result<Vec<Warning>, Blocked> {
        self.check_path(path, origin)?;
        let facts = probe
            .facts(path)
            .map_err(|e| Blocked::Unresolvable(e.to_string()))?;
        if facts.link {
            return Err(Blocked::Link);
        }
        let real = probe
            .final_path(path)
            .map_err(|e| Blocked::Unresolvable(e.to_string()))?;
        self.check_path(&real, origin)?;
        if let Some(root) = volume_root(path) {
            if probe
                .root_volume(&root)
                .is_some_and(|v| v != facts.volume_id)
            {
                return Err(Blocked::CrossVolume);
            }
        }
        let real_cmp = comparable(&real).unwrap_or_default();
        let rule = matches!(origin, Origin::Rule { .. });
        if facts.system && (rule || self.system_dirs.iter().any(|d| is_within(&real_cmp, d))) {
            return Err(Blocked::SystemFile);
        }
        if !facts.is_dir
            && self
                .system_root
                .as_deref()
                .is_some_and(|r| is_within(&real_cmp, r))
            && has_ext(&real_cmp, SYSTEM_IMAGE_EXTS)
        {
            return Err(Blocked::SystemBinary);
        }
        let mut warnings = Vec::new();
        if facts.cloud {
            if rule {
                return Err(Blocked::CloudFile);
            }
            warnings.push(Warning::CloudPlaceholder);
        }
        if let Some(exp) = expected {
            let changed = exp.is_dir != facts.is_dir
                || (!facts.is_dir && (exp.size != facts.size || exp.mtime != facts.mtime));
            if changed {
                return Err(Blocked::Changed);
            }
        }
        match origin {
            Origin::Rule {
                level: RiskLevel::Caution,
            } => warnings.push(Warning::Caution),
            Origin::Rule {
                level: RiskLevel::Advanced,
            } => warnings.push(Warning::Advanced),
            _ => {}
        }
        let lockers = probe.lockers(path, facts.is_dir);
        if !lockers.is_empty() {
            warnings.push(Warning::InUse(lockers));
        }
        Ok(warnings)
    }

    /// Check every item of `plan` against the live filesystem: nested items are
    /// refused ([`Blocked::Nested`]) and rules' `requires_closed` apps that are
    /// running become [`Warning::AppRunning`].
    #[must_use]
    pub fn check(&self, plan: &CleanPlan, probe: &dyn Probe) -> GateReport {
        let keys: Vec<Option<String>> = plan.items.iter().map(|i| comparable(&i.path)).collect();
        let needs_running = plan.items.iter().any(|i| !i.requires_closed.is_empty());
        let running = if needs_running {
            probe.running()
        } else {
            Vec::new()
        };
        let mut allowed_bytes = 0;
        let verdicts = plan
            .items
            .iter()
            .enumerate()
            .map(|(n, item)| {
                let nested = keys[n].as_deref().is_some_and(|me| {
                    keys.iter().enumerate().any(|(m, other)| {
                        m != n
                            && other.as_deref().is_some_and(|o| {
                                (o != me && is_within(me, o)) || (o == me && m < n)
                            })
                    })
                });
                if nested {
                    return Verdict::Blocked(Blocked::Nested);
                }
                match self.check_live(&item.path, item.origin, Some(item.expected()), probe) {
                    Err(b) => Verdict::Blocked(b),
                    Ok(mut warnings) => {
                        let apps: Vec<String> = item
                            .requires_closed
                            .iter()
                            .filter(|exe| crate::plan::is_running(&running, exe))
                            .cloned()
                            .collect();
                        if !apps.is_empty() {
                            warnings.push(Warning::AppRunning(apps));
                        }
                        allowed_bytes += item.bytes;
                        Verdict::from_warnings(warnings)
                    }
                }
            })
            .collect();
        GateReport {
            verdicts,
            allowed_bytes,
        }
    }
}

/// Path tricks refused before anything else: alternate data streams and reserved
/// device names. (`..`, `.`, `\\?\`, case and trailing dots/spaces are normalized by
/// the comparable form instead.)
fn lexical_traps(path: &Path) -> Result<(), Blocked> {
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => {
                if matches!(p.kind(), Prefix::DeviceNS(_)) {
                    return Err(Blocked::Device);
                }
            }
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                if cfg!(windows) && name.contains(':') {
                    return Err(Blocked::Stream);
                }
                if cfg!(windows) {
                    let stem = name
                        .split('.')
                        .next()
                        .unwrap_or_default()
                        .trim_end_matches(' ')
                        .to_lowercase();
                    if DEVICE_NAMES.contains(&stem.as_str()) {
                        return Err(Blocked::Device);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `C:\` / `\\server\share\` for an absolute Windows path, `None` otherwise.
fn volume_root(path: &Path) -> Option<PathBuf> {
    let mut comps = path.components();
    match comps.next()? {
        Component::Prefix(p) => {
            let mut root = PathBuf::from(p.as_os_str());
            root.push(std::path::MAIN_SEPARATOR_STR);
            Some(root)
        }
        _ => None,
    }
}

fn has_ext(cmp: &str, exts: &[&str]) -> bool {
    let name = cmp.rsplit('/').next().unwrap_or(cmp);
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| exts.contains(&ext))
}

/// The real filesystem (`nfd_platform`).
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveProbe;

impl Probe for LiveProbe {
    fn facts(&self, path: &Path) -> io::Result<Facts> {
        use nfd_platform::fsinfo::{FileFlags, LinkKind, file_info};
        let info = file_info(path)?;
        Ok(Facts {
            is_dir: info.is_dir,
            link: info.link != LinkKind::None,
            system: info.flags.contains(FileFlags::SYSTEM),
            cloud: info.flags.contains(FileFlags::OFFLINE),
            size: info.logical,
            mtime: info.mtime,
            volume_id: info.volume_id,
        })
    }

    fn final_path(&self, path: &Path) -> io::Result<PathBuf> {
        // `canonicalize` is `GetFinalPathNameByHandleW` on Windows (the item is
        // opened without any access right, so nothing is locked or hydrated).
        std::fs::canonicalize(path)
    }

    fn root_volume(&self, root: &Path) -> Option<u64> {
        if cfg!(windows) {
            nfd_platform::fsinfo::file_info(root)
                .ok()
                .map(|i| i.volume_id)
        } else {
            None
        }
    }

    fn lockers(&self, path: &Path, is_dir: bool) -> Vec<String> {
        let files: Vec<PathBuf> = if is_dir {
            sample_files(path, 256)
        } else {
            vec![path.to_path_buf()]
        };
        let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
        let mut names: Vec<String> = nfd_platform::locks::lockers(&refs)
            .unwrap_or_default()
            .into_iter()
            .filter(|l| l.pid != std::process::id())
            .map(|l| l.name)
            .collect();
        names.sort();
        names.dedup();
        names
    }

    fn running(&self) -> Vec<String> {
        nfd_platform::locks::running_processes().unwrap_or_default()
    }
}

/// Up to `limit` files below `dir`, breadth first, never following links.
fn sample_files(dir: &Path, limit: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut queue = std::collections::VecDeque::from([dir.to_path_buf()]);
    while let Some(d) = queue.pop_front() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                queue.push_back(entry.path());
            } else {
                out.push(entry.path());
                if out.len() >= limit {
                    return out;
                }
            }
        }
    }
    out
}
