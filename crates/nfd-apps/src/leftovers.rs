//! What an uninstalled program left behind (`docs/UNINSTALLER.md` §2.4).
//!
//! A [`Snapshot`] is taken *before* the vendor's uninstaller runs (where the program
//! lived, its exe files, the names its folders and keys may carry); [`scan`] then
//! looks for what is still there: the install folder, folders named like the
//! program in Program Files / ProgramData / AppData, shortcuts whose target is gone,
//! and registry keys and values that point at it.
//!
//! Every item carries the evidence that ties it to the program (`reason`) and a
//! level: 🟢 Safe items are pre-selected, 🟡 Caution ones (a weak name match, a
//! vendor folder, a name another installed program shares) are listed but never
//! pre-selected. Folders that hold or sit inside another program, that a running
//! process uses, that hold an AI agent's conversation history, or that the
//! SafetyGate refuses are listed as blocked.

use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::safety::{Origin, SafetyGate};
use serde::{Deserialize, Serialize};

use crate::command;
use crate::fsutil;
use crate::inventory::{Program, norm, same, within};
use crate::names;
use crate::registry::{Hive, Registry, text};
use crate::{Scope, Source};

/// `…\CurrentVersion\App Paths` (64-bit view; `WOW6432Node` for 32-bit).
pub const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
const APP_PATHS_WOW: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths";
/// `…\CurrentVersion\Run`.
pub const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_WOW: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
/// Task Manager's on/off switch of a Run entry.
const STARTUP_APPROVED: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const STARTUP_APPROVED_32: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";
const SERVICES: &str = r"SYSTEM\CurrentControlSet\Services";

/// Leftover folders this big are never pre-selected: an uninstaller that leaves
/// half a gigabyte behind usually left the user's own files (saves, libraries,
/// chat history, downloads).
pub const LARGE_LEFTOVER: u64 = 500 << 20;

/// Keys directly under `Software` that are never a program's leftover.
pub const SOFTWARE_DENY: &[&str] = &[
    "microsoft",
    "classes",
    "policies",
    "wow6432node",
    "clients",
    "registeredapplications",
    "odbc",
    "windows",
    "description",
    "intel",
    "partner",
    "appdatalow",
];

/// Where leftovers are looked for.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    /// `Program Files`, `Program Files (x86)` (all users).
    pub program_files: Vec<PathBuf>,
    /// `ProgramData` (all users).
    pub program_data: Option<PathBuf>,
    /// `AppData\Roaming`, `AppData\Local`, `AppData\LocalLow`, `AppData\Local\Programs`.
    pub app_data: Vec<PathBuf>,
    /// Start Menu `Programs` folders.
    pub start_menus: Vec<(PathBuf, Scope)>,
    /// Desktops (this user's, the public one).
    pub desktops: Vec<(PathBuf, Scope)>,
    /// Quick Launch (and pinned taskbar shortcuts below it).
    pub quick_launch: Vec<PathBuf>,
    /// `AppData\Local\Packages` (Store apps' data).
    pub packages: Option<PathBuf>,
}

impl Roots {
    /// The real folders. `resolve` maps `PROGRAMFILES`, `PROGRAMFILES_X86`,
    /// `PROGRAMDATA`, `APPDATA`, `LOCALAPPDATA`, `HOME`; `env` reads `PUBLIC`.
    #[must_use]
    pub fn detect(
        resolve: &dyn Fn(&str) -> Option<String>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        let p = |v: &str| resolve(v).map(|s| crate::inventory::native_path(&s));
        let mut r = Self {
            program_files: ["PROGRAMFILES", "PROGRAMFILES_X86"]
                .iter()
                .filter_map(|v| p(v))
                .collect(),
            program_data: p("PROGRAMDATA"),
            ..Self::default()
        };
        r.program_files.dedup_by(|a, b| same(a, b));
        if let Some(a) = p("APPDATA") {
            r.start_menus.push((
                a.join(r"Microsoft\Windows\Start Menu\Programs"),
                Scope::User,
            ));
            r.quick_launch
                .push(a.join(r"Microsoft\Internet Explorer\Quick Launch"));
            r.app_data.push(a);
        }
        if let Some(l) = p("LOCALAPPDATA") {
            r.app_data.push(l.join("Programs"));
            r.packages = Some(l.join("Packages"));
            r.app_data.push(l);
        }
        if let Some(h) = p("HOME") {
            r.app_data.push(h.join(r"AppData\LocalLow"));
            r.desktops.push((h.join("Desktop"), Scope::User));
        }
        if let Some(pd) = &r.program_data {
            r.start_menus.push((
                pd.join(r"Microsoft\Windows\Start Menu\Programs"),
                Scope::Machine,
            ));
        }
        if let Some(public) = env("PUBLIC") {
            r.desktops
                .push((PathBuf::from(public).join("Desktop"), Scope::Machine));
        }
        r
    }

