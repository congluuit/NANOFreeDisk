//! Unknown developer caches (PLAN §4.3, DEVTOOL_CACHE_CATALOG §6).
//!
//! Looks one level below `%APPDATA%`, `%LOCALAPPDATA%`, `~/.cache`, `~/.config`,
//! `~/.local/share` and at the dot-folders of the user profile for folders that no
//! rule mentions yet but look like caches: a dot-folder, or a folder with a
//! `cache` / `logs` / `sessions` / `models` / … sub-folder (two levels deep). They are
//! reported as 🟡 *unverified* with their size and a link to propose a rule — never
//! cleaned, never part of "Clean safe".

use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::safety::{Origin, SafetyGate};
use nfd_rules::{Pack, expand};
use rayon::prelude::*;
use serde::Serialize;

use crate::fsutil::{self, Totals};
use crate::{literal_base, norm, within};

/// Sub-folder names that suggest a cache, a log or session store.
pub const SIGNAL_NAMES: &[&str] = &[
    "cache",
    "caches",
    "code cache",
    "gpucache",
    "crashpad",
    "tmp",
    "temp",
    "logs",
    "log",
    "sessions",
    "history",
    "index",
    "blobs",
    "models",
];

/// Folders below the roots that belong to Windows or are too generic to judge
/// (lower case).
const IGNORE: &[&str] = &[
    "microsoft",
    "packages",
    "temp",
    "tmp",
    "programs",
    "comms",
    "connecteddevicesplatform",
    "crashdumps",
    "d3dscache",
    "publishers",
    "virtualstore",
    "history",
    "application data",
    "microsoft_corporation",
    "windows",
    "nanofreedisk",
    "vn.io.nanodev.nanofreedisk",
    ".cache",
    ".config",
    ".local",
    "appdata",
];

/// Where a candidate was found (for the UI and the rule template).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Root {
    Home,
    AppData,
    LocalAppData,
    XdgCache,
    XdgConfig,
    XdgData,
}

impl Root {
    /// The variable form used in rule packs and issue reports (no user name).
    #[must_use]
    pub const fn template(self) -> &'static str {
        match self {
            Self::Home => "${HOME}",
            Self::AppData => "${APPDATA}",
            Self::LocalAppData => "${LOCALAPPDATA}",
            Self::XdgCache => "${HOME}/.cache",
            Self::XdgConfig => "${HOME}/.config",
            Self::XdgData => "${HOME}/.local/share",
        }
    }
}

/// A cache-like folder without a rule.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnknownFolder {
    pub path: PathBuf,
    pub name: String,
    pub root: Root,
    /// The folder as a rule-pack template (`${LOCALAPPDATA}/Foo`).
    pub template: String,
    /// Always 🟡 Caution: unverified.
    pub level: RiskLevel,
    #[serde(flatten)]
    pub totals: Totals,
    /// Why it looks like a cache: `dot-folder` or the signal sub-folders found
    /// (`cache`, `Foo/logs`, …).
    pub signals: Vec<String>,
    /// Pre-filled "Cache rule proposal" issue on GitHub.
    pub propose_url: String,
}

/// Options of [`find`].
#[derive(Debug, Clone, Copy)]
pub struct UnknownOptions {
    /// Smallest folder reported (bytes on disk).
    pub min_bytes: u64,
    /// Entries visited per folder before its size is reported as "at least".
    pub max_entries: u64,
}

impl Default for UnknownOptions {
    fn default() -> Self {
        Self {
            min_bytes: 50 << 20,
            max_entries: 300_000,
        }
    }
}

/// Every rule path of `packs` as a normalized literal base.
fn known_bases(packs: &[Pack], resolve: &impl Fn(&str) -> Option<String>) -> Vec<String> {
    packs
        .iter()
        .flat_map(|p| &p.rules)
        .flat_map(|r| r.paths.iter().chain(&r.exclude))
        .filter_map(|p| expand(p, resolve))
        .map(|p| literal_base(&p))
        .filter(|b| !b.is_empty())
        .collect()
}

