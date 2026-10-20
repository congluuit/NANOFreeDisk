//! Software junk (`docs/UNINSTALLER.md` §2.5): what installed programs pile up and
//! never need — Chromium / Electron caches, crash dumps, logs, temporary files, the
//! old versions and packages their updaters keep — found by the *structure* of the
//! folder (a `Cache` only counts inside a Chromium profile, recognized by its
//! `Local State` / `Preferences`), grouped by program.
//!
//! Never offered: profiles and site data (`Local Storage`, `IndexedDB`, `Cookies`,
//! `Session Storage`…), anything a rule pack of the Clean view already covers (no
//! double counting), conversation history of AI agents, this app's own data.
//! Cleaning goes Planner → SafetyGate → Executor → quarantine like a rule.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use nfd_core::RiskLevel;
use nfd_core::plan::{self, CleanPlan, DeleteMode, PlanItem, PlanOptions};
use nfd_core::safety::{Origin, SafetyGate};
use serde::{Deserialize, Serialize};

use crate::fsutil;
use crate::inventory::{Program, norm, same, within};
use crate::names;

/// What kind of junk an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JunkKind {
    Cache,
    Crash,
    Log,
    Temp,
    Update,
}

/// One junk folder or file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JunkItem {
    pub path: PathBuf,
    pub kind: JunkKind,
    /// What it is exactly (the UI's text): `chromium-cache`, `service-worker`,
    /// `electron-logs`, `crashpad`, `crash-dumps`, `crashes`, `dump-file`, `logs`,
    /// `temp`, `old-version`, `update-package`, `update-temp`, `pending-update`.
    pub what: String,
    pub level: RiskLevel,
    pub is_dir: bool,
    pub bytes: u64,
    pub files: u64,
    /// Newest write inside.
    pub newest: i64,
    /// Logical size and last write of a file (checked again before cleaning).
    pub size: u64,
    pub mtime: i64,
    pub selected: bool,
    /// SafetyGate code, `in-use`, `contains-history`, `too-large`.
    pub blocked: Option<String>,
}

/// The junk of one program (or of one folder no installed program claims).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JunkGroup {
    /// The program's id, or `folder:<path>`.
    pub key: String,
    pub program: Option<String>,
    pub name: String,
    pub publisher: Option<String>,
    /// The program's data folder the junk was found in.
    pub folder: PathBuf,
    /// Exe names to close first.
    pub requires_closed: Vec<String>,
    /// One of them is running now.
    pub running: bool,
    pub items: Vec<JunkItem>,
    pub bytes: u64,
}

/// Everything [`scan`] found.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JunkReport {
    pub groups: Vec<JunkGroup>,
    pub total_bytes: u64,
    pub selected_bytes: u64,
    pub cancelled: bool,
}

impl JunkReport {
    /// Paths pre-selected (🟢, not blocked).
    #[must_use]
    pub fn preselected(&self) -> Vec<PathBuf> {
        self.groups
            .iter()
            .flat_map(|g| {
                g.items
                    .iter()
                    .filter(|i| i.selected)
                    .map(|i| i.path.clone())
            })
            .collect()
    }
}

/// Where the junk is looked for.
#[derive(Debug, Clone, Default)]
pub struct JunkRoots {
    /// `AppData\Roaming`, `AppData\Local`, `AppData\LocalLow`.
    pub app_data: Vec<PathBuf>,
    /// `AppData\Local` (electron-updater's `<app>-updater` folders live there).
    pub local: Option<PathBuf>,
    /// `AppData\Local\Programs` (per-user installs).
    pub programs: Option<PathBuf>,
    /// Folders never looked into (this app's own data).
    pub skip: Vec<PathBuf>,
}