    /// Folders searched for folders named like the program, with their scope.
    fn folder_roots(&self) -> Vec<(PathBuf, Scope)> {
        let mut out: Vec<(PathBuf, Scope)> = self
            .program_files
            .iter()
            .map(|p| (p.clone(), Scope::Machine))
            .collect();
        out.extend(
            self.program_data
                .iter()
                .map(|p| (p.clone(), Scope::Machine)),
        );
        out.extend(self.app_data.iter().map(|p| (p.clone(), Scope::User)));
        out
    }

    /// Every root (never removed whole, nor any folder holding one).
    #[must_use]
    pub fn all(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.folder_roots().into_iter().map(|(p, _)| p).collect();
        out.extend(self.start_menus.iter().map(|(p, _)| p.clone()));
        out.extend(self.desktops.iter().map(|(p, _)| p.clone()));
        out.extend(self.quick_launch.iter().cloned());
        out.extend(self.packages.iter().cloned());
        out
    }

    /// All-users or this user's.
    #[must_use]
    pub fn scope_of(&self, path: &Path) -> Scope {
        let machine = self
            .program_files
            .iter()
            .chain(self.program_data.iter())
            .any(|r| within(path, r))
            || self
                .start_menus
                .iter()
                .chain(self.desktops.iter())
                .any(|(r, s)| *s == Scope::Machine && within(path, r));
        if machine { Scope::Machine } else { Scope::User }
    }
}

/// What is known about a program right before its uninstaller runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub program: Program,
    /// Names its folders and keys may carry ([`names::program_keys`]).
    pub keys: Vec<String>,
    /// Its publisher, made comparable (vendor folders and keys).
    pub publisher: Option<String>,
    /// Folders it lived in.
    pub homes: Vec<PathBuf>,
    /// Its exe files (install folder, two levels).
    pub exes: Vec<PathBuf>,
    pub taken_at: i64,
}

/// Exe files of `dir` and its sub-folders (two levels), at most `max`.
fn exes_in(dir: &Path, max: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((d, depth)) = stack.pop() {
        for (name, is_dir) in fsutil::children(&d) {
            if out.len() >= max {
                return out;
            }
            if is_dir {
                if depth < 1 {
                    stack.push((d.join(&name), depth + 1));
                }
            } else if name.to_ascii_lowercase().ends_with(".exe") {
                out.push(d.join(&name));
            }
        }
    }
    out
}

/// `path` without trailing separators (`C:\` stays).
#[must_use]
pub fn clean(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let t = s.trim_end_matches(['\\', '/']);
    if t.len() < 3 || t.ends_with(':') {
        path.to_path_buf()
    } else {
        PathBuf::from(t)
    }
}

/// A folder that can be a program's own: absolute, outside the Windows folder,
/// neither a root nor holding one, below a volume's top level.
fn usable_home(h: &Path, roots: &[PathBuf], system_root: &Path) -> bool {
    let n = norm(h);
    h.is_absolute()
        && !within(h, system_root)
        && !roots.iter().any(|r| within(r, h))
        && n.trim_start_matches('/')
            .split('/')
            .filter(|c| !c.is_empty())
            .count()
            >= 2
}

