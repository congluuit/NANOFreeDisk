//! Per-website browser cache cleanup (Clean view → "Web browser caches" →
//! "Advanced: clean one website"; `nfd browsers`).
//!
//! Lists, per browser profile, which websites own how much of each cache store, and
//! plans removing only the chosen sites' entries:
//!
//! | store | format | per site |
//! |---|---|---|
//! | `Cache/Cache_Data` (HTTP cache) | blockfile on Windows ([`blockfile`]) | **listed only** |
//! | `Cache/Cache_Data` | Simple ([`simple`]), where a browser uses it | whole entries |
//! | `Code Cache/js`, `Code Cache/wasm` | Simple | whole entries |
//! | `Service Worker/CacheStorage/<origin>` | [`cachestorage`] | whole origin folder |
//! | Firefox `cache2/entries` | [`firefox`] | whole entries |
//!
//! Safety, in this order:
//!
//! * Only these stores are ever read or planned: never cookies, logins, Local
//!   Storage, IndexedDB, History, passwords or extensions ([`cache_path_allowed`]).
//! * Every planned path must also be covered by the browser's 🟢 Safe rule in
//!   `rules/browsers.toml` — per-site cleaning is a strict subset of what the Clean
//!   view already cleans as a whole.
//! * Nothing is edited in place: whole entry files (and the advisory index, so the
//!   browser rebuilds it) or a whole origin folder go through the usual Planner →
//!   SafetyGate → Executor → UndoJournal path into the quarantine (Restore works).
//! * The blockfile HTTP cache is never modified (see [`blockfile`]).
//! * Nothing is planned while the browser runs.

// tests build their fixtures with `unwrap()`
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod blockfile;
pub mod cachestorage;
pub mod firefox;
#[doc(hidden)]
pub mod fixture;
pub mod simple;
pub mod site;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::plan::{self, CleanPlan, DeleteMode, PlanItem, PlanOptions};
use nfd_core::safety::gate::Origin;
use serde::Serialize;

/// Browser engine family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    Chromium,
    Firefox,
}

/// Where a browser keeps its profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `${LOCALAPPDATA}/<rel>/<profile>/…`: every sub-folder with a cache store.
    UserData(&'static str),
    /// One profile split in two (Opera): `Cache` under `${LOCALAPPDATA}/<rel>`, the
    /// rest under `${APPDATA}/<rel>`.
    Split(&'static [&'static str]),
    /// `${LOCALAPPDATA}/Mozilla/Firefox/Profiles/*`.
    FirefoxProfiles,
}

/// A browser this module knows, tied to its rule in `rules/browsers.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserDef {
    pub id: &'static str,
    pub name: &'static str,
    pub rule_id: &'static str,
    /// Must be closed before anything is cleaned.
    pub exe: &'static str,
    pub engine: Engine,
    pub layout: Layout,
}

/// The browsers of the "Web browser caches" pack.
pub const BROWSERS: &[BrowserDef] = &[
    BrowserDef {
        id: "chrome",
        name: "Chrome",
        rule_id: "browser.chrome.cache",
        exe: "chrome.exe",
        engine: Engine::Chromium,
        layout: Layout::UserData("Google/Chrome/User Data"),
    },
    BrowserDef {
        id: "edge",
        name: "Edge",
        rule_id: "browser.edge.cache",
        exe: "msedge.exe",
        engine: Engine::Chromium,
        layout: Layout::UserData("Microsoft/Edge/User Data"),
    },
    BrowserDef {
        id: "brave",
        name: "Brave",
        rule_id: "browser.brave.cache",
        exe: "brave.exe",
        engine: Engine::Chromium,
        layout: Layout::UserData("BraveSoftware/Brave-Browser/User Data"),
    },
    BrowserDef {
        id: "vivaldi",
        name: "Vivaldi",
        rule_id: "browser.vivaldi.cache",
        exe: "vivaldi.exe",
        engine: Engine::Chromium,
        layout: Layout::UserData("Vivaldi/User Data"),
    },
    BrowserDef {
        id: "opera",
        name: "Opera / Opera GX",
        rule_id: "browser.opera.cache",
        exe: "opera.exe",
        engine: Engine::Chromium,
        layout: Layout::Split(&[
            "Opera Software/Opera Stable",
            "Opera Software/Opera GX Stable",
        ]),
    },
    BrowserDef {
        id: "firefox",
        name: "Firefox",
        rule_id: "browser.firefox.cache",
        exe: "firefox.exe",
        engine: Engine::Firefox,
        layout: Layout::FirefoxProfiles,
    },
];