impl JunkRoots {
    /// The real folders (`resolve` maps `APPDATA`, `LOCALAPPDATA`, `HOME`).
    #[must_use]
    pub fn detect(resolve: &dyn Fn(&str) -> Option<String>, own_data: &[PathBuf]) -> Self {
        let p = |v: &str| resolve(v).map(PathBuf::from);
        let mut r = Self {
            skip: own_data.to_vec(),
            ..Self::default()
        };
        r.app_data.extend(p("APPDATA"));
        if let Some(l) = p("LOCALAPPDATA") {
            r.programs = Some(l.join("Programs"));
            r.local = Some(l.clone());
            r.app_data.push(l);
        }
        if let Some(h) = p("HOME") {
            r.app_data.push(h.join(r"AppData\LocalLow"));
        }
        r
    }
}

/// Top-level folders of AppData that are Windows', the Store's or the temp folder.
const SKIP_TOP: &[&str] = &[
    "microsoft",
    "packages",
    "temp",
    "programs",
    "crashdumps",
    "d3dscache",
    "virtualstore",
    "connecteddevicesplatform",
    "comms",
    "publishers",
    "placeholdertilelogofolder",
];

/// Chromium / Electron cache folders (rebuilt on the next start).
const CHROMIUM_CACHES: &[&str] = &[
    "cache",
    "code cache",
    "gpucache",
    "dawncache",
    "dawngraphitecache",
    "dawnwebgpucache",
    "graphitedawncache",
    "grshadercache",
    "shadercache",
];

/// Never walked into: site data, profiles' databases, repositories.
const NEVER_DESCEND: &[&str] = &[
    "local storage",
    "session storage",
    "indexeddb",
    "databases",
    "blob_storage",
    "file system",
    "extensions",
    "extension state",
    "extension rules",
    "sync data",
    "node_modules",
    ".git",
    "webstorage",
    "shared_proto_db",
    "storage",
];

/// Crashpad's report folders.
const CRASHPAD: &[&str] = &["reports", "completed", "pending", "new", "attachments"];

/// What a scan reads besides the folders.
pub struct JunkCtx<'a> {
    pub gate: &'a SafetyGate,
    /// `true` when a rule pack of the Clean view already covers `path`.
    pub covered: &'a dyn Fn(&Path) -> bool,
    /// Image paths of running processes.
    pub running: &'a [PathBuf],
    pub max_entries: u64,
    /// Folder levels below a program's data folder looked at.
    pub depth: usize,
    pub cancel: Option<&'a AtomicBool>,
}

impl std::fmt::Debug for JunkCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JunkCtx")
            .field("max_entries", &self.max_entries)
            .field("depth", &self.depth)
            .finish_non_exhaustive()
    }
}

/// A Chromium profile / user-data folder: Chromium's own files are there.
fn is_chromium(files: &[String], dirs: &[String]) -> bool {
    let has_f = |n: &str| files.iter().any(|f| f == n);
    let has_d = |n: &str| dirs.iter().any(|d| d == n);
    has_f("local state")
        || has_f("preferences")
        || ((has_d("network") || has_d("local storage"))
            && (has_d("cache") || has_d("code cache") || has_d("gpucache")))
}

/// `app-1.2.3` → (1, 2, 3, 0).
fn squirrel_version(name: &str) -> Option<Vec<u64>> {
    let v = name.strip_prefix("app-")?;
    let core = v.split(['-', '+']).next()?;
    let parts: Option<Vec<u64>> = core.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty())
}

/// `Discord-1.0.9036-full.nupkg` → (version, full).
fn nupkg_version(name: &str) -> Option<(Vec<u64>, bool)> {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".nupkg")?;
    let (stem, full) = match stem.strip_suffix("-full") {
        Some(s) => (s, true),
        None => (stem.strip_suffix("-delta")?, false),
    };
    let v = stem.rsplit('-').next()?;
    let parts: Option<Vec<u64>> = v.split('.').map(|p| p.parse().ok()).collect();
    parts.map(|p| (p, full))
}

/// A found item before sizing.
struct Hit {
    path: PathBuf,
    kind: JunkKind,
    what: &'static str,
    level: RiskLevel,
    is_dir: bool,
}

