//! The top-down pass: one walk over the scan arena (pre-order, so a folder is always
//! classified before what is inside it) that decides which entries start a unit.
//!
//! Inside a unit only *stricter* claims may carve out a part: conversation history,
//! 🔒 Keep rules and protected locations inside a cache folder stay; inside a kept
//! folder (Windows) a rule may still clean what the SafetyGate lets rules clean
//! (`Windows\Temp`); inside an agent worktree the build output is its own unit.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use nfd_core::RiskLevel;
use nfd_core::insights::{self, DOWNLOADS_DIR, OldLargeOptions};
use nfd_core::model::{ChildIndex, Entry, EntryId, EntryKind};
use nfd_core::query::Matcher as FileMatcher;
use nfd_core::safety::{AGENT_HISTORY_TEMPLATES, Blocked, Origin, agent_history_entry};
use nfd_core::scanner::ScanTree;
use nfd_devtools::ai_leftovers::catalog;
use nfd_devtools::artifacts::{self, Site};
use nfd_rules::Rule;

use crate::knowledge::{GUIDE_ARTICLES, KEEP_ARTICLES, Knowledge, PERSONAL_NAMES};
use crate::text;
use crate::{Action, Advice, NONE, Reason, Unit, Verdict, key_of};

/// What an inherited unit lets the classifier look for below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    /// Nothing claims it.
    Open,
    /// The user's media: only stricter claims and rules.
    Personal,
    /// A unit that may be cleaned (rule, build output, leftover, old file…).
    Clean,
    /// An agent worktree not checked with git yet.
    Worktree,
    /// Kept by Windows / a protected location: only rules (as the SafetyGate allows
    /// them) and guide articles carve parts out.
    Keep,
    /// Kept for good (conversation history, 🔒 Keep rules, a project's `.git`).
    Kept,
    /// Explained only: rules may still clean parts the gate allows.
    Guide,
}

pub(crate) fn class_of(u: Option<&Unit>) -> Class {
    match u {
        None => Class::Open,
        Some(u) => match (u.verdict, u.reason) {
            (_, Reason::Personal) => Class::Personal,
            (_, Reason::AgentWorktree) if u.blocked.as_deref() == Some("git-check") => {
                Class::Worktree
            }
            (_, Reason::Rule) => Class::Clean,
            (_, Reason::System | Reason::Programs | Reason::Protected) => Class::Keep,
            (Verdict::Keep, _) => Class::Kept,
            (Verdict::Guide, _) => Class::Guide,
            _ => Class::Clean,
        },
    }
}

/// What a child needs to know about its folder.
#[derive(Debug, Clone)]
struct Frame {
    id: u32,
    key: String,
    unit: u32,
    class: Class,
    /// Rules whose base folder holds this folder.
    rules: Rc<Vec<usize>>,
    interesting: bool,
    glob_parent: bool,
    volume_root: bool,
    downloads: bool,
    /// `<project>/.claude/worktrees`
    worktrees: bool,
}

/// `c:` / `/` / `//server/share`: the root of a volume.
fn is_volume_root(key: &str) -> bool {
    key == "/"
        || (key.len() == 2 && key.ends_with(':'))
        || (key.starts_with("//") && key[2..].split('/').filter(|s| !s.is_empty()).count() == 2)
}

fn ends_with_ignore_case(name: &str, suffix: &str) -> bool {
    name.len() > suffix.len()
        && name.as_bytes()[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
}

struct Walker<'a> {
    tree: &'a ScanTree,
    children: &'a ChildIndex,
    k: &'a Knowledge,
    now: i64,
    stale: FileMatcher<'a>,
    installers: FileMatcher<'a>,
    units: Vec<Unit>,
    unit_of: Vec<u32>,
    /// Build-output folders found among the children of a visited folder.
    sites: HashMap<u32, Site>,
    agent_projects: Vec<PathBuf>,
}