/// The browser with id `id` (case-insensitive).
#[must_use]
pub fn browser(id: &str) -> Option<&'static BrowserDef> {
    BROWSERS.iter().find(|b| b.id.eq_ignore_ascii_case(id))
}

/// A cache store of a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoreKind {
    HttpCache,
    CodeCacheJs,
    CodeCacheWasm,
    CacheStorage,
    FirefoxCache,
}

/// On-disk format of a store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    Blockfile,
    Simple,
    CacheStorage,
    Cache2,
    Unknown,
}

/// Where the caches are read from and what runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub local_app_data: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    /// Running exe names, lower case (`nfd_platform::locks::running_processes`).
    pub running: Vec<String>,
}

impl Env {
    /// This machine: `%LOCALAPPDATA%`, `%APPDATA%` and the running processes.
    #[must_use]
    pub fn detect() -> Self {
        Self {
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            app_data: std::env::var_os("APPDATA").map(PathBuf::from),
            running: nfd_platform::locks::running_processes().unwrap_or_default(),
        }
    }

    /// A fixture laid out as `<root>/Local` and `<root>/Roaming`; `running.txt`
    /// (one exe per line) stands for the running processes.
    #[must_use]
    pub fn fixture(root: &Path) -> Self {
        let running = fs::read_to_string(root.join("running.txt"))
            .map(|t| {
                t.lines()
                    .map(|l| l.trim().to_ascii_lowercase())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            local_app_data: Some(root.join("Local")),
            app_data: Some(root.join("Roaming")),
            running,
        }
    }

    /// `${LOCALAPPDATA}` / `${APPDATA}` as this environment sees them (for the rule
    /// matcher); other variables from the process environment.
    #[must_use]
    pub fn resolve(&self, var: &str) -> Option<String> {
        match var {
            "LOCALAPPDATA" => self
                .local_app_data
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            "APPDATA" => self
                .app_data
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            _ => std::env::var(var).ok(),
        }
    }

    #[must_use]
    pub fn is_running(&self, exe: &str) -> bool {
        nfd_platform::locks::is_running(&self.running, exe)
    }
}

/// One profile of a browser: `local` holds `Cache` (and everything for most
/// browsers), `roaming` the Code Cache and CacheStorage (same as `local` except
/// Opera).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub browser: &'static BrowserDef,
    pub name: String,
    pub local: PathBuf,
    pub roaming: PathBuf,
}

impl Profile {
    /// The stores this profile may have, with their folders.
    #[must_use]
    pub fn stores(&self) -> Vec<(StoreKind, PathBuf)> {
        match self.browser.engine {
            Engine::Chromium => vec![
                (
                    StoreKind::HttpCache,
                    self.local.join("Cache").join("Cache_Data"),
                ),
                (
                    StoreKind::CodeCacheJs,
                    self.roaming.join("Code Cache").join("js"),
                ),
                (
                    StoreKind::CodeCacheWasm,
                    self.roaming.join("Code Cache").join("wasm"),
                ),
                (
                    StoreKind::CacheStorage,
                    self.roaming.join("Service Worker").join("CacheStorage"),
                ),
            ],
            Engine::Firefox => vec![(
                StoreKind::FirefoxCache,
                self.local.join("cache2").join("entries"),
            )],
        }
    }