/// Look for junk below one program data folder `app`.
fn walk_app(app: &Path, depth: usize, top_level_local: bool, hits: &mut Vec<Hit>) {
    let mut stack: Vec<(PathBuf, usize, bool)> = vec![(app.to_path_buf(), 0, false)];
    while let Some((dir, level, in_chromium)) = stack.pop() {
        let kids = fsutil::children(&dir);
        let lower: Vec<(String, String, bool)> = kids
            .iter()
            .map(|(n, d)| (n.clone(), n.to_lowercase(), *d))
            .collect();
        let files: Vec<String> = lower.iter().filter(|k| !k.2).map(|k| k.1.clone()).collect();
        let dirs: Vec<String> = lower.iter().filter(|k| k.2).map(|k| k.1.clone()).collect();
        let chromium = is_chromium(&files, &dirs);
        let mut handled: Vec<String> = Vec::new();
        let mut hit =
            |name: &str, kind: JunkKind, what: &'static str, level: RiskLevel, is_dir: bool| {
                hits.push(Hit {
                    path: dir.join(name),
                    kind,
                    what,
                    level,
                    is_dir,
                });
            };
        for (name, low, is_dir) in &lower {
            if *is_dir {
                if chromium && CHROMIUM_CACHES.contains(&low.as_str()) {
                    hit(
                        name,
                        JunkKind::Cache,
                        "chromium-cache",
                        RiskLevel::Safe,
                        true,
                    );
                    handled.push(low.clone());
                } else if chromium && low == "service worker" {
                    if fsutil::children(&dir.join(name))
                        .iter()
                        .any(|(n, d)| *d && n.eq_ignore_ascii_case("cachestorage"))
                    {
                        hit(
                            &format!("{name}\\CacheStorage"),
                            JunkKind::Cache,
                            "service-worker",
                            RiskLevel::Caution,
                            true,
                        );
                    }
                    handled.push(low.clone());
                } else if low == "crashpad" {
                    for (sub, sub_dir) in fsutil::children(&dir.join(name)) {
                        if sub_dir && CRASHPAD.contains(&sub.to_lowercase().as_str()) {
                            hit(
                                &format!("{name}\\{sub}"),
                                JunkKind::Crash,
                                "crashpad",
                                RiskLevel::Safe,
                                true,
                            );
                        }
                    }
                    handled.push(low.clone());
                } else if [
                    "crashdumps",
                    "crash reports",
                    "crash_reports",
                    "crashreports",
                    "minidumps",
                ]
                .contains(&low.as_str())
                {
                    hit(name, JunkKind::Crash, "crash-dumps", RiskLevel::Safe, true);
                    handled.push(low.clone());
                } else if low == "crashes" {
                    hit(name, JunkKind::Crash, "crashes", RiskLevel::Caution, true);
                    handled.push(low.clone());
                } else if low == "logs" || low == "log" {
                    if chromium || in_chromium {
                        hit(name, JunkKind::Log, "electron-logs", RiskLevel::Safe, true);
                    } else {
                        hit(name, JunkKind::Log, "logs", RiskLevel::Caution, true);
                    }
                    handled.push(low.clone());
                } else if level == 0 && (low == "temp" || low == "tmp") {
                    hit(name, JunkKind::Temp, "temp", RiskLevel::Caution, true);
                    handled.push(low.clone());
                }
            } else if level <= 2 && (low.ends_with(".dmp") || low.ends_with(".mdmp")) {
                hit(name, JunkKind::Crash, "dump-file", RiskLevel::Safe, false);
            }
        }
        // Squirrel (Discord, Slack, Teams classic…): old app-x.y.z and packages
        if level == 0 && files.iter().any(|f| f == "update.exe") {
            let versions: Vec<(&String, Vec<u64>)> = lower
                .iter()
                .filter(|k| k.2)
                .filter_map(|k| squirrel_version(&k.1).map(|v| (&k.0, v)))
                .collect();
            if let Some(newest) = versions.iter().map(|(_, v)| v.clone()).max() {
                for (name, v) in &versions {
                    if *v != newest {
                        hit(
                            name,
                            JunkKind::Update,
                            "old-version",
                            RiskLevel::Caution,
                            true,
                        );
                        handled.push(name.to_lowercase());
                    }
                }
            }
            if let Some((pkg_name, _, _)) = lower.iter().find(|k| k.2 && k.1 == "packages") {
                let pkgs = fsutil::children(&dir.join(pkg_name));
                let newest_full = pkgs
                    .iter()
                    .filter_map(|(n, d)| (!d).then(|| nupkg_version(n)).flatten())
                    .filter(|(_, full)| *full)
                    .map(|(v, _)| v)
                    .max();
                for (n, d) in &pkgs {
                    if *d {
                        if n.eq_ignore_ascii_case("squirreltemp") {
                            hit(
                                &format!("{pkg_name}\\{n}"),
                                JunkKind::Update,
                                "update-temp",
                                RiskLevel::Safe,
                                true,
                            );
                        }
                        continue;
                    }
                    if let Some((v, full)) = nupkg_version(n) {
                        if !(full && Some(&v) == newest_full.as_ref()) {
                            hit(
                                &format!("{pkg_name}\\{n}"),
                                JunkKind::Update,
                                "update-package",
                                RiskLevel::Safe,
                                false,
                            );
                        }
                    }
                }
                handled.push("packages".into());
            }
        }
        // electron-updater: %LOCALAPPDATA%\<app>-updater
        if level == 0 && top_level_local {
            let name = app
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if name.ends_with("-updater")
                && (dirs.iter().any(|d| d == "pending")
                    || files
                        .iter()
                        .any(|f| f == "installer.exe" || f == "update-info.json"))
            {
                hits.push(Hit {
                    path: app.to_path_buf(),
                    kind: JunkKind::Update,
                    what: "pending-update",
                    level: RiskLevel::Safe,
                    is_dir: true,
                });
                continue;
            }
        }
        if level + 1 >= depth {
            continue;
        }
        for (name, low, is_dir) in &lower {
            if *is_dir && !handled.contains(low) && !NEVER_DESCEND.contains(&low.as_str()) {
                stack.push((dir.join(name), level + 1, chromium || in_chromium));
            }
        }
    }
}

