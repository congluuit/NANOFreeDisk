//! Everything the engine consults, compiled once for this machine: rule packs, the
//! Folder Encyclopedia, the SafetyGate's protected locations, build-output markers,
//! package-store commands.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use nfd_core::RiskLevel;
use nfd_core::safety::{
    AGENT_HISTORY_TEMPLATES, CONTAINER_TEMPLATES, PROTECTED_TEMPLATES, SafetyGate,
    USER_DATA_TEMPLATES,
};
use nfd_devtools::artifacts::{self, Marker};
use nfd_devtools::stores::{self, Undo};
use nfd_rules::encyclopedia::{self, Article};
use nfd_rules::matcher::Matcher;
use nfd_rules::{Pack, Rule, expand};

use crate::text::{self, Text};
use crate::{join_key, key_of, within};

/// Articles describing something only the proper system tool may shrink.
pub(crate) const GUIDE_ARTICLES: &[&str] = &[
    "winsxs",
    "pagefile",
    "hiberfil",
    "system-volume-information",
    "windows-old",
    "software-distribution",
    "recycle-bin",
    "package-cache",
    "appdata-packages",
    "docker-wsl",
    "windows-installer",
];

/// Articles describing something to keep (a project's git history).
pub(crate) const KEEP_ARTICLES: &[&str] = &["git-folder"];

/// Folder names that hold the user's own media (lower case).
pub(crate) const PERSONAL_NAMES: &[&str] = &[
    "pictures",
    "picture",
    "photos",
    "photo",
    "videos",
    "video",
    "music",
    "dcim",
    "camera roll",
    "ảnh",
    "hình ảnh",
    "nhạc",
];

/// A rule as the engine needs it.
#[derive(Debug, Clone)]
pub(crate) struct RuleInfo {
    pub rule: Rule,
    pub impact: Arc<Text>,
    pub untouched: Arc<Text>,
}

/// The compiled knowledge (build it once, use it for every scan).
#[derive(Debug)]
pub struct Knowledge {
    pub(crate) rules: Matcher,
    pub(crate) rule_info: HashMap<String, RuleInfo>,
    /// Rule indices ([`Matcher::bases`]) by the [`key_of`] of their base folder.
    pub(crate) base_rules: HashMap<String, Vec<usize>>,
    /// Rule index → 🔒 Keep rule.
    pub(crate) rule_keep: Vec<bool>,
    pub(crate) encyclopedia: encyclopedia::Matcher,
    pub(crate) ency_names: HashSet<String>,
    pub(crate) ency_paths: HashSet<String>,
    pub(crate) ency_volume: HashSet<String>,
    pub(crate) gate: SafetyGate,
    /// Protected templates and all their ancestors: only there may the gate refuse
    /// something by location.
    pub(crate) interesting: HashSet<String>,
    /// Parents of templates whose last component has wildcards.
    pub(crate) glob_parents: HashSet<String>,
    /// Pictures / Videos / Music of the user.
    pub(crate) personal: HashSet<String>,
    pub(crate) marker_files: Vec<String>,
    pub(crate) marker_exts: Vec<String>,
    pub(crate) marker_dirs: Vec<String>,
    /// (store folder key, command id, runnable).
    pub(crate) stores: Vec<(String, &'static str, bool)>,
}

/// `key` and every ancestor of it.
fn with_ancestors(key: &str, out: &mut HashSet<String>) {
    let mut cur = key.to_owned();
    loop {
        out.insert(cur.clone());
        match cur.rsplit_once('/') {
            Some((parent, _)) if !parent.is_empty() => cur = parent.to_owned(),
            Some(_) if cur.len() > 1 => {
                out.insert("/".to_owned());
                break;
            }
            _ => break,
        }
    }
}

impl Knowledge {
    /// Compile `packs` and the encyclopedia `articles` for this machine (`platform` =
    /// `windows`, `linux`, `macos`; `resolve` = `nfd_platform::Platform::resolver`).
    #[must_use]
    pub fn new(
        packs: &[Pack],
        articles: Vec<Article>,
        resolve: &impl Fn(&str) -> Option<String>,
        platform: &str,
    ) -> Self {
        let rules = Matcher::new(packs, resolve, platform);
        let mut rule_info = HashMap::new();
        for pack in packs {
            for r in &pack.rules {
                if !r.platform.iter().any(|p| p == platform) {
                    continue;
                }
                rule_info.insert(
                    r.id.clone(),
                    RuleInfo {
                        rule: r.clone(),
                        impact: Arc::new(Text::new(
                            r.impact.clone(),
                            r.impact_vi.clone().unwrap_or_else(|| r.impact.clone()),
                        )),
                        untouched: text::rule_untouched(r.min_age_days),
                    },
                );
            }
        }
        let mut base_rules: HashMap<String, Vec<usize>> = HashMap::new();
        let mut rule_keep = Vec::new();
        for (base, i) in rules.bases() {
            let list = base_rules
                .entry(key_of(&base.to_string_lossy()))
                .or_default();
            if !list.contains(&i) {
                list.push(i);
            }
            if rule_keep.len() <= i {
                rule_keep.resize(i + 1, false);
            }
            rule_keep[i] = rules
                .rule(i)
                .is_some_and(|r| r.level == RiskLevel::Protected);
        }
        let encyclopedia = encyclopedia::Matcher::new(articles, resolve);
        let mut ency_names = HashSet::new();
        let mut ency_paths = HashSet::new();
        let mut ency_volume = HashSet::new();
        for a in encyclopedia.articles() {
            ency_names.extend(a.names.iter().map(|n| n.to_lowercase()));
            ency_volume.extend(a.volume_paths.iter().map(|n| n.to_lowercase()));
            ency_paths.extend(
                a.paths
                    .iter()
                    .filter_map(|p| expand(p, resolve))
                    .map(|p| key_of(&p)),
            );
        }
        let mut interesting = HashSet::new();
        let mut glob_parents = HashSet::new();
        for t in PROTECTED_TEMPLATES
            .iter()
            .chain(AGENT_HISTORY_TEMPLATES)
            .chain(CONTAINER_TEMPLATES)
            .chain(USER_DATA_TEMPLATES)
        {
            let Some(p) = expand(t, resolve) else {
                continue;
            };
            if p.trim().is_empty() {
                continue;
            }
            let k = key_of(&p);
            match k.rsplit_once('/') {
                Some((parent, last)) if last.contains(['*', '?']) => {
                    glob_parents.insert(parent.to_owned());
                    with_ancestors(parent, &mut interesting);
                }
                _ => with_ancestors(&k, &mut interesting),
            }
        }
        let personal = ["${HOME}/Pictures", "${HOME}/Videos", "${HOME}/Music"]
            .iter()
            .filter_map(|t| expand(t, resolve))
            .map(|p| key_of(&p))
            .collect();
        let mut marker_files = Vec::new();
        let mut marker_exts = Vec::new();
        let mut marker_dirs = Vec::new();
        for policy in artifacts::POLICIES {
            for m in policy.markers {
                match m {
                    Marker::File(f) => marker_files.push(f.to_lowercase()),
                    Marker::Ext(e) => marker_exts.push(e.to_lowercase()),
                    Marker::Path(p) => {
                        marker_dirs.push(p.split('/').next().unwrap_or_default().to_lowercase())
                    }
                }
            }
        }
        marker_dirs.extend(artifacts::ANYWHERE.iter().map(|a| a.to_lowercase()));
        let stores = stores::COMMANDS
            .iter()
            .flat_map(|c| {
                c.store.iter().filter_map(move |t| {
                    expand(t, resolve).map(|p| {
                        (
                            key_of(&p),
                            c.id,
                            c.undo == Undo::Redownload && c.level != RiskLevel::Advanced,
                        )
                    })
                })
            })
            .collect();
        Self {
            gate: rules.gate().clone(),
            rules,
            rule_info,
            base_rules,
            rule_keep,
            encyclopedia,
            ency_names,
            ency_paths,
            ency_volume,
            interesting,
            glob_parents,
            personal,
            marker_files,
            marker_exts,
            marker_dirs,
            stores,
        }
    }