    fn has_store(&self) -> bool {
        self.stores().iter().any(|(_, p)| p.is_dir())
    }
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| fs::symlink_metadata(e.path()).is_ok_and(|m| m.is_dir()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// Profiles of `b` that hold at least one cache store.
#[must_use]
pub fn profiles(env: &Env, b: &'static BrowserDef) -> Vec<Profile> {
    let name_of = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let found: Vec<Profile> = match b.layout {
        Layout::UserData(rel) => env
            .local_app_data
            .as_ref()
            .map(|l| subdirs(&l.join(rel)))
            .unwrap_or_default()
            .into_iter()
            .map(|p| Profile {
                browser: b,
                name: name_of(&p),
                roaming: p.clone(),
                local: p,
            })
            .collect(),
        Layout::Split(rels) => rels
            .iter()
            .filter_map(|rel| {
                let local = env.local_app_data.as_ref()?.join(rel);
                let roaming = env.app_data.as_ref()?.join(rel);
                Some(Profile {
                    browser: b,
                    name: name_of(&local),
                    local,
                    roaming,
                })
            })
            .collect(),
        Layout::FirefoxProfiles => env
            .local_app_data
            .as_ref()
            .map(|l| subdirs(&l.join("Mozilla").join("Firefox").join("Profiles")))
            .unwrap_or_default()
            .into_iter()
            .map(|p| Profile {
                browser: b,
                name: name_of(&p),
                roaming: p.clone(),
                local: p,
            })
            .collect(),
    };
    found.into_iter().filter(Profile::has_store).collect()
}

/// `true` when `path` lies in one of the cache stores of `profile` that may lose
/// entries — never a profile file such as `Cookies`, `Login Data`, `History`,
/// `Local Storage`, `IndexedDB` or `Extensions`, and never a store folder itself.
#[must_use]
pub fn cache_path_allowed(profile: &Profile, path: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
    let p = norm(path);
    if p.split('/').any(|c| c == ".." || c == ".") {
        return false;
    }
    let under = |root: &Path, rel: &str| {
        let base = format!("{}/{rel}/", norm(root).trim_end_matches('/'));
        p.len() > base.len() && p.starts_with(&base) && !p[base.len()..].is_empty()
    };
    match profile.browser.engine {
        Engine::Chromium => {
            under(&profile.local, "cache/cache_data")
                || under(&profile.roaming, "code cache/js")
                || under(&profile.roaming, "code cache/wasm")
                || under(&profile.roaming, "service worker/cachestorage")
        }
        Engine::Firefox => {
            under(&profile.local, "cache2/entries")
                || p == format!(
                    "{}/cache2/index",
                    norm(&profile.local).trim_end_matches('/')
                )
        }
    }
}

/// One cache store of a profile as the scan saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreReport {
    pub kind: StoreKind,
    pub path: PathBuf,
    pub format: Format,
    /// Entries can be removed one site at a time (not the blockfile HTTP cache).
    pub cleanable: bool,
    pub entries: u64,
    pub bytes: u64,
    /// Bytes of entries without a website (extensions, internal pages, unreadable).
    pub other_bytes: u64,
    /// Blockfile chains cut short: the listing is partial.
    pub damaged: u32,
    /// Read error (e.g. locked by the running browser).
    pub error: Option<String>,
}

/// What one website holds in a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteRow {
    pub site: String,
    pub bytes: u64,
    pub entries: u64,
    /// Part that per-site cleaning removes.
    pub cleanable_bytes: u64,
    pub cleanable_entries: u64,
    /// Part in the blockfile HTTP cache: listed, cleaned only with the whole cache.
    pub listed_bytes: u64,
    pub stores: Vec<StoreKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileReport {
    pub name: String,
    pub path: PathBuf,
    pub stores: Vec<StoreReport>,
    /// Largest first.
    pub sites: Vec<SiteRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserReport {
    pub id: String,
    pub name: String,
    pub rule_id: String,
    pub exe: String,
    pub running: bool,
    pub profiles: Vec<ProfileReport>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub browsers: Vec<BrowserReport>,
    /// Some store had more entries than were read.
    pub truncated: bool,
}

/// Scan limits and filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    /// Only this browser id.
    pub browser: Option<String>,
    /// Entries read per store.
    pub max_entries: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            browser: None,
            max_entries: 200_000,
        }
    }
}

/// Something a store holds for one site: the files that go together.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Found {
    site: Option<String>,
    bytes: u64,
    /// Files of one entry, or one origin folder.
    paths: Vec<PathBuf>,
    is_dir: bool,
    files: u64,
}