/// Keys of every program, for attributing folders.
struct Owners<'a> {
    list: Vec<(&'a Program, Vec<String>, Option<String>)>,
}

impl<'a> Owners<'a> {
    fn new(programs: &'a [Program]) -> Self {
        Self {
            list: programs
                .iter()
                .map(|p| {
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
                    let keys = names::program_keys(
                        &p.name,
                        p.publisher.as_deref(),
                        dir.as_deref(),
                        &stems,
                    );
                    let publisher = p
                        .publisher
                        .as_deref()
                        .map(names::publisher_key)
                        .filter(|k| names::distinctive(k));
                    (p, keys, publisher)
                })
                .collect(),
        }
    }

    /// The program a data folder `app` (and the sub-folder `sub` an item sits in)
    /// belongs to.
    fn of(&self, app: &Path, sub: Option<&str>) -> Option<&'a Program> {
        if let Some((p, _, _)) = self.list.iter().find(|(p, _, _)| {
            p.install_location
                .as_deref()
                .is_some_and(|l| within(l, app) || same(l, app))
        }) {
            return Some(p);
        }
        let name = app.file_name()?.to_string_lossy().into_owned();
        // `<vendor>\<product>`: the product's folder decides
        if let Some(sub) = sub {
            let is_vendor = self.list.iter().any(|(_, _, pk)| {
                pk.as_deref()
                    .is_some_and(|pk| names::publisher_key(&name) == pk)
            });
            if is_vendor {
                if let Some((p, _, _)) = self
                    .list
                    .iter()
                    .find(|(_, keys, _)| names::matches(sub, keys))
                {
                    return Some(p);
                }
            }
        }
        self.list
            .iter()
            .find(|(_, keys, _)| names::matches(&name, keys))
            .map(|(p, _, _)| *p)
    }
}