/// Take the snapshot of `program` (reads its install folder for exe files).
#[must_use]
pub fn snapshot(program: &Program, roots: &Roots, system_root: &Path, now: i64) -> Snapshot {
    let all_roots = roots.all();
    let homes: Vec<PathBuf> = program
        .homes()
        .iter()
        .filter(|h| usable_home(h, &all_roots, system_root))
        .map(|h| clean(h))
        .collect();
    let mut exes: Vec<PathBuf> = Vec::new();
    for h in &homes {
        for e in exes_in(h, 400) {
            if !exes.iter().any(|x| same(x, &e)) {
                exes.push(e);
            }
        }
    }
    if let Some(icon) = &program.icon {
        if icon
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            && !exes.iter().any(|x| same(x, icon))
        {
            exes.push(icon.clone());
        }
    }
    let stems: Vec<String> = exes
        .iter()
        .filter(|e| {
            // main programs only: the install folder itself, not helpers below
            e.parent().is_some_and(|p| homes.iter().any(|h| same(p, h)))
        })
        .filter_map(|e| e.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    let install_name = program
        .install_location
        .as_deref()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned());
    let mut keys = names::program_keys(
        &program.name,
        program.publisher.as_deref(),
        install_name.as_deref(),
        &stems,
    );
    if let Some(pkg) = &program.package {
        let n = names::normalize(&crate::inventory::pretty_package_name(
            pkg.family.split('_').next().unwrap_or_default(),
        ));
        if names::distinctive(&n) && !keys.contains(&n) {
            keys.push(n);
        }
    }
    Snapshot {
        publisher: program
            .publisher
            .as_deref()
            .map(names::publisher_key)
            .filter(|p| names::distinctive(p)),
        program: program.clone(),
        keys,
        homes,
        exes,
        taken_at: now,
    }
}

/// The other installed programs (a leftover must not belong to one of them).
#[derive(Debug, Clone, Default)]
pub struct Others {
    homes: Vec<(PathBuf, String)>,
    keys: Vec<(String, String)>,
    publishers: Vec<String>,
}

impl Others {
    /// Every program of `programs` except the one with `except_id` (the list as it
    /// is *after* the uninstall).
    #[must_use]
    pub fn new(programs: &[Program], except_id: &str, roots: &Roots, system_root: &Path) -> Self {
        let all_roots = roots.all();
        let mut o = Self::default();
        for p in programs.iter().filter(|p| p.id != except_id) {
            for h in p.homes() {
                // a registration naming a whole root (`C:\Program Files`) says nothing
                if usable_home(&h, &all_roots, system_root) {
                    o.homes.push((clean(&h), p.name.clone()));
                }
            }
            let stems: Vec<String> = p
                .icon
                .as_deref()
                .and_then(Path::file_stem)
                .map(|s| vec![s.to_string_lossy().into_owned()])
                .unwrap_or_default();
            let dir = p
                .install_location
                .as_deref()
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned());
            for k in names::program_keys(&p.name, p.publisher.as_deref(), dir.as_deref(), &stems) {
                o.keys.push((k, p.name.clone()));
            }
            if let Some(pk) = p.publisher.as_deref().map(names::publisher_key) {
                if !pk.is_empty() && !o.publishers.contains(&pk) {
                    o.publishers.push(pk);
                }
            }
        }
        o
    }

    /// The other program whose folder `path` holds or lies in.
    fn owner_of(&self, path: &Path) -> Option<&str> {
        self.homes
            .iter()
            .find(|(h, _)| within(h, path) || within(path, h))
            .map(|(_, n)| n.as_str())
    }

    /// Another program a folder or key named `name` may belong to.
    fn named(&self, name: &str) -> Option<&str> {
        let n = names::normalize(name);
        let c = n.replace(' ', "");
        self.keys
            .iter()
            .find(|(k, _)| *k == n || *k == c)
            .map(|(_, p)| p.as_str())
    }

    fn shares_publisher(&self, publisher: &str) -> bool {
        self.publishers.iter().any(|p| p == publisher)
    }
}

/// What kind of thing a leftover is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Folder,
    File,
    Shortcut,
    RegistryKey,
    RegistryValue,
    /// A service still pointing at the program (shown with the command to remove
    /// it; never removed here).
    Service,
}