/// A folder is known when a rule path lies inside it or it lies inside a rule path.
fn is_known(folder: &str, bases: &[String]) -> bool {
    bases.iter().any(|b| within(b, folder) || within(folder, b))
}

/// Signal sub-folders of `dir` (children and grandchildren).
fn signals(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for child in fsutil::subdirs(dir) {
        let lower = child.to_lowercase();
        if SIGNAL_NAMES.contains(&lower.as_str()) {
            out.push(child.clone());
            continue;
        }
        for grand in fsutil::subdirs(&dir.join(&child)) {
            if SIGNAL_NAMES.contains(&grand.to_lowercase().as_str()) {
                out.push(format!("{child}/{grand}"));
            }
        }
    }
    out.sort();
    out.truncate(8);
    out
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A new-issue link for the "Cache rule proposal" form, with the folder as a template
/// (no user name), its size and its signal sub-folders.
#[must_use]
pub fn propose_url(template: &str, name: &str, totals: &Totals, signals: &[String]) -> String {
    let gib = totals.bytes as f64 / f64::from(1u32 << 30);
    let paths = format!(
        "{template}\n(size {gib:.2} GiB, {} files{}; sub-folders: {})",
        totals.files,
        if totals.truncated { "+" } else { "" },
        if signals.is_empty() {
            "-".to_owned()
        } else {
            signals.join(", ")
        }
    );
    format!(
        "https://github.com/congluuit/NANOFreeDisk/issues/new?template=rule_proposal.yml&title={}&paths={}",
        encode(&format!("[rule] {name}")),
        encode(&paths)
    )
}

/// `/`-separated resolver output in the platform's own form.
fn native(path: &str) -> String {
    if cfg!(windows) {
        path.replace('/', "\\")
    } else {
        path.to_owned()
    }
}

/// Find unknown cache-like folders. `packs` should be every pack (a folder any rule
/// mentions is known); `gate` keeps out what a rule could never touch.
#[must_use]
pub fn find(
    packs: &[Pack],
    resolve: &impl Fn(&str) -> Option<String>,
    gate: &SafetyGate,
    opts: UnknownOptions,
) -> Vec<UnknownFolder> {
    let bases = known_bases(packs, resolve);
    let roots: Vec<(Root, &str)> = vec![
        (Root::Home, "${HOME}"),
        (Root::AppData, "${APPDATA}"),
        (Root::LocalAppData, "${LOCALAPPDATA}"),
        (Root::XdgCache, "${HOME}/.cache"),
        (Root::XdgConfig, "${HOME}/.config"),
        (Root::XdgData, "${HOME}/.local/share"),
    ];
    let mut todo: Vec<(Root, PathBuf, String, Vec<String>)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (root, template) in roots {
        let Some(dir) = expand(template, resolve) else {
            continue;
        };
        let dir = PathBuf::from(native(&dir));
        for name in fsutil::subdirs(&dir) {
            let lower = name.to_lowercase();
            if IGNORE.contains(&lower.as_str()) {
                continue;
            }
            let dot = name.starts_with('.');
            // the profile itself holds user folders (Documents, source, …): only
            // its dot-folders are looked at
            if root == Root::Home && !dot {
                continue;
            }
            let path = dir.join(&name);
            let key = norm(&path.to_string_lossy());
            if seen.contains(&key) || is_known(&key, &bases) {
                continue;
            }
            let origin = Origin::Rule {
                level: RiskLevel::Caution,
            };
            if gate.check_path(&path, origin).is_err() {
                continue;
            }
            let mut sig = signals(&path);
            if dot {
                sig.insert(0, "dot-folder".into());
            }
            if sig.is_empty() {
                continue;
            }
            seen.push(key);
            todo.push((root, path, name, sig));
        }
    }
    let mut out: Vec<UnknownFolder> = todo
        .into_par_iter()
        .filter_map(|(root, path, name, signals)| {
            let totals = fsutil::totals(&path, opts.max_entries);
            if totals.bytes < opts.min_bytes {
                return None;
            }
            let template = format!("{}/{name}", root.template());
            Some(UnknownFolder {
                propose_url: propose_url(&template, &name, &totals, &signals),
                path,
                name,
                root,
                template,
                level: RiskLevel::Caution,
                totals,
                signals,
            })
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.totals.bytes));
    out
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, vec![b'x'; bytes]).expect("write");
    }

    fn pack(paths: &str) -> Pack {
        nfd_rules::parse_pack(
            &format!(
                r#"
[pack]
id = "t"
name = "t"
version = "0"

[[rule]]
id = "known.cache"
vendor = "K"
app = "Known"
category = "x"
platform = ["windows", "linux", "macos"]
paths = [{paths}]
level = "safe"
regenerates = true
impact = "Rebuilt automatically on next start."
verified_on = 2026-10-16
"#
            ),
            None,
        )
        .expect("pack")
    }

    #[test]
    fn finds_cache_like_folders_without_a_rule() {
        let dir = tempfile::tempdir().expect("tmp");
        let home = dir.path().join("home");
        let local = home.join("AppData/Local");
        let roaming = home.join("AppData/Roaming");
        write(&local.join("Mystery/cache/blob.bin"), 4096);
        write(&local.join("Vendor/Tool/logs/a.log"), 4096);
        write(&local.join("Known/Cache/x.bin"), 4096); // has a rule
        write(&local.join("Plain/data.bin"), 4096); // no signal
        write(&local.join("Microsoft/Cache/x.bin"), 4096); // ignored
        write(&home.join(".weird-tool/stuff.bin"), 4096); // dot-folder
        write(&home.join("Projects/cache/x.bin"), 4096); // not a dot-folder
        write(&home.join(".ssh/id_ed25519"), 4096); // protected
        write(&roaming.join("Tiny/logs/x"), 1);
        let h = home.to_string_lossy().replace('\\', "/");
        let resolve = move |v: &str| match v {
            "HOME" | "USERPROFILE" => Some(h.clone()),
            "LOCALAPPDATA" => Some(format!("{h}/AppData/Local")),
            "APPDATA" => Some(format!("{h}/AppData/Roaming")),
            _ => None,
        };
        let gate = SafetyGate::new(&resolve);
        let packs = [pack(r#""${LOCALAPPDATA}/Known/Cache/**""#)];
        let opts = UnknownOptions {
            min_bytes: 100,
            max_entries: 1000,
        };
        let found = find(&packs, &resolve, &gate, opts);
        let mut names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, [".weird-tool", "Mystery", "Vendor"]);
        let vendor = found.iter().find(|f| f.name == "Vendor").expect("vendor");
        assert_eq!(vendor.signals, ["Tool/logs"]);
        assert_eq!(vendor.template, "${LOCALAPPDATA}/Vendor");
        assert_eq!(vendor.level, RiskLevel::Caution);
        assert!(!vendor.propose_url.contains(&*dir.path().to_string_lossy()));
        assert!(vendor.propose_url.contains("template=rule_proposal.yml"));
        let dot = found.iter().find(|f| f.name == ".weird-tool").expect("dot");
        assert_eq!(dot.signals, ["dot-folder"]);
    }

    #[test]
    fn literal_base_stops_at_the_first_glob() {
        assert_eq!(literal_base("C:/U/x/A/*/Cache/**"), "c:/u/x/a");
        assert_eq!(literal_base("C:/U/x/A/f.log"), "c:/u/x/a/f.log");
        assert!(is_known("c:/u/x/a", &["c:/u/x/a/b".into()]));
        assert!(is_known("c:/u/x/a/b", &["c:/u/x/a".into()]));
        assert!(!is_known("c:/u/x/ab", &["c:/u/x/a".into()]));
    }
}