/// Exe names in `dir` (top level) worth closing before cleaning.
fn exe_names(dir: &Path) -> Vec<String> {
    fsutil::children(dir)
        .into_iter()
        .filter(|(n, d)| !d && n.to_ascii_lowercase().ends_with(".exe"))
        .map(|(n, _)| n.to_ascii_lowercase())
        .filter(|n| {
            let stem = n.trim_end_matches(".exe");
            !names::GENERIC_EXES.contains(&stem) && !stem.starts_with("unins")
        })
        .collect()
}

/// Find the software junk of `programs` in `roots`.
#[must_use]
pub fn scan(programs: &[Program], roots: &JunkRoots, ctx: &JunkCtx<'_>) -> JunkReport {
    let owners = Owners::new(programs);
    let mut report = JunkReport::default();
    let local = roots.local.clone();
    let mut apps: Vec<(PathBuf, bool)> = Vec::new();
    for root in &roots.app_data {
        let is_local = local.as_deref().is_some_and(|l| same(l, root));
        for (name, is_dir) in fsutil::children(root) {
            if is_dir && !SKIP_TOP.contains(&name.to_lowercase().as_str()) {
                apps.push((root.join(name), is_local));
            }
        }
    }
    if let Some(p) = &roots.programs {
        for (name, is_dir) in fsutil::children(p) {
            if is_dir {
                apps.push((p.join(name), false));
            }
        }
    }
    let mut groups: Vec<JunkGroup> = Vec::new();
    for (app, is_local) in apps {
        if ctx.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            report.cancelled = true;
            break;
        }
        if roots
            .skip
            .iter()
            .any(|s| within(&app, s) || within(s, &app))
        {
            continue;
        }
        let mut hits = Vec::new();
        walk_app(&app, ctx.depth, is_local, &mut hits);
        for hit in hits {
            if (ctx.covered)(&hit.path) {
                continue;
            }
            let sub = hit
                .path
                .strip_prefix(&app)
                .ok()
                .and_then(|r| r.components().next())
                .map(|c| c.as_os_str().to_string_lossy().into_owned());
            let owner = owners.of(&app, sub.as_deref());
            let key = owner.map_or_else(|| format!("folder:{}", norm(&app)), |p| p.id.clone());
            let gi = match groups.iter().position(|g| g.key == key) {
                Some(i) => i,
                None => {
                    let mut requires: Vec<String> = owner
                        .and_then(|p| p.install_location.as_deref())
                        .map(exe_names)
                        .unwrap_or_default();
                    if let Some(icon) = owner
                        .and_then(|p| p.icon.as_deref())
                        .and_then(Path::file_name)
                    {
                        let n = icon.to_string_lossy().to_ascii_lowercase();
                        if n.ends_with(".exe") && !requires.contains(&n) {
                            requires.push(n);
                        }
                    }
                    if requires.is_empty() {
                        requires = exe_names(&app);
                    }
                    let running = ctx.running.iter().any(|r| {
                        r.file_name().is_some_and(|n| {
                            requires.contains(&n.to_string_lossy().to_ascii_lowercase())
                        })
                    });
                    groups.push(JunkGroup {
                        key,
                        program: owner.map(|p| p.id.clone()),
                        name: owner.map_or_else(
                            || {
                                app.file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default()
                            },
                            |p| p.name.clone(),
                        ),
                        publisher: owner.and_then(|p| p.publisher.clone()),
                        folder: app.clone(),
                        requires_closed: requires,
                        running,
                        items: Vec::new(),
                        bytes: 0,
                    });
                    groups.len() - 1
                }
            };
            let t = fsutil::totals(&hit.path, ctx.max_entries);
            if t.files == 0 && t.bytes == 0 {
                continue;
            }
            let mut blocked = ctx
                .gate
                .check_path(&hit.path, Origin::Rule { level: hit.level })
                .err()
                .map(|b| b.code().to_owned());
            if blocked.is_none() && t.history.is_some() {
                blocked = Some("contains-history".into());
            }
            if blocked.is_none() && t.truncated {
                blocked = Some("too-large".into());
            }
            if blocked.is_none() && ctx.running.iter().any(|r| within(r, &hit.path)) {
                blocked = Some("in-use".into());
            }
            let info = nfd_platform::fsinfo::file_info(&hit.path).ok();
            let g = &mut groups[gi];
            if g.items.iter().any(|i| same(&i.path, &hit.path)) {
                continue;
            }
            g.bytes += t.bytes;
            g.items.push(JunkItem {
                selected: blocked.is_none() && hit.level == RiskLevel::Safe,
                path: hit.path,
                kind: hit.kind,
                what: hit.what.to_owned(),
                level: hit.level,
                is_dir: hit.is_dir,
                bytes: t.bytes,
                files: t.files,
                newest: t.newest,
                size: if hit.is_dir {
                    0
                } else {
                    info.map_or(0, |i| i.logical)
                },
                mtime: info.map_or(0, |i| i.mtime),
                blocked,
            });
        }
    }
    groups.retain(|g| !g.items.is_empty());
    for g in &mut groups {
        g.items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.bytes));
    report.total_bytes = groups
        .iter()
        .flat_map(|g| &g.items)
        .filter(|i| i.blocked.is_none())
        .map(|i| i.bytes)
        .sum();
    report.selected_bytes = groups
        .iter()
        .flat_map(|g| &g.items)
        .filter(|i| i.selected)
        .map(|i| i.bytes)
        .sum();
    report.groups = groups;
    report
}