/// One leftover.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    /// Unique: the path, or `HKEY_…\key` (`|value` for a value).
    pub id: String,
    pub kind: Kind,
    /// The file path, or the registry key `HKEY_…\…`.
    pub path: String,
    /// A registry value's name.
    pub value: Option<String>,
    pub scope: Scope,
    pub level: RiskLevel,
    /// Why it is the program's: `install-dir`, `same-name`, `vendor-folder`,
    /// `weak-name`, `shortcut`, `start-menu-folder`, `arp-entry`, `software-key`,
    /// `vendor-key`, `app-path`, `run-entry`, `service`, `store-data`.
    pub reason: String,
    /// The shortcut's target, the Run command, the service's name…
    pub detail: Option<String>,
    /// Another installed program the name also fits (then never pre-selected).
    pub also: Option<String>,
    pub bytes: u64,
    pub files: u64,
    pub mtime: i64,
    pub selected: bool,
    /// Why it cannot be removed: a SafetyGate code, `other-program`, `in-use`,
    /// `contains-history`, `too-large`, `info` (services).
    pub blocked: Option<String>,
    /// Holds more than [`Ctx::large`]: what is left may be the user's (a game
    /// library, chat history, downloads), so it is never pre-selected.
    #[serde(default)]
    pub large: bool,
}

impl Leftover {
    /// The registry hive and key of a registry item.
    #[must_use]
    pub fn reg_key(&self) -> Option<(Hive, &str)> {
        matches!(self.kind, Kind::RegistryKey | Kind::RegistryValue)
            .then(|| Hive::split(&self.path))
            .flatten()
    }
}

/// Why the scan runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// The vendor's uninstaller ran and the program is no longer listed.
    AfterUninstall,
    /// The entry's uninstaller is gone: the program cannot uninstall itself, so
    /// its entry and files are what is left.
    Broken,
    /// The program still looks installed (Expert): everything 🟡, nothing
    /// pre-selected, its entry never offered.
    Forced,
}

/// Everything [`scan`] found.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub program: Program,
    pub mode: Mode,
    pub items: Vec<Leftover>,
    pub selected_bytes: u64,
    pub total_bytes: u64,
}

impl Report {
    /// Items pre-selected.
    #[must_use]
    pub fn preselected(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.id.clone())
            .collect()
    }
}

/// What [`scan`] reads besides the snapshot.
pub struct Ctx<'a> {
    pub reg: &'a dyn Registry,
    pub roots: &'a Roots,
    pub gate: &'a SafetyGate,
    pub others: &'a Others,
    /// Image paths of running processes.
    pub running: &'a [PathBuf],
    /// Entries visited per folder when sizing.
    pub max_entries: u64,
    /// A leftover folder at least this big is 🟡 and never pre-selected
    /// ([`LARGE_LEFTOVER`]).
    pub large: u64,
    pub exists: &'a dyn Fn(&Path) -> bool,
    pub env: &'a dyn Fn(&str) -> Option<String>,
    pub system_root: PathBuf,
}

impl std::fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx")
            .field("roots", &self.roots)
            .field("max_entries", &self.max_entries)
            .finish_non_exhaustive()
    }
}

struct Scan<'a, 'c> {
    snap: &'a Snapshot,
    ctx: &'a Ctx<'c>,
    mode: Mode,
    items: Vec<Leftover>,
}

impl Scan<'_, '_> {
    fn has(&self, id: &str) -> bool {
        let n = norm(Path::new(id));
        self.items.iter().any(|i| norm(Path::new(&i.id)) == n)
    }

    fn level(&self, level: RiskLevel) -> RiskLevel {
        if self.mode == Mode::Forced {
            RiskLevel::Caution
        } else {
            level
        }
    }