/// Read one store (read-only): its report and, when cleanable, what each site holds.
fn read_store(kind: StoreKind, dir: &Path, max: usize) -> (StoreReport, Vec<Found>) {
    let mut report = StoreReport {
        kind,
        path: dir.to_path_buf(),
        format: Format::Unknown,
        cleanable: false,
        entries: 0,
        bytes: 0,
        other_bytes: 0,
        damaged: 0,
        error: None,
    };
    let mut found = Vec::new();
    if !dir.is_dir() {
        return (report, found);
    }
    match kind {
        StoreKind::HttpCache if blockfile::is_blockfile(dir) => {
            report.format = Format::Blockfile;
            match blockfile::list(dir, max) {
                Ok(l) => {
                    report.damaged = l.damaged;
                    for e in l.entries {
                        found.push(Found {
                            site: site::http_cache_site(&e.key),
                            bytes: e.bytes,
                            paths: Vec::new(),
                            is_dir: false,
                            files: 0,
                        });
                    }
                }
                Err(e) => report.error = Some(e.to_string()),
            }
        }
        StoreKind::HttpCache | StoreKind::CodeCacheJs | StoreKind::CodeCacheWasm => {
            if kind == StoreKind::HttpCache && !simple::is_simple(dir) {
                return (report, found);
            }
            report.format = Format::Simple;
            report.cleanable = true;
            let key_site = if kind == StoreKind::HttpCache {
                site::http_cache_site
            } else {
                site::code_cache_site
            };
            for e in simple::list(dir, max) {
                found.push(Found {
                    site: e.key.as_deref().and_then(key_site),
                    bytes: e.bytes,
                    files: e.files.len() as u64,
                    paths: e.files,
                    is_dir: false,
                });
            }
        }
        StoreKind::CacheStorage => {
            report.format = Format::CacheStorage;
            report.cleanable = true;
            for o in cachestorage::list(dir, max as u64) {
                found.push(Found {
                    site: site::cache_storage_site(o.origin.as_deref(), o.storage_key.as_deref()),
                    bytes: o.bytes,
                    files: o.files,
                    paths: vec![o.path],
                    is_dir: true,
                });
            }
        }
        StoreKind::FirefoxCache => {
            report.format = Format::Cache2;
            report.cleanable = true;
            for e in firefox::list(dir, max) {
                found.push(Found {
                    site: e.key.as_deref().and_then(site::firefox_site),
                    bytes: e.bytes,
                    files: 1,
                    paths: vec![e.path],
                    is_dir: false,
                });
            }
        }
    }
    report.entries = found.len() as u64;
    report.bytes = found.iter().map(|f| f.bytes).sum();
    report.other_bytes = found
        .iter()
        .filter(|f| f.site.is_none())
        .map(|f| f.bytes)
        .sum();
    (report, found)
}

fn site_rows(stores: &[(StoreReport, Vec<Found>)]) -> Vec<SiteRow> {
    let mut rows: BTreeMap<String, SiteRow> = BTreeMap::new();
    for (store, found) in stores {
        for f in found {
            let Some(s) = &f.site else { continue };
            let row = rows.entry(s.clone()).or_insert_with(|| SiteRow {
                site: s.clone(),
                bytes: 0,
                entries: 0,
                cleanable_bytes: 0,
                cleanable_entries: 0,
                listed_bytes: 0,
                stores: Vec::new(),
            });
            row.bytes += f.bytes;
            row.entries += 1;
            if store.cleanable {
                row.cleanable_bytes += f.bytes;
                row.cleanable_entries += 1;
            } else {
                row.listed_bytes += f.bytes;
            }
            if !row.stores.contains(&store.kind) {
                row.stores.push(store.kind);
            }
        }
    }
    let mut out: Vec<SiteRow> = rows.into_values().collect();
    for r in &mut out {
        r.stores.sort();
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.site.cmp(&b.site)));
    out
}