/// `true` when a rule pack already covers `path` — the path or a folder above it
/// matches a rule, or a rule's folder lies inside it: the Clean view (or the
/// DevCache view) shows it, the software junk does not count it twice. 🔴 Keep
/// rules cover too, so what they protect is never offered here.
pub fn covered_by(matcher: &nfd_rules::matcher::Matcher) -> impl Fn(&Path) -> bool + '_ {
    let bases: Vec<PathBuf> = matcher.bases().into_iter().map(|(b, _)| b).collect();
    move |path: &Path| {
        path.ancestors()
            .take(8)
            .any(|a| !matcher.rules_matching(a).is_empty())
            || bases.iter().any(|b| within(b, path))
    }
}

/// The plan for the `picked` items of `report` (blocked ones left out): quarantine,
/// with the program's exe names to close.
#[must_use]
pub fn plan(report: &JunkReport, picked: &[PathBuf]) -> CleanPlan {
    let wanted: Vec<String> = picked.iter().map(|p| norm(p)).collect();
    let items: Vec<PlanItem> = report
        .groups
        .iter()
        .flat_map(|g| g.items.iter().map(move |i| (g, i)))
        .filter(|(_, i)| i.blocked.is_none() && wanted.contains(&norm(&i.path)))
        .map(|(g, i)| PlanItem {
            path: i.path.clone(),
            origin: Origin::Rule { level: i.level },
            rule_id: Some(format!("software.{}", i.what)),
            is_dir: i.is_dir,
            bytes: i.bytes,
            files: i.files,
            size: i.size,
            mtime: i.mtime,
            requires_closed: g.requires_closed.clone(),
            mode: DeleteMode::Quarantine,
            selected: true,
            keeper: None,
        })
        .collect();
    let mut plan = plan::build(
        items,
        PlanOptions {
            expert: true,
            include_caution: true,
            permanent: false,
        },
    );
    for item in &mut plan.items {
        item.selected = true;
        item.mode = DeleteMode::Quarantine;
    }
    plan
}

#[cfg(test)]
mod tests;