    /// A folder or file found on disk.
    fn push_path(
        &mut self,
        path: &Path,
        kind: Kind,
        reason: &str,
        level: RiskLevel,
        detail: Option<String>,
    ) {
        let path = &clean(path);
        let id = path.to_string_lossy().into_owned();
        if self.has(&id) || !(self.ctx.exists)(path) {
            return;
        }
        let roots = self.ctx.roots.all();
        if roots.iter().any(|r| within(r, path)) {
            return;
        }
        let mut level = self.level(level);
        let mut blocked = self
            .ctx
            .gate
            .check_path(path, Origin::Leftover)
            .err()
            .map(|b| b.code().to_owned());
        let mut also = None;
        if blocked.is_none() {
            if let Some(other) = self.ctx.others.owner_of(path) {
                blocked = Some("other-program".into());
                also = Some(other.to_owned());
            }
        }
        if matches!(
            reason,
            "same-name" | "weak-name" | "start-menu-folder" | "vendor-folder"
        ) {
            if let Some(other) = path
                .file_name()
                .and_then(|n| self.ctx.others.named(&n.to_string_lossy()))
            {
                level = RiskLevel::Caution;
                also.get_or_insert_with(|| other.to_owned());
            }
        }
        if blocked.is_none() && self.ctx.running.iter().any(|r| within(r, path)) {
            blocked = Some("in-use".into());
        }
        let t = fsutil::totals(path, self.ctx.max_entries);
        if blocked.is_none() && t.history.is_some() {
            blocked = Some("contains-history".into());
        }
        if blocked.is_none() && t.truncated {
            blocked = Some("too-large".into());
        }
        let large = t.bytes >= self.ctx.large;
        if large {
            level = RiskLevel::Caution;
        }
        let selected = blocked.is_none() && level == RiskLevel::Safe && also.is_none();
        self.items.push(Leftover {
            id,
            kind,
            path: path.to_string_lossy().into_owned(),
            value: None,
            scope: self.ctx.roots.scope_of(path),
            level,
            reason: reason.to_owned(),
            detail,
            also,
            bytes: t.bytes,
            files: t.files,
            mtime: if t.newest > 0 {
                t.newest
            } else {
                fsutil::mtime(path)
            },
            selected,
            blocked,
            large,
        });
    }

    /// A registry key (or one of its values).
    fn push_reg(
        &mut self,
        hive: Hive,
        key: &str,
        value: Option<&str>,
        reason: &str,
        level: RiskLevel,
        detail: Option<String>,
    ) {
        let path = format!(r"{}\{key}", hive.name());
        let id = match value {
            Some(v) => format!("{path}|{v}"),
            None => path.clone(),
        };
        if self.has(&id) {
            return;
        }
        let mut level = self.level(level);
        let mut also = None;
        if matches!(reason, "software-key" | "vendor-key") {
            if let Some(other) = key
                .rsplit('\\')
                .next()
                .and_then(|n| self.ctx.others.named(n))
            {
                level = RiskLevel::Caution;
                also = Some(other.to_owned());
            }
        }
        let blocked = registry_refusal(hive, key, value.is_some()).map(str::to_owned);
        let selected = blocked.is_none() && level == RiskLevel::Safe && also.is_none();
        self.items.push(Leftover {
            id,
            kind: if value.is_some() {
                Kind::RegistryValue
            } else {
                Kind::RegistryKey
            },
            path,
            value: value.map(str::to_owned),
            scope: if hive == Hive::LocalMachine {
                Scope::Machine
            } else {
                Scope::User
            },
            level,
            reason: reason.to_owned(),
            detail,
            also,
            bytes: 0,
            files: 0,
            mtime: self.ctx.reg.last_write(hive, key).unwrap_or(0),
            selected,
            blocked,
            large: false,
        });
    }

    fn homes_hold(&self, p: &Path) -> bool {
        self.snap.homes.iter().any(|h| within(p, h)) || self.snap.exes.iter().any(|e| same(e, p))
    }

    fn folders(&mut self) {
        let snap = self.snap;
        for h in &snap.homes {
            self.push_path(h, Kind::Folder, "install-dir", RiskLevel::Safe, None);
        }
        let publisher = snap.publisher.clone();
        for (root, _) in self.ctx.roots.folder_roots() {
            for (name, is_dir) in fsutil::children(&root) {
                if !is_dir {
                    continue;
                }
                let path = root.join(&name);
                // the vendor's folder first: a product may share its vendor's name
                // (`Acme\Acme`, an exe called `acme.exe`) and the vendor folder
                // holds other products
                let is_vendor = publisher.as_deref().is_some_and(|p| is_publisher(&name, p));
                if !is_vendor && names::matches(&name, &snap.keys) {
                    self.push_path(&path, Kind::Folder, "same-name", RiskLevel::Safe, None);
                    continue;
                }
                if is_vendor {
                    let kids = fsutil::children(&path);
                    let mut matched = 0;
                    for (kid, kid_dir) in &kids {
                        if *kid_dir && names::matches(kid, &snap.keys) {
                            self.push_path(
                                &path.join(kid),
                                Kind::Folder,
                                "same-name",
                                RiskLevel::Safe,
                                None,
                            );
                            matched += 1;
                        }
                    }
                    let shared = publisher
                        .as_deref()
                        .is_some_and(|p| self.ctx.others.shares_publisher(p));
                    if matched > 0 {
                        if matched == kids.len() && !shared {
                            self.push_path(
                                &path,
                                Kind::Folder,
                                "vendor-folder",
                                RiskLevel::Caution,
                                None,
                            );
                        }
                    } else if names::matches(&name, &snap.keys) {
                        // the product carries its vendor's name (`Discord`): its own
                        // folder, unless another program of the vendor is installed
                        let level = if shared {
                            RiskLevel::Caution
                        } else {
                            RiskLevel::Safe
                        };
                        self.push_path(&path, Kind::Folder, "same-name", level, None);
                    }
                    continue;
                }
                if names::starts_with_key(&name, &snap.keys) {
                    self.push_path(&path, Kind::Folder, "weak-name", RiskLevel::Caution, None);
                }
            }
        }
        if let (Some(pkg), Some(packages)) = (&snap.program.package, &self.ctx.roots.packages) {
            let data = packages.join(&pkg.family);
            self.push_path(&data, Kind::Folder, "store-data", RiskLevel::Safe, None);
        }
    }