/// Every browser profile's stores and sites (read-only).
#[must_use]
pub fn scan(env: &Env, opts: &ScanOptions) -> Report {
    let mut report = Report::default();
    for b in BROWSERS {
        if opts
            .browser
            .as_deref()
            .is_some_and(|id| !b.id.eq_ignore_ascii_case(id))
        {
            continue;
        }
        let profiles: Vec<ProfileReport> = profiles(env, b)
            .into_iter()
            .map(|p| {
                let stores: Vec<(StoreReport, Vec<Found>)> = p
                    .stores()
                    .into_iter()
                    .map(|(kind, dir)| read_store(kind, &dir, opts.max_entries))
                    .collect();
                if stores
                    .iter()
                    .any(|(s, _)| s.entries as usize >= opts.max_entries)
                {
                    report.truncated = true;
                }
                ProfileReport {
                    name: p.name.clone(),
                    path: p.local.clone(),
                    sites: site_rows(&stores),
                    stores: stores
                        .into_iter()
                        .map(|(s, _)| s)
                        .filter(|s| s.format != Format::Unknown)
                        .collect(),
                }
            })
            .filter(|p: &ProfileReport| p.stores.iter().any(|s| s.entries > 0))
            .collect();
        if profiles.is_empty() {
            continue;
        }
        report.browsers.push(BrowserReport {
            id: b.id.into(),
            name: b.name.into(),
            rule_id: b.rule_id.into(),
            exe: b.exe.into(),
            running: env.is_running(b.exe),
            profiles,
        });
    }
    report
}

/// What to clean: one site in one browser (one profile, or all of them).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub browser: String,
    /// Profile folder (`ProfileReport::path`); `None` = every profile.
    #[serde(default)]
    pub profile: Option<PathBuf>,
    pub site: String,
    /// Also its subdomains (`m.youtube.com` for `youtube.com`).
    #[serde(default)]
    pub subdomains: bool,
}

/// Bytes and entries removed from one kind of store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreTotal {
    pub kind: StoreKind,
    pub entries: u64,
    pub bytes: u64,
}

/// A path that was found but not planned (should never happen; reported, not acted on).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refused {
    pub path: PathBuf,
    pub reason: String,
}

/// The plan for the chosen sites, and what it does and does not cover.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitePlan {
    pub plan: CleanPlan,
    /// Cache entries (or origin folders) removed.
    pub entries: u64,
    /// Their bytes (the advisory index files come on top, see `index_files`).
    pub bytes: u64,
    pub by_store: Vec<StoreTotal>,
    /// Index files removed so the browser rebuilds them.
    pub index_files: u64,
    /// Bytes these sites keep in the blockfile HTTP cache (not cleaned).
    pub listed_only: u64,
    /// Browsers (exe) that are running: nothing is planned for them.
    pub running: Vec<String>,
    pub refused: Vec<Refused>,
}