/// Classify every entry of `tree` (`children` / `ends` built from it) with what `k`
/// knows; `now` is the time old files and rule ages are measured against.
#[must_use]
pub fn advise(
    tree: &ScanTree,
    children: &ChildIndex,
    ends: &[u32],
    k: &Knowledge,
    now: i64,
) -> Advice {
    let _ = ends;
    let opts = OldLargeOptions::at(now);
    let [_, stale_f, inst_f] = insights::old_large_filters(&opts);
    let mut w = Walker {
        tree,
        children,
        k,
        now,
        stale: stale_f.matcher(),
        installers: inst_f.matcher(),
        units: Vec::new(),
        unit_of: vec![NONE; tree.entries.len()],
        sites: HashMap::new(),
        agent_projects: Vec::new(),
    };
    w.walk();
    let mut advice = Advice {
        units: w.units,
        unit_of: w.unit_of,
        computed_at: now,
        agent_projects: w.agent_projects,
        ..Advice::default()
    };
    advice.compute_totals(tree);
    advice
}

/// Copyable facts of the parent frame.
#[derive(Debug, Clone, Copy)]
struct Up {
    id: u32,
    unit: u32,
    class: Class,
    interesting: bool,
    glob_parent: bool,
    volume_root: bool,
    downloads: bool,
    worktrees: bool,
}