    /// `.lnk` files below `dir` (to `depth` levels) whose target was the program's
    /// and is gone; Start Menu folders named like the program that only hold such
    /// shortcuts.
    fn shortcuts_in(&mut self, dir: &Path, depth: usize, folders: bool) {
        let mut stack = vec![(dir.to_path_buf(), 0usize)];
        while let Some((d, level)) = stack.pop() {
            for (name, is_dir) in fsutil::children(&d) {
                let path = d.join(&name);
                if is_dir {
                    if level + 1 < depth {
                        if folders
                            && level == 0
                            && names::matches(&name, &self.snap.keys)
                            && self.only_dead_links(&path)
                        {
                            self.push_path(
                                &path,
                                Kind::Folder,
                                "start-menu-folder",
                                RiskLevel::Safe,
                                None,
                            );
                            continue;
                        }
                        stack.push((path, level + 1));
                    }
                    continue;
                }
                if let Some(target) = self.dead_target(&path) {
                    self.push_path(
                        &path,
                        Kind::Shortcut,
                        "shortcut",
                        RiskLevel::Safe,
                        Some(target),
                    );
                }
            }
        }
    }

    /// The target of a `.lnk` that pointed into the program and no longer exists.
    fn dead_target(&self, link: &Path) -> Option<String> {
        if !link
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
        {
            return None;
        }
        let meta = std::fs::metadata(link).ok()?;
        if meta.len() > nfd_core::shortcut::MAX_LINK_SIZE {
            return None;
        }
        let data = std::fs::read(link).ok()?;
        let target =
            nfd_core::shortcut::parse(&data, link.parent()?, &|v| (self.ctx.env)(v)).ok()?;
        let nfd_core::shortcut::Target::Local(t) = target else {
            return None;
        };
        let tp = PathBuf::from(&t);
        (self.homes_hold(&tp) && !(self.ctx.exists)(&tp)).then_some(t)
    }

    /// A folder whose files are all dead links into the program (or nothing).
    fn only_dead_links(&self, dir: &Path) -> bool {
        let kids = fsutil::children(dir);
        kids.iter()
            .all(|(n, is_dir)| !is_dir && self.dead_target(&dir.join(n)).is_some())
    }

    fn shortcuts(&mut self) {
        let roots = self.ctx.roots.clone();
        for (sm, _) in &roots.start_menus {
            self.shortcuts_in(sm, 4, true);
        }
        for (desk, _) in &roots.desktops {
            self.shortcuts_in(desk, 1, false);
        }
        for ql in &roots.quick_launch {
            self.shortcuts_in(ql, 4, false);
        }
    }