fn plan_item(
    b: &BrowserDef,
    path: &Path,
    is_dir: bool,
    bytes: u64,
    files: u64,
) -> Option<PlanItem> {
    let meta = fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() || meta.is_dir() != is_dir {
        return None;
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    Some(PlanItem {
        path: path.to_path_buf(),
        origin: Origin::Rule {
            level: RiskLevel::Safe,
        },
        rule_id: Some(b.rule_id.into()),
        is_dir,
        bytes,
        files,
        size: if is_dir { 0 } else { meta.len() },
        mtime: if is_dir { 0 } else { mtime },
        requires_closed: vec![b.exe.into()],
        mode: DeleteMode::Quarantine,
        selected: true,
        keeper: None,
    })
}

/// Rule matcher for the browsers pack, resolved like `env`.
fn rule_matcher(env: &Env) -> Option<nfd_rules::matcher::Matcher> {
    let packs: Vec<nfd_rules::Pack> = nfd_rules::builtin_packs()
        .ok()?
        .into_iter()
        .filter(|p| p.pack.id == "browsers")
        .collect();
    Some(nfd_rules::matcher::Matcher::new(
        &packs,
        &|v: &str| env.resolve(v),
        "windows",
    ))
}

/// Plan removing the cache of `targets` (fresh read of the targeted profiles).
#[must_use]
pub fn plan(env: &Env, targets: &[Target], max_entries: usize) -> SitePlan {
    let mut out = SitePlan::default();
    let matcher = rule_matcher(env);
    let mut items: Vec<PlanItem> = Vec::new();
    let mut totals: BTreeMap<StoreKind, (u64, u64)> = BTreeMap::new();
    let mut by_browser: BTreeMap<&'static str, Vec<&Target>> = BTreeMap::new();
    for t in targets {
        if let Some(b) = browser(&t.browser) {
            by_browser.entry(b.id).or_default().push(t);
        }
    }
    for (id, wanted) in by_browser {
        let Some(b) = browser(id) else { continue };
        if env.is_running(b.exe) {
            out.running.push(b.exe.into());
            continue;
        }
        let covered = |path: &Path| {
            matcher.as_ref().is_some_and(|m| {
                m.rules_matching(path)
                    .iter()
                    .any(|(r, _)| r.id == b.rule_id && r.level == RiskLevel::Safe)
            })
        };
        for p in profiles(env, b) {
            let picks: Vec<&&Target> = wanted
                .iter()
                .filter(|t| t.profile.as_ref().is_none_or(|tp| same_path(tp, &p.local)))
                .collect();
            if picks.is_empty() {
                continue;
            }
            let wants = |s: &str| {
                picks
                    .iter()
                    .any(|t| site::site_matches(s, &t.site, t.subdomains))
            };
            for (kind, dir) in p.stores() {
                let (store, found) = read_store(kind, &dir, max_entries);
                let mine: Vec<&Found> = found
                    .iter()
                    .filter(|f| f.site.as_deref().is_some_and(wants))
                    .collect();
                if !store.cleanable {
                    out.listed_only += mine.iter().map(|f| f.bytes).sum::<u64>();
                    continue;
                }
                let mut removed_any = false;
                for f in mine {
                    let mut entry_items = Vec::new();
                    let mut ok = true;
                    for path in &f.paths {
                        let reason = if !cache_path_allowed(&p, path) {
                            Some("outside-cache-store")
                        } else if !covered(path) {
                            Some("not-covered-by-rule")
                        } else {
                            None
                        };
                        if let Some(reason) = reason {
                            out.refused.push(Refused {
                                path: path.clone(),
                                reason: reason.into(),
                            });
                            ok = false;
                            continue;
                        }
                        let bytes = if f.is_dir {
                            f.bytes
                        } else {
                            fs::symlink_metadata(path).map_or(0, |m| m.len())
                        };
                        let files = if f.is_dir { f.files } else { 1 };
                        match plan_item(b, path, f.is_dir, bytes, files) {
                            Some(i) => entry_items.push(i),
                            None => ok = false,
                        }
                    }
                    // an entry goes as a whole or not at all
                    if ok && !entry_items.is_empty() {
                        items.extend(entry_items);
                        let t = totals.entry(kind).or_default();
                        t.0 += 1;
                        t.1 += f.bytes;
                        out.entries += 1;
                        out.bytes += f.bytes;
                        removed_any = true;
                    }
                }
                if removed_any {
                    let index = match kind {
                        StoreKind::FirefoxCache => dir
                            .parent()
                            .map(|c| c.join("index"))
                            .filter(|i| i.is_file()),
                        StoreKind::CacheStorage => None,
                        _ => simple::index_file(&dir),
                    };
                    if let Some(index) = index {
                        if cache_path_allowed(&p, &index) && covered(&index) {
                            let len = fs::symlink_metadata(&index).map_or(0, |m| m.len());
                            if let Some(i) = plan_item(b, &index, false, len, 1) {
                                items.push(i);
                                out.index_files += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    out.by_store = totals
        .into_iter()
        .map(|(kind, (entries, bytes))| StoreTotal {
            kind,
            entries,
            bytes,
        })
        .collect();
    out.plan = plan::build(
        items,
        PlanOptions {
            expert: false,
            include_caution: false,
            permanent: false,
        },
    );
    out
}

fn same_path(a: &Path, b: &Path) -> bool {
    let n = |p: &Path| {
        p.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    n(a) == n(b)
}

#[cfg(test)]
mod tests;