impl Walker<'_> {
    fn walk(&mut self) {
        let tree = self.tree;
        let mut stack: Vec<Frame> = Vec::new();
        let root_key = key_of(&tree.root.to_string_lossy());
        for i in 0..tree.entries.len() {
            let e = &tree.entries[i];
            if e.removed {
                continue;
            }
            let (up, key, rules) = match e.parent {
                None => (
                    None,
                    Some(root_key.clone()),
                    Rc::new(self.k.rules_for(&root_key)),
                ),
                Some(p) => {
                    while stack.last().is_some_and(|f| f.id != p.0) {
                        stack.pop();
                    }
                    let Some(f) = stack.last() else {
                        continue;
                    };
                    let up = Up {
                        id: f.id,
                        unit: f.unit,
                        class: f.class,
                        interesting: f.interesting,
                        glob_parent: f.glob_parent,
                        volume_root: f.volume_root,
                        downloads: f.downloads,
                        worktrees: f.worktrees,
                    };
                    let is_dir = e.kind == EntryKind::Dir;
                    let needs_key = is_dir
                        || up.interesting
                        || up.glob_parent
                        || up.volume_root
                        || !f.rules.is_empty();
                    let key = needs_key.then(|| Knowledge::child_key(&f.key, &e.name));
                    let rules = match key.as_deref().and_then(|k| self.k.base_rules.get(k)) {
                        Some(more) => {
                            let mut r: Vec<usize> = f.rules.as_ref().clone();
                            for i in more {
                                if !r.contains(i) {
                                    r.push(*i);
                                }
                            }
                            Rc::new(r)
                        }
                        None => f.rules.clone(),
                    };
                    (Some(up), key, rules)
                }
            };
            let id = u32::try_from(i).unwrap_or(NONE);
            let own = self.classify(id, e, up, key.as_deref(), &rules);
            let unit = match own {
                Some(u) => {
                    self.units.push(u);
                    u32::try_from(self.units.len() - 1).unwrap_or(NONE)
                }
                None => up.map_or(NONE, |p| p.unit),
            };
            self.unit_of[i] = unit;
            if e.kind != EntryKind::Dir {
                continue;
            }
            let key = key.unwrap_or_default();
            let class = class_of((unit != NONE).then(|| &self.units[unit as usize]));
            let name = e.name.to_lowercase();
            let parent_name = up.map(|p| self.tree.entries[p.id as usize].name.to_lowercase());
            let frame = Frame {
                id,
                unit,
                class,
                rules,
                interesting: self.k.interesting.contains(&key),
                glob_parent: self.k.glob_parents.contains(&key),
                volume_root: up.is_none() && is_volume_root(&key),
                downloads: up.is_some_and(|p| p.downloads) || name == DOWNLOADS_DIR,
                worktrees: name == "worktrees" && parent_name.as_deref() == Some(".claude"),
                key,
            };
            if matches!(class, Class::Open | Class::Worktree) {
                for (eid, site) in self.sites_of(id) {
                    self.sites.insert(eid, site);
                }
            }
            stack.push(frame);
        }
    }

    /// The unit `e` starts, if any.
    #[allow(clippy::too_many_lines)]
    fn classify(
        &mut self,
        id: u32,
        e: &Entry,
        up: Option<Up>,
        key: Option<&str>,
        rules: &[usize],
    ) -> Option<Unit> {
        if matches!(
            e.kind,
            EntryKind::Symlink | EntryKind::Junction | EntryKind::MountPoint
        ) {
            return None;
        }
        let class = up.map_or(Class::Open, |p| p.class);
        let in_history = up
            .map(|p| p.unit)
            .filter(|&u| u != NONE)
            .is_some_and(|u| self.units[u as usize].reason == Reason::AgentHistory);
        let is_dir = e.kind == EntryKind::Dir;
        let name_l = e.name.to_lowercase();
        let parent_name = up.map(|p| self.tree.entries[p.id as usize].name.to_string());
        let mut path: Option<PathBuf> = None;
        let mut path_of = |tree: &ScanTree| -> PathBuf {
            path.get_or_insert_with(|| tree.path(EntryId(id))).clone()
        };

        // 1. conversation history, wherever it is
        if !in_history {
            if let Some(hit) = agent_history_entry(parent_name.as_deref(), &e.name) {
                return Some(history_unit(id, hit, &e.name));
            }
        }
        let volume_parent = up.is_some_and(|p| p.volume_root);
        // 2. articles of things only a system tool may shrink, or to keep
        let mut caution_article = None;
        if !matches!(class, Class::Guide | Class::Kept) {
            if let Some(key) = key {
                let p = path_of(self.tree);
                if let Some(a) = self.k.article_at(&p, key, &name_l, volume_parent) {
                    let (title, impact) = Knowledge::article_texts(a);
                    // a guide / keep article on something the SafetyGate refuses
                    // (WinSxS, pagefile…) is a hard protection, else a judgment
                    let hard = self.k.gate.check_path(&p, Origin::Manual).is_err_and(|b| {
                        matches!(
                            b,
                            Blocked::Protected { .. } | Blocked::SystemDir | Blocked::VolumeRoot
                        )
                    });
                    if GUIDE_ARTICLES.contains(&a.id.as_str()) {
                        return Some(Unit {
                            hard,
                            title: Some(title),
                            untouched: Some(text::guide_untouched()),
                            ..unit(id, Verdict::Guide, Reason::Explained, Action::Guide, impact)
                                .source(&a.id)
                                .level(a.risk)
                        });
                    }
                    if KEEP_ARTICLES.contains(&a.id.as_str()) {
                        return Some(Unit {
                            hard,
                            title: Some(title),
                            ..unit(id, Verdict::Keep, Reason::Explained, Action::None, impact)
                                .source(&a.id)
                                .level(a.risk)
                        });
                    }
                    if a.rules.is_empty() {
                        caution_article = Some((a.id.clone(), a.risk, title, impact));
                    }
                }
            }
        }
        // 3. protected locations of the SafetyGate
        if matches!(
            class,
            Class::Open | Class::Personal | Class::Clean | Class::Worktree
        ) {
            if let Some(key) = key {
                let relevant = up.is_none()
                    || self.k.interesting.contains(key)
                    || up.is_some_and(|p| p.glob_parent || p.volume_root);
                if relevant {
                    match self.k.gate.check_path(&path_of(self.tree), Origin::Manual) {
                        Err(Blocked::Protected { template }) => {
                            return Some(protected_unit(id, &template));
                        }
                        Err(Blocked::SystemDir) => {
                            return Some(Unit {
                                hard: true,
                                ..unit(
                                    id,
                                    Verdict::Keep,
                                    Reason::System,
                                    Action::None,
                                    text::system(),
                                )
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
        // 4. rules: every rule where nothing claims it yet (or kept / explained only),
        //    🔒 Keep rules inside cleanable units
        let in_zone = !rules.is_empty();
        if let Some(key) = key.filter(|_| in_zone) {
            // inside a cleanable unit only 🔒 Keep rules carve parts out; inside a kept
            // or explained-only one only cleanup rules (a 🔒 rule would say the same)
            let keep_only = matches!(class, Class::Clean | Class::Worktree);
            let clean_only = matches!(class, Class::Keep | Class::Guide);
            let is_keep = |i: &usize| self.k.rule_keep.get(*i).copied().unwrap_or(false);
            let among: Vec<usize> = match class {
                Class::Open | Class::Personal => rules.to_vec(),
                _ if keep_only => rules.iter().copied().filter(is_keep).collect(),
                _ if clean_only => rules.iter().copied().filter(|i| !is_keep(i)).collect(),
                _ => Vec::new(),
            };
            if !among.is_empty() {
                if let Some(u) = self.rule_unit(id, e, &path_of(self.tree), key, &among, keep_only)
                {
                    return Some(u);
                }
            }
        }
        if matches!(
            class,
            Class::Keep | Class::Kept | Class::Guide | Class::Clean
        ) {
            return None;
        }
        // 5. build output of a project (marker files next to it)
        if let Some(site) = self.sites.remove(&id) {
            let level = artifacts::level_for(&site, e.on_disk);
            let verdict = Verdict::of_level(level);
            let blocked = (class == Class::Worktree).then(|| "git-check".to_owned());
            return Some(Unit {
                selected: verdict == Verdict::Safe && blocked.is_none(),
                blocked,
                untouched: Some(text::build_output_untouched()),
                ..unit(
                    id,
                    verdict,
                    Reason::BuildOutput,
                    Action::Quarantine,
                    text::build_output(site.kind),
                )
                .source(site.kind)
                .level(level)
            });
        }
        if class != Class::Open {
            return None;
        }
        // 6. an agent worktree (`<project>/.claude/worktrees/<name>`), until git says more
        if is_dir && up.is_some_and(|p| p.worktrees) {
            let p = path_of(self.tree);
            if let Some(project) = p.parent().and_then(Path::parent).and_then(Path::parent) {
                if !self.agent_projects.iter().any(|x| x == project) {
                    self.agent_projects.push(project.to_path_buf());
                }
            }
            return Some(Unit {
                blocked: Some("git-check".to_owned()),
                app: Some("Claude Code".to_owned()),
                untouched: Some(text::worktree_untouched()),
                ..unit(
                    id,
                    Verdict::Caution,
                    Reason::AgentWorktree,
                    Action::AiLeftovers,
                    text::worktree_static(),
                )
                .source("claude-code.worktree")
            });
        }
        // 7. an article of something cleanable that no rule or marker confirms here
        //    (where a rule may match below, the rule decides)
        if let Some((aid, risk, title, impact)) = caution_article.filter(|_| !in_zone) {
            let verdict = if risk.is_deletable() && risk != RiskLevel::Advanced {
                Verdict::Caution
            } else {
                Verdict::Guide
            };
            // a package store with its own cleanup command: that command is the way
            let store = key.and_then(|k| self.k.store_for(k));
            let action = match (verdict, store) {
                (Verdict::Caution, Some((_, true))) => Action::Store,
                (Verdict::Caution, _) => Action::RecycleBin,
                _ => Action::Guide,
            };
            return Some(Unit {
                title: Some(title),
                store: store.map(|(s, _)| s.to_owned()),
                untouched: Some(match action {
                    Action::Store => text::store_untouched(),
                    Action::RecycleBin => text::explained_untouched(),
                    _ => text::guide_untouched(),
                }),
                ..unit(id, verdict, Reason::Explained, action, impact)
                    .source(&aid)
                    .level(risk)
            });
        }
        // 8. the user's photos, videos, music
        if is_dir
            && (key.is_some_and(|k| self.k.personal.contains(k))
                || PERSONAL_NAMES.contains(&name_l.as_str()))
        {
            return Some(unit(
                id,
                Verdict::Unknown,
                Reason::Personal,
                Action::None,
                text::personal(),
            ));
        }
        // 9. old installers in Downloads, old large files
        if !is_dir {
            if up.is_some_and(|p| p.downloads) && self.installers.matches(e) {
                return Some(Unit {
                    untouched: Some(text::recycle_untouched()),
                    ..unit(
                        id,
                        Verdict::Caution,
                        Reason::OldInstaller,
                        Action::RecycleBin,
                        text::old_installer(),
                    )
                });
            }
            if self.stale.matches(e) {
                return Some(Unit {
                    untouched: Some(text::recycle_untouched()),
                    ..unit(
                        id,
                        Verdict::Caution,
                        Reason::OldLarge,
                        Action::RecycleBin,
                        text::old_large(),
                    )
                });
            }
        }
        None
    }

    /// The rule claiming `path`: a 🔒 Keep rule, else (unless `keep_only`) the
    /// strictest rule the SafetyGate lets clean it.
    fn rule_unit(
        &self,
        id: u32,
        e: &Entry,
        path: &Path,
        key: &str,
        among: &[usize],
        keep_only: bool,
    ) -> Option<Unit> {
        let matches = self.k.rules.rules_matching_among(path, among);
        if matches.is_empty() {
            return None;
        }
        if let Some((rule, _)) = matches
            .iter()
            .find(|(r, _)| r.level == RiskLevel::Protected)
        {
            let info = self.k.rule_info.get(&rule.id)?;
            return Some(Unit {
                app: Some(rule.app.clone()),
                ..unit(
                    id,
                    Verdict::Keep,
                    Reason::KeepRule,
                    Action::None,
                    info.impact.clone(),
                )
                .source(&rule.id)
                .level(RiskLevel::Protected)
            });
        }
        if keep_only {
            return None;
        }
        let mut allowed: Vec<&Rule> = matches
            .iter()
            .map(|(r, _)| *r)
            .filter(|r| {
                self.k
                    .gate
                    .check_path(path, Origin::Rule { level: r.level })
                    .is_ok()
            })
            .collect();
        allowed.sort_by_key(|r| std::cmp::Reverse(r.level));
        let rule = allowed.first()?;
        let info = self.k.rule_info.get(&rule.id)?;
        let mut verdict = Verdict::of_level(rule.level);
        let mut blocked = None;
        if rule.keep_newest && !self.older_revision(id, e) {
            verdict = Verdict::Keep;
            blocked = Some("newest".to_owned());
        }
        let store = self.k.store_for(key);
        let action = match store {
            Some((_, true)) => Action::Store,
            _ => Action::Clean,
        };
        Some(Unit {
            app: Some(rule.app.clone()),
            store: store.map(|(s, _)| s.to_owned()),
            untouched: Some(if action == Action::Store {
                text::store_untouched()
            } else {
                info.untouched.clone()
            }),
            requires_closed: rule.requires_closed.clone(),
            cutoff: (rule.min_age_days > 0)
                .then(|| self.now - i64::from(rule.min_age_days) * 86_400),
            selected: verdict == Verdict::Safe && blocked.is_none() && action == Action::Clean,
            blocked,
            ..unit(id, verdict, Reason::Rule, action, info.impact.clone())
                .source(&rule.id)
                .level(rule.level)
        })
    }

    /// `true` when a newer revision of the same name sits next to `e` (Playwright's
    /// `chromium-1187` next to `chromium-1200`).
    fn older_revision(&self, id: u32, e: &Entry) -> bool {
        let Some((name, rev)) = nfd_rules::revision_of(&e.name) else {
            return false;
        };
        let Some(parent) = e.parent else {
            return false;
        };
        self.children.children(parent).iter().any(|c| {
            let s = self.tree.entry(*c);
            c.0 != id
                && !s.removed
                && s.kind == EntryKind::Dir
                && nfd_rules::revision_of(&s.name).is_some_and(|(n, r)| n == name && r > rev)
        })
    }

    /// Entry at `rel` (`/`-separated, case-insensitive) below `id`.
    fn lookup_rel(&self, id: u32, rel: &str) -> Option<u32> {
        let mut cur = EntryId(id);
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            cur = *self.children.children(cur).iter().find(|c| {
                let e = self.tree.entry(**c);
                !e.removed && e.name.eq_ignore_ascii_case(part)
            })?;
        }
        Some(cur.0)
    }

    /// Build-output folders among the children of folder `id` (C9 policy; generic
    /// names need git and are left to the Repo tab).
    fn sites_of(&self, id: u32) -> Vec<(u32, Site)> {
        let kids = self.children.children(EntryId(id));
        let k = self.k;
        let has_marker = kids.iter().any(|c| {
            let e = self.tree.entry(*c);
            if e.removed {
                return false;
            }
            match e.kind {
                EntryKind::File => {
                    k.marker_files
                        .iter()
                        .any(|m| m.eq_ignore_ascii_case(&e.name))
                        || k.marker_exts
                            .iter()
                            .any(|x| ends_with_ignore_case(&e.name, x))
                }
                EntryKind::Dir => k
                    .marker_dirs
                    .iter()
                    .any(|m| m.eq_ignore_ascii_case(&e.name)),
                _ => false,
            }
        });
        if !has_marker {
            return Vec::new();
        }
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for c in kids {
            let e = self.tree.entry(*c);
            match e.kind {
                _ if e.removed => {}
                EntryKind::File => files.push(e.name.to_string()),
                EntryKind::Dir => dirs.push(e.name.to_string()),
                _ => {}
            }
        }
        let exists = |rel: &str| self.lookup_rel(id, rel).is_some();
        artifacts::sites(&files, &dirs, &exists)
            .into_iter()
            .filter(|s| !s.generic)
            .filter_map(|s| {
                let eid = self.lookup_rel(id, &s.rel)?;
                (self.tree.entries[eid as usize].kind == EntryKind::Dir).then_some((eid, s))
            })
            .collect()
    }
}

/// A unit with the common fields filled.
fn unit(
    id: u32,
    verdict: Verdict,
    reason: Reason,
    action: Action,
    impact: Arc<crate::Text>,
) -> Unit {
    Unit {
        entry: id,
        verdict,
        reason,
        level: None,
        source: None,
        app: None,
        title: None,
        impact,
        untouched: None,
        action,
        blocked: None,
        selected: false,
        store: None,
        requires_closed: Vec::new(),
        cutoff: None,
        hard: false,
    }
}

impl Unit {
    fn source(mut self, s: &str) -> Self {
        self.source = Some(s.to_owned());
        self
    }

    const fn level(mut self, l: RiskLevel) -> Self {
        self.level = Some(l);
        self
    }
}

fn history_unit(id: u32, hit: &str, name: &str) -> Unit {
    let what = if hit.eq_ignore_ascii_case(name) {
        name.to_owned()
    } else {
        format!("{hit}/{name}")
    };
    Unit {
        app: Some(catalog::history_tool(&what).to_owned()),
        hard: true,
        ..unit(
            id,
            Verdict::Keep,
            Reason::AgentHistory,
            Action::None,
            text::history(),
        )
        .source(&what)
    }
}

/// The unit for a location the SafetyGate protects (`template` as it reports it).
fn protected_unit(id: u32, template: &str) -> Unit {
    let t = template.to_lowercase();
    if AGENT_HISTORY_TEMPLATES.contains(&template) || t.starts_with("<any>/") {
        return Unit {
            hard: true,
            app: Some(
                catalog::history_tool(template.rsplit('/').nth(1).unwrap_or(template)).to_owned(),
            ),
            ..unit(
                id,
                Verdict::Keep,
                Reason::AgentHistory,
                Action::None,
                text::history(),
            )
            .source(template)
        };
    }
    let top = t.strip_prefix("<volume>/");
    let (reason, impact) = if t.starts_with("${systemroot}") || top == Some("windows") {
        (Reason::System, text::system())
    } else if t.contains("${programfiles")
        || t.contains("program files")
        || t.contains("${programdata}")
        || top == Some("programdata")
    {
        (Reason::Programs, text::programs())
    } else {
        (Reason::Protected, text::protected(template))
    };
    Unit {
        hard: true,
        ..unit(id, Verdict::Keep, reason, Action::None, impact).source(template)
    }
}