    fn registry(&mut self) {
        let snap = self.snap;
        let reg = self.ctx.reg;
        // the program's own entry, still listed although its uninstaller ran / is gone
        if self.mode != Mode::Forced {
            if let Some((hive, key)) = snap.program.reg_key() {
                if reg.exists(hive, key) {
                    self.push_reg(
                        hive,
                        key,
                        None,
                        "arp-entry",
                        RiskLevel::Safe,
                        Some(snap.program.name.clone()),
                    );
                }
            }
        }
        // Software\<Product>, Software\<Vendor>\<Product>
        for (hive, base) in [
            (Hive::CurrentUser, "Software"),
            (Hive::CurrentUser, r"Software\WOW6432Node"),
            (Hive::LocalMachine, "SOFTWARE"),
            (Hive::LocalMachine, r"SOFTWARE\WOW6432Node"),
        ] {
            for sub in reg.subkeys(hive, base) {
                if SOFTWARE_DENY.contains(&sub.to_lowercase().as_str()) {
                    continue;
                }
                let key = format!(r"{base}\{sub}");
                let is_vendor = snap
                    .publisher
                    .as_deref()
                    .is_some_and(|p| is_publisher(&sub, p));
                if !is_vendor {
                    if names::matches(&sub, &snap.keys) {
                        self.push_reg(hive, &key, None, "software-key", RiskLevel::Safe, None);
                    }
                    continue;
                }
                let kids = reg.subkeys(hive, &key);
                let mut matched = 0;
                for kid in &kids {
                    if names::matches(kid, &snap.keys) {
                        self.push_reg(
                            hive,
                            &format!(r"{key}\{kid}"),
                            None,
                            "software-key",
                            RiskLevel::Safe,
                            None,
                        );
                        matched += 1;
                    }
                }
                let shared = snap
                    .publisher
                    .as_deref()
                    .is_some_and(|p| self.ctx.others.shares_publisher(p));
                let no_values = reg.values(hive, &key).is_none_or(|v| v.is_empty());
                if matched > 0 {
                    if matched == kids.len() && no_values && !shared {
                        self.push_reg(hive, &key, None, "vendor-key", RiskLevel::Caution, None);
                    }
                } else if names::matches(&sub, &snap.keys) {
                    // the product carries its vendor's name
                    let level = if shared {
                        RiskLevel::Caution
                    } else {
                        RiskLevel::Safe
                    };
                    self.push_reg(hive, &key, None, "software-key", level, None);
                }
            }
        }
        // App Paths\<exe>.exe of the program's exe files
        let exe_names: Vec<String> = snap
            .exes
            .iter()
            .filter_map(|e| e.file_name().map(|n| n.to_string_lossy().to_lowercase()))
            .collect();
        for (hive, base) in [
            (Hive::LocalMachine, APP_PATHS),
            (Hive::LocalMachine, APP_PATHS_WOW),
            (Hive::CurrentUser, APP_PATHS),
        ] {
            for sub in reg.subkeys(hive, base) {
                if !exe_names.contains(&sub.to_lowercase()) {
                    continue;
                }
                let key = format!(r"{base}\{sub}");
                let target = reg.values(hive, &key).and_then(|v| {
                    text(&v, "").map(|t| command::expand_vars(t.trim_matches('"'), self.ctx.env))
                });
                if let Some(t) = target {
                    let tp = PathBuf::from(&t);
                    if self.homes_hold(&tp) && !(self.ctx.exists)(&tp) {
                        self.push_reg(hive, &key, None, "app-path", RiskLevel::Safe, Some(t));
                    }
                }
            }
        }
        // Run entries starting the program's (now missing) exe files
        let mut run_names: Vec<String> = Vec::new();
        for (hive, key) in [
            (Hive::CurrentUser, RUN),
            (Hive::LocalMachine, RUN),
            (Hive::LocalMachine, RUN_WOW),
        ] {
            for (name, value) in reg.values(hive, key).unwrap_or_default() {
                let Some(cmd) = value.as_str() else { continue };
                let Some(c) =
                    command::parse(cmd, self.ctx.env, self.ctx.exists, &self.ctx.system_root)
                else {
                    continue;
                };
                if self.homes_hold(&c.exe) && !(self.ctx.exists)(&c.exe) {
                    self.push_reg(
                        hive,
                        key,
                        Some(&name),
                        "run-entry",
                        RiskLevel::Safe,
                        Some(cmd.to_owned()),
                    );
                    run_names.push(name.to_lowercase());
                }
            }
        }
        for hive in [Hive::CurrentUser, Hive::LocalMachine] {
            for key in [STARTUP_APPROVED, STARTUP_APPROVED_32] {
                for (name, _) in reg.values(hive, key).unwrap_or_default() {
                    if run_names.contains(&name.to_lowercase()) {
                        self.push_reg(hive, key, Some(&name), "run-entry", RiskLevel::Safe, None);
                    }
                }
            }
        }
    }