    /// The built-in rule packs and encyclopedia on this machine.
    ///
    /// # Errors
    /// The built-in packs do not parse (a build problem).
    pub fn builtin(resolve: &impl Fn(&str) -> Option<String>) -> Result<Self, nfd_rules::Error> {
        let packs = nfd_rules::builtin_packs()?;
        Ok(Self::new(
            &packs,
            encyclopedia::builtin(),
            resolve,
            current_platform(),
        ))
    }

    /// The rule packs this knowledge was made from (for actions).
    #[must_use]
    pub const fn matcher(&self) -> &Matcher {
        &self.rules
    }

    /// Rules whose base folder holds `key` (or is it): the only ones that may match
    /// it or anything below it.
    pub(crate) fn rules_for(&self, key: &str) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .base_rules
            .iter()
            .filter(|(b, _)| within(key, b))
            .flat_map(|(_, r)| r.iter().copied())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The official store command covering `key`, if any: (id, runnable).
    pub(crate) fn store_for(&self, key: &str) -> Option<(&'static str, bool)> {
        self.stores
            .iter()
            .find(|(k, _, _)| within(key, k))
            .map(|(_, id, ok)| (*id, *ok))
    }

    /// The impact text of an article in both languages (its first paragraph).
    pub(crate) fn article_texts(a: &Article) -> (Arc<Text>, Arc<Text>) {
        let get = |lang: &str| a.text.get(lang).or_else(|| a.text.get("en"));
        let title = |lang: &str| get(lang).map(|t| t.title.clone()).unwrap_or_default();
        let body = |lang: &str| {
            get(lang)
                .map(|t| text::first_paragraph(&t.body))
                .unwrap_or_default()
        };
        (
            Arc::new(Text::new(title("en"), title("vi"))),
            Arc::new(Text::new(body("en"), body("vi"))),
        )
    }

    /// Exact encyclopedia hit for `path` (whose key is `key`, name `name`), using the
    /// prefilter sets first.
    pub(crate) fn article_at(
        &self,
        path: &Path,
        key: &str,
        name: &str,
        parent_is_volume_root: bool,
    ) -> Option<&Article> {
        let candidate = self.ency_names.contains(name)
            || self.ency_paths.contains(key)
            || (parent_is_volume_root && self.ency_volume.contains(name));
        if !candidate {
            return None;
        }
        self.encyclopedia.at(path, &|p: &Path| p.exists())
    }

    /// `join_key` re-exported for the classifier.
    pub(crate) fn child_key(parent: &str, name: &str) -> String {
        join_key(parent, name)
    }
}

/// `windows`, `macos` or `linux`: the platform rule packs are filtered by.
#[must_use]
pub const fn current_platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}