    fn services(&mut self) {
        let reg = self.ctx.reg;
        for name in reg.subkeys(Hive::LocalMachine, SERVICES) {
            let key = format!(r"{SERVICES}\{name}");
            let Some(values) = reg.values(Hive::LocalMachine, &key) else {
                continue;
            };
            let Some(image) = text(&values, "ImagePath") else {
                continue;
            };
            let Some(c) =
                command::parse(image, self.ctx.env, self.ctx.exists, &self.ctx.system_root)
            else {
                continue;
            };
            if self.snap.homes.iter().any(|h| within(&c.exe, h)) {
                let id = format!(r"service:{name}");
                if self.has(&id) {
                    continue;
                }
                self.items.push(Leftover {
                    id,
                    kind: Kind::Service,
                    path: c.exe.to_string_lossy().into_owned(),
                    value: None,
                    scope: Scope::Machine,
                    level: RiskLevel::Caution,
                    reason: "service".into(),
                    detail: Some(name.clone()),
                    also: None,
                    bytes: 0,
                    files: 0,
                    mtime: 0,
                    selected: false,
                    blocked: Some("info".into()),
                    large: false,
                });
            }
        }
    }
}

/// A folder or key `name` is the vendor's (`publisher` = [`names::publisher_key`]).
fn is_publisher(name: &str, publisher: &str) -> bool {
    let n = names::publisher_key(name);
    !n.is_empty() && (n == publisher || n.replace(' ', "") == publisher.replace(' ', ""))
}

/// Registry keys a leftover may never be: top-level keys, `Software` itself and
/// what Windows keeps there (other than the three program locations: `Uninstall`
/// entries, `App Paths` entries and `Run` values).
#[must_use]
pub fn registry_refusal(hive: Hive, key: &str, is_value: bool) -> Option<&'static str> {
    let k = key.trim_matches('\\').to_lowercase();
    let parts: Vec<&str> = k.split('\\').collect();
    if parts.len() < 2 || parts[0] != "software" {
        return Some("registry-protected");
    }
    let rest = if parts.get(1) == Some(&"wow6432node") {
        &parts[2..]
    } else {
        &parts[1..]
    };
    if rest.is_empty() {
        return Some("registry-protected");
    }
    if rest[0] == "microsoft" {
        let tail = rest[1..].join("\\");
        let allowed = (!is_value
            && (tail.starts_with("windows\\currentversion\\uninstall\\")
                || tail.starts_with("windows\\currentversion\\app paths\\"))
            && tail.split('\\').count() == 4)
            || (is_value
                && [
                    "windows\\currentversion\\run",
                    "windows\\currentversion\\explorer\\startupapproved\\run",
                    "windows\\currentversion\\explorer\\startupapproved\\run32",
                ]
                .contains(&tail.as_str()));
        return (!allowed).then_some("registry-protected");
    }
    if is_value || SOFTWARE_DENY.contains(&rest[0]) {
        return Some("registry-protected");
    }
    let _ = hive;
    None
}

/// Look for what `snap`'s program left behind.
#[must_use]
pub fn scan(snap: &Snapshot, ctx: &Ctx<'_>, mode: Mode) -> Report {
    let mut s = Scan {
        snap,
        ctx,
        mode,
        items: Vec::new(),
    };
    s.folders();
    s.shortcuts();
    if snap.program.source != Source::Store {
        s.registry();
        s.services();
    }
    let mut items = s.items;
    // largest first within each kind; folders, shortcuts, registry, services
    let rank = |k: Kind| match k {
        Kind::Folder | Kind::File => 0,
        Kind::Shortcut => 1,
        Kind::RegistryKey | Kind::RegistryValue => 2,
        Kind::Service => 3,
    };
    items.sort_by(|a, b| {
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then(b.bytes.cmp(&a.bytes))
            .then(norm(Path::new(&a.path)).cmp(&norm(Path::new(&b.path))))
    });
    let selected_bytes = items.iter().filter(|i| i.selected).map(|i| i.bytes).sum();
    let total_bytes = items
        .iter()
        .filter(|i| i.blocked.is_none())
        .map(|i| i.bytes)
        .sum();
    Report {
        program: snap.program.clone(),
        mode,
        items,
        selected_bytes,
        total_bytes,
    }
}

#[cfg(test)]
mod tests;
