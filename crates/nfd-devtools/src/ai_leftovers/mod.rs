//! AI Leftovers (PLAN §13.2 C10, `docs/AI_LEFTOVERS.md`): what AI coding agents leave
//! behind per session — worktrees, scratchpads, browser snapshots, screenshots, test
//! output — found in the folders the sessions worked in, in `%TEMP%` and in
//! Playwright's browser store, and tied to the session that made it.
//!
//! Safety of the proposals:
//! * conversation history is never a candidate: the SafetyGate refuses it by
//!   location, name and `folder/name` pair, and every proposed folder is walked
//!   first — one history file inside and it is refused (`contains-history`);
//! * leftovers of a session that is still open in the Claude app, or was active in
//!   the last [`Options::keep_recent_days`] days, are listed but never pre-selected;
//! * common names (`test-results`, `screenshots`…) are only offered when git
//!   confirms nothing in them is tracked; a folder holding tracked files never is;
//! * worktrees are removed by git, only when clean and merged or pushed
//!   ([`worktrees`]); everything else goes Planner → SafetyGate → Executor →
//!   quarantine like the repository build output ([`prepare`]).

pub mod catalog;
pub mod sessions;
pub mod worktrees;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use nfd_core::RiskLevel;
use nfd_core::plan::{self, CleanPlan, DeleteMode, PlanItem, PlanOptions};
use nfd_core::safety::{AGENT_HISTORY_SUFFIXES, Origin, SafetyGate, agent_history_entry};
use nfd_platform::fsinfo::{self, LinkKind};
use serde::Serialize;

use crate::artifacts::NEVER_DESCEND;
use crate::fsutil::Totals;
use crate::repos::{self, Artifact, RepoOptions};
use crate::{norm, within};
use catalog::{Entry, Kind, Scope};
use sessions::{SessionIndex, SessionRef, SessionSources};
use worktrees::{WorktreeJob, WtEntry};

/// Options of [`scan`].
#[derive(Debug, Clone)]
pub struct Options {
    pub now: i64,
    /// Leftovers of sessions active this recently (or written this recently) are not
    /// pre-selected (0 = off).
    pub keep_recent_days: u32,
    /// Entries visited per folder when sizing.
    pub max_entries: u64,
    /// More folders to look for projects in (the Repo Finder root), besides the
    /// sessions' working directories.
    pub roots: Vec<PathBuf>,
    /// Folder levels below a project searched for leftovers.
    pub project_depth: usize,
    /// Folder levels below a root searched for git repositories.
    pub root_depth: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            now: nfd_rules::matcher::now(),
            keep_recent_days: 7,
            max_entries: 2_000_000,
            roots: Vec::new(),
            project_depth: 3,
            root_depth: 3,
        }
    }
}

/// Where to look (tests and the GUI smoke test point these into a fixture).
#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub sessions: SessionSources,
    /// `%TEMP%\claude` folders.
    pub temp_claude: Vec<PathBuf>,
    /// `ms-playwright` browser stores.
    pub playwright: Vec<PathBuf>,
    /// Folders that are never projects (`%APPDATA%`, `%LOCALAPPDATA%`, `%TEMP%`).
    pub not_projects: Vec<PathBuf>,
}

impl Sources {
    /// The real locations on this machine.
    #[must_use]
    pub fn detect(
        resolve: &dyn Fn(&str) -> Option<String>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        let p = |v: &str| resolve(v).map(PathBuf::from);
        let mut s = Self {
            sessions: SessionSources::detect(resolve, env),
            ..Self::default()
        };
        if let Some(t) = p("TEMP") {
            s.temp_claude.push(t.join("claude"));
        }
        if let Some(l) = p("LOCALAPPDATA") {
            s.playwright.push(l.join("ms-playwright"));
        }
        if let Some(dir) =
            env("PLAYWRIGHT_BROWSERS_PATH").filter(|d| !d.trim().is_empty() && d != "0")
        {
            s.playwright.insert(0, PathBuf::from(dir));
        }
        if let Some(h) = p("HOME") {
            s.playwright.push(h.join(".cache").join("ms-playwright"));
        }
        s.not_projects = ["APPDATA", "LOCALAPPDATA", "TEMP", "PROGRAMDATA"]
            .iter()
            .filter_map(|v| p(v))
            .collect();
        for list in [&mut s.temp_claude, &mut s.playwright] {
            list.retain(|d| d.is_dir());
        }
        s
    }
}

/// A 🟡 / 🟢 worktree's git facts.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub repo: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub default_branch: Option<String>,
    pub merged: bool,
    pub pushed: bool,
    /// Uncommitted changes and untracked files.
    pub changes: u32,
    pub locked: bool,
    /// git knows it (else: an orphan folder in `.claude/worktrees`).
    pub registered: bool,
    /// The folder is gone; only `git worktree prune` is left to do.
    pub prunable: bool,
    /// What git ignores in it (quarantined before git removes the rest).
    pub ignored: Vec<Ignored>,
    /// `git worktree remove` may run.
    pub removable: bool,
    /// When it cannot be removed: its build output (C9) that may still be cleaned.
    pub artifacts: Vec<Artifact>,
    pub command: Option<String>,
}

/// One ignored entry of a worktree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ignored {
    pub rel: String,
    pub is_dir: bool,
    #[serde(flatten)]
    pub totals: Totals,
    pub mtime: i64,
}

/// One leftover.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub path: PathBuf,
    /// Catalog entry id.
    pub entry: &'static str,
    pub tool: &'static str,
    pub kind: Kind,
    pub scope: Scope,
    /// The project folder (sessions' working directory, worktrees' repository).
    pub project: Option<PathBuf>,
    /// Shown name, relative to the project / `%TEMP%` / the browser store.
    pub rel: String,
    pub level: RiskLevel,
    #[serde(flatten)]
    pub totals: Totals,
    pub mtime: i64,
    pub session: Option<SessionRef>,
    /// Why it is not pre-selected although it could be cleaned: `session-open`,
    /// `session-recent`, `recent-write`.
    pub keep: Option<&'static str>,
    pub selected: bool,
    /// Why it cannot be cleaned: a SafetyGate code, `tracked`, `contains-history`,
    /// `too-large`, `no-git`, `git-failed`, `dirty`, `not-merged`, `locked`,
    /// `too-many-ignored`, `foreign-worktree`.
    pub blocked: Option<String>,
    pub impact: &'static str,
    pub impact_vi: &'static str,
    pub worktree: Option<Worktree>,
}

/// Conversation history found next to leftovers: shown as 🔒 kept, never offered.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub path: PathBuf,
    pub tool: &'static str,
    pub project: Option<PathBuf>,
    /// `.aider.chat.history.md`, `.codex/sessions`…
    pub what: String,
}

/// Everything [`scan`] returns.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub items: Vec<Item>,
    pub kept: Vec<Kept>,
    pub projects: Vec<PathBuf>,
    pub sessions: usize,
    /// git was found (worktrees and common names need it).
    pub git: bool,
    pub keep_recent_days: u32,
    /// Bytes pre-selected ("Clean finished sessions").
    pub selected_bytes: u64,
    pub total_bytes: u64,
    pub cancelled: bool,
}

/// Size of a folder tree, and the first chat-history entry inside it, if any.
#[derive(Debug, Clone, Default)]
struct Measure {
    totals: Totals,
    history: Option<PathBuf>,
}

/// Walk `root` (no links followed) for its totals, the totals of each of `buckets`
/// (relative `/`-separated paths below it) and the first chat-history entry.
fn measure_buckets(root: &Path, buckets: &[String], max_entries: u64) -> (Measure, Vec<Totals>) {
    let mut m = Measure::default();
    let mut per = vec![Totals::default(); buckets.len()];
    let lower: Vec<String> = buckets.iter().map(|b| b.to_lowercase()).collect();
    let Ok(info) = fsinfo::file_info(root) else {
        return (m, per);
    };
    if info.link != LinkKind::None {
        return (m, per);
    }
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !info.is_dir {
        m.totals = Totals {
            bytes: info.on_disk,
            files: 1,
            newest: info.mtime,
            truncated: false,
        };
        return (m, per);
    }
    let add = |t: &mut Totals, bytes: u64, mtime: i64| {
        t.bytes += bytes;
        t.files += 1;
        t.newest = t.newest.max(mtime);
    };
    let mut seen = 0u64;
    let mut stack: Vec<(PathBuf, String, String, Option<usize>)> =
        vec![(root.to_path_buf(), String::new(), root_name, None)];
    while let Some((dir, rel, name, bucket)) = stack.pop() {
        let Ok(items) = fsinfo::read_dir(&dir, false) else {
            continue;
        };
        for item in items {
            seen += 1;
            if seen > max_entries {
                m.totals.truncated = true;
                for t in &mut per {
                    t.truncated = true;
                }
                return (m, per);
            }
            let child = item.name.to_string_lossy().into_owned();
            if m.history.is_none() && agent_history_entry(Some(&name), &child).is_some() {
                m.history = Some(dir.join(&child));
            }
            let Ok(info) = item.info else { continue };
            if info.link != LinkKind::None {
                continue;
            }
            let child_rel = if rel.is_empty() {
                child.clone()
            } else {
                format!("{rel}/{child}")
            };
            let b = bucket.or_else(|| {
                let l = child_rel.to_lowercase();
                lower.iter().position(|x| *x == l)
            });
            if info.is_dir {
                stack.push((dir.join(&child), child_rel, child, b));
            } else {
                add(&mut m.totals, info.on_disk, info.mtime);
                if let Some(i) = b {
                    add(&mut per[i], info.on_disk, info.mtime);
                }
            }
        }
    }
    (m, per)
}

fn measure(path: &Path, max_entries: u64) -> Measure {
    measure_buckets(path, &[], max_entries).0
}

fn mtime(path: &Path) -> i64 {
    fsinfo::file_info(path).map_or(0, |i| i.mtime)
}

fn is_dir(path: &Path) -> bool {
    fsinfo::file_info(path).is_ok_and(|i| i.is_dir && i.link == LinkKind::None)
}

/// Why a candidate folder cannot be proposed (gate, history inside, too large).
fn refuse(gate: &SafetyGate, path: &Path, m: &Measure) -> Option<String> {
    if let Err(b) = gate.check_path(path, Origin::Manual) {
        return Some(b.code().to_owned());
    }
    if m.history.is_some() {
        return Some("contains-history".into());
    }
    m.totals.truncated.then(|| "too-large".into())
}

/// The project a session's working directory belongs to: the repository of a
/// `.claude/worktrees/<name>` worktree, else the nearest folder with `.git`.
fn project_of(cwd: &Path) -> PathBuf {
    let text = cwd.to_string_lossy().replace('\\', "/");
    if let Some(i) = text.to_lowercase().find("/.claude/worktrees/") {
        return PathBuf::from(&text[..i]);
    }
    let mut dir = Some(cwd);
    for _ in 0..6 {
        let Some(d) = dir else { break };
        if d.join(".git").exists() {
            return d.to_path_buf();
        }
        dir = d.parent();
    }
    cwd.to_path_buf()
}

/// Git repositories below `root` (not inside one another), `depth` levels deep.
fn repos_below(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if root.join(".git").exists() {
        out.push(root.to_path_buf());
        return;
    }
    if depth == 0 {
        return;
    }
    for name in crate::fsutil::subdirs(root) {
        let lower = name.to_lowercase();
        if name.starts_with('.') || NEVER_DESCEND.contains(&lower.as_str()) {
            continue;
        }
        repos_below(&root.join(name), depth - 1, out);
    }
}

/// Folder names never entered while looking for leftovers inside a project.
const SKIP: &[&str] = &[
    "target",
    "build",
    "dist",
    "out",
    "bin",
    "obj",
    "vendor",
    "library",
    "intermediate",
    "deriveddatacache",
    "venv",
    "env",
    "__pycache__",
    "site-packages",
];
/// Folders visited per project at most.
const MAX_DIRS: usize = 5_000;

struct Hit {
    path: PathBuf,
    rel: String,
    entry: &'static Entry,
}

fn rel_of(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

/// Leftovers and chat history inside `project` (not its worktrees).
fn walk_project(project: &Path, depth: usize, hits: &mut Vec<Hit>, kept: &mut Vec<Kept>) {
    let entries: Vec<&'static Entry> = catalog::CATALOG
        .iter()
        .filter(|e| e.scope == Scope::Project && e.kind == Kind::Folder)
        .collect();
    let parents: Vec<&str> = AGENT_HISTORY_SUFFIXES.iter().map(|(p, _)| *p).collect();
    let mut visited = 0usize;
    let mut stack = vec![(project.to_path_buf(), 0usize)];
    while let Some((dir, d)) = stack.pop() {
        visited += 1;
        if visited > MAX_DIRS {
            break;
        }
        let Ok(items) = fsinfo::read_dir(&dir, false) else {
            continue;
        };
        let mut children: Vec<(String, bool)> = Vec::new();
        for i in items {
            let Ok(info) = i.info else { continue };
            if info.link != LinkKind::None {
                continue;
            }
            children.push((i.name.to_string_lossy().into_owned(), info.is_dir));
        }
        let mut taken: Vec<String> = Vec::new();
        for e in &entries {
            if e.pattern.contains('/') {
                let p = e.pattern.split('/').fold(dir.clone(), |a, c| a.join(c));
                if is_dir(&p) {
                    taken.push(e.pattern.split('/').next().unwrap_or("").to_lowercase());
                    hits.push(Hit {
                        rel: rel_of(project, &p),
                        path: p,
                        entry: e,
                    });
                }
            } else {
                for (name, _) in children
                    .iter()
                    .filter(|(n, _)| catalog::name_matches(e.pattern, n))
                {
                    taken.push(name.to_lowercase());
                    let p = dir.join(name);
                    hits.push(Hit {
                        rel: rel_of(project, &p),
                        path: p,
                        entry: e,
                    });
                }
            }
        }
        for (name, dir_kind) in &children {
            if let Some(what) = agent_history_entry(None, name) {
                kept.push(Kept {
                    path: dir.join(name),
                    tool: catalog::history_tool(what),
                    project: Some(project.to_path_buf()),
                    what: what.to_owned(),
                });
            }
            if *dir_kind && parents.iter().any(|p| p.eq_ignore_ascii_case(name)) {
                for inner in crate::fsutil::subdirs(&dir.join(name))
                    .into_iter()
                    .chain(files_of(&dir.join(name)))
                {
                    if let Some(f) = agent_history_entry(Some(name), &inner) {
                        kept.push(Kept {
                            path: dir.join(name).join(&inner),
                            tool: catalog::history_tool(f),
                            project: Some(project.to_path_buf()),
                            what: format!("{f}/{}", inner.to_lowercase()),
                        });
                    }
                }
            }
        }
        if d >= depth {
            continue;
        }
        for (name, dir_kind) in children {
            let lower = name.to_lowercase();
            if !dir_kind
                || name.starts_with('.')
                || NEVER_DESCEND.contains(&lower.as_str())
                || SKIP.contains(&lower.as_str())
                || taken.contains(&lower)
            {
                continue;
            }
            stack.push((dir.join(name), d + 1));
        }
    }
}

fn files_of(dir: &Path) -> Vec<String> {
    fsinfo::read_dir(dir, false)
        .map(|items| {
            items
                .into_iter()
                .filter(|i| i.info.as_ref().is_ok_and(|f| !f.is_dir))
                .map(|i| i.name.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

struct Ctx<'a> {
    opts: &'a Options,
    gate: &'a SafetyGate,
    git: Option<&'a Path>,
    index: &'a SessionIndex,
}

impl Ctx<'_> {
    fn keep_window(&self) -> i64 {
        i64::from(self.opts.keep_recent_days) * 86_400
    }

    /// The session may still be working: active in the last [`LIVE_HOURS`]. Its
    /// leftovers cannot even be ticked by hand.
    fn live(&self, session: Option<&SessionRef>) -> bool {
        session.is_some_and(|s| self.opts.now - s.last_active < LIVE_HOURS * 3600)
    }

    /// Why a leftover touched by `session` and last written at `newest` is kept.
    fn keep(&self, session: Option<&SessionRef>, newest: i64) -> Option<&'static str> {
        match session {
            Some(s) if s.open => Some("session-open"),
            Some(s) if s.recent => Some("session-recent"),
            _ if self.opts.keep_recent_days > 0 && self.opts.now - newest < self.keep_window() => {
                Some("recent-write")
            }
            _ => None,
        }
    }

    fn item(
        &self,
        entry: &'static Entry,
        path: PathBuf,
        rel: String,
        project: Option<PathBuf>,
        m: &Measure,
        session: Option<SessionRef>,
    ) -> Item {
        let blocked = refuse(self.gate, &path, m).or_else(|| {
            (self.live(session.as_ref()) || in_use_here(&path)).then(|| "session-live".to_owned())
        });
        let keep = self.keep(session.as_ref(), m.totals.newest.max(mtime(&path)));
        Item {
            mtime: mtime(&path),
            entry: entry.id,
            tool: entry.tool,
            kind: entry.kind,
            scope: entry.scope,
            project,
            rel,
            level: entry.level,
            totals: m.totals,
            session,
            keep,
            selected: false,
            blocked,
            impact: entry.impact,
            impact_vi: entry.impact_vi,
            worktree: None,
            path,
        }
    }

    fn project_items(&self, project: &Path, out: &mut Vec<Item>, kept: &mut Vec<Kept>) {
        let mut hits = Vec::new();
        walk_project(project, self.opts.project_depth, &mut hits, kept);
        let git_repo = project.join(".git").exists();
        let rels: Vec<String> = hits.iter().map(|h| h.rel.clone()).collect();
        let (ignored, tracked) = match (git_repo, self.git) {
            (true, Some(g)) => repos::git_checks(g, project, &rels),
            _ => (Vec::new(), Vec::new()),
        };
        for h in hits {
            let is_ignored = ignored.iter().any(|i| i.eq_ignore_ascii_case(&h.rel));
            if h.entry.generic && (!(git_repo && self.git.is_some()) || tracked.contains(&h.rel)) {
                // a common name outside git, or one git tracks, is somebody's folder
                continue;
            }
            let m = measure(&h.path, self.opts.max_entries);
            let session = self
                .index
                .at(project, m.totals.newest)
                .map(|s| sessions::reference(s, self.opts.now, self.opts.keep_recent_days));
            let mut it = self.item(
                h.entry,
                h.path,
                h.rel.clone(),
                Some(project.to_path_buf()),
                &m,
                session,
            );
            if tracked.contains(&h.rel) {
                it.blocked = Some("tracked".into());
            }
            if h.entry.generic && !is_ignored {
                it.level = RiskLevel::Caution;
            }
            out.push(it);
        }
        let wt_dir = project.join(".claude").join("worktrees");
        if is_dir(&wt_dir) {
            self.worktrees(project, &wt_dir, git_repo, out);
        }
    }

    fn worktrees(&self, repo: &Path, wt_dir: &Path, git_repo: bool, out: &mut Vec<Item>) {
        let entry = catalog::entry("claude-code.worktree").expect("catalog");
        let registered = match (git_repo, self.git) {
            (true, Some(g)) => worktrees::list(g, repo),
            _ => None,
        };
        let wt_norm = worktrees::real_key(wt_dir);
        let mine: Vec<WtEntry> = registered
            .iter()
            .flatten()
            .filter(|e| {
                let p = worktrees::real_key(&e.path);
                within(&p, &wt_norm) && p != wt_norm
            })
            .cloned()
            .collect();
        let mut names: Vec<String> = crate::fsutil::subdirs(wt_dir);
        names.sort();
        for name in names {
            let path = wt_dir.join(&name);
            let key = worktrees::real_key(&path);
            let reg = mine.iter().find(|e| worktrees::real_key(&e.path) == key);
            out.push(self.worktree(repo, entry, path, reg, registered.is_some()));
        }
        // registered worktrees whose folder is gone: `git worktree prune`
        for e in mine.iter().filter(|e| e.prunable && !e.path.exists()) {
            let session = sessions::strongest(
                self.index.within(&e.path),
                self.opts.now,
                self.opts.keep_recent_days,
            );
            out.push(Item {
                path: e.path.clone(),
                entry: entry.id,
                tool: entry.tool,
                kind: Kind::Worktree,
                scope: Scope::Project,
                project: Some(repo.to_path_buf()),
                rel: rel_of(repo, &e.path),
                level: RiskLevel::Safe,
                totals: Totals::default(),
                mtime: 0,
                session,
                keep: None,
                selected: true,
                blocked: None,
                impact: entry.impact,
                impact_vi: entry.impact_vi,
                worktree: Some(Worktree {
                    repo: repo.to_path_buf(),
                    branch: e.branch.clone(),
                    head: e.head.clone(),
                    registered: true,
                    prunable: true,
                    removable: true,
                    ..Worktree::default()
                }),
            });
        }
    }

    fn worktree(
        &self,
        repo: &Path,
        entry: &'static Entry,
        path: PathBuf,
        reg: Option<&WtEntry>,
        git_answered: bool,
    ) -> Item {
        let session = sessions::strongest(
            self.index.within(&path),
            self.opts.now,
            self.opts.keep_recent_days,
        );
        let rel = rel_of(repo, &path);
        let mut wt = Worktree {
            repo: repo.to_path_buf(),
            registered: reg.is_some(),
            ..Worktree::default()
        };
        let mut blocked: Option<String>;
        let mut level = RiskLevel::Caution;
        let (m, per, ignored) = match (reg, self.git) {
            (Some(e), Some(g)) => {
                wt.branch = e.branch.clone();
                wt.head = e.head.clone();
                wt.locked = e.locked;
                match worktrees::state(g, repo, e) {
                    Some(st) => {
                        wt.changes = st.changes;
                        wt.merged = st.merged;
                        wt.pushed = st.pushed;
                        wt.default_branch = st.default_branch.clone();
                        let ign = st.ignored.clone();
                        let buckets = ign.clone().unwrap_or_default();
                        let (m, per) = measure_buckets(&path, &buckets, self.opts.max_entries);
                        blocked = if e.locked {
                            Some("locked".into())
                        } else if st.changes > 0 {
                            Some("dirty".into())
                        } else if !st.merged && !st.pushed {
                            Some("not-merged".into())
                        } else if ign.is_none() {
                            Some("too-many-ignored".into())
                        } else {
                            None
                        };
                        (m, per, buckets)
                    }
                    None => {
                        blocked = Some("git-failed".into());
                        (
                            measure(&path, self.opts.max_entries),
                            Vec::new(),
                            Vec::new(),
                        )
                    }
                }
            }
            (None, _) => {
                let m = measure(&path, self.opts.max_entries);
                blocked = if self.git.is_none() || !git_answered {
                    Some("no-git".into())
                } else if path.join(".git").exists() {
                    Some("foreign-worktree".into())
                } else {
                    None
                };
                (m, Vec::new(), Vec::new())
            }
            (Some(_), None) => {
                blocked = Some("no-git".into());
                (
                    measure(&path, self.opts.max_entries),
                    Vec::new(),
                    Vec::new(),
                )
            }
        };
        // the folder itself and every ignored entry must pass the gate
        let refused = refuse(self.gate, &path, &m);
        for (rel_i, totals) in ignored.iter().zip(per) {
            let p = path.join(rel_i);
            if blocked.is_none() {
                if let Err(b) = self.gate.check_path(&p, Origin::Manual) {
                    blocked = Some(b.code().to_owned());
                }
            }
            wt.ignored.push(Ignored {
                rel: rel_i.clone(),
                is_dir: is_dir(&p),
                totals,
                mtime: mtime(&p),
            });
        }
        if refused.is_some() {
            blocked = refused;
        }
        // the worktree of a session that may still be working (this one included)
        // is never touched, not even its build output
        let live = self.live(session.as_ref()) || in_use_here(&path);
        if live {
            blocked = Some("session-live".into());
        }
        if reg.is_some() && blocked.is_none() {
            level = RiskLevel::Safe;
            wt.removable = true;
            let job = job_of(&path, &wt, &m.totals);
            wt.command = Some(job.command_line());
        }
        let keep = self.keep(session.as_ref(), m.totals.newest);
        // a worktree that cannot go may still shed its build output (C9)
        if reg.is_some() && matches!(blocked.as_deref(), Some("dirty" | "not-merged" | "locked")) {
            let opts = RepoOptions {
                now: self.opts.now,
                keep_recent_days: self.opts.keep_recent_days,
                git_status: false,
                max_entries: self.opts.max_entries,
                max_depth: 6,
                ..RepoOptions::default()
            };
            let scan = repos::find(&path, opts, self.git, self.gate, None);
            wt.artifacts = scan.repos.into_iter().flat_map(|r| r.artifacts).collect();
            if keep.is_some() {
                for a in &mut wt.artifacts {
                    a.selected = false;
                }
            }
        }
        if !wt.registered && blocked.is_none() {
            // an orphan folder: its files are not in git any more
            level = RiskLevel::Caution;
        }
        Item {
            mtime: mtime(&path),
            entry: entry.id,
            tool: entry.tool,
            kind: Kind::Worktree,
            scope: Scope::Project,
            project: Some(repo.to_path_buf()),
            rel,
            level,
            totals: m.totals,
            session,
            keep,
            selected: false,
            blocked,
            impact: entry.impact,
            impact_vi: entry.impact_vi,
            worktree: Some(wt),
            path,
        }
    }

    fn temp_items(&self, temp: &Path, out: &mut Vec<Item>) {
        let entry = catalog::entry("claude-code.session-temp").expect("catalog");
        for project in crate::fsutil::subdirs(temp) {
            let ids: Vec<String> = crate::fsutil::subdirs(&temp.join(&project))
                .into_iter()
                .filter(|n| is_session_id(n))
                .collect();
            if ids.is_empty() {
                // a folder without sessions: Claude Code's own (`bash-edit-diff`, a
                // project's folder) is one item; anything else someone put there is
                // only offered as 🟡
                let path = temp.join(&project);
                let m = measure(&path, self.opts.max_entries);
                let mut it = self.item(entry, path, format!("claude/{project}"), None, &m, None);
                if !is_claude_temp_name(&project) {
                    it.level = RiskLevel::Caution;
                }
                out.push(it);
                continue;
            }
            for id in ids {
                let path = temp.join(&project).join(&id);
                let m = measure(&path, self.opts.max_entries);
                let s = self.index.by_id(&id);
                let session =
                    s.map(|s| sessions::reference(s, self.opts.now, self.opts.keep_recent_days));
                let home = s.and_then(|s| s.cwd.clone()).map(|c| project_of(&c));
                out.push(self.item(
                    entry,
                    path,
                    format!("claude/{project}/{id}"),
                    home,
                    &m,
                    session,
                ));
            }
        }
    }

    fn browser_items(&self, store: &Path, out: &mut Vec<Item>) {
        let entry = catalog::entry("playwright.old-browser").expect("catalog");
        let mut newest: BTreeMap<String, u64> = BTreeMap::new();
        let mut found: Vec<(String, u64, String)> = Vec::new();
        for name in crate::fsutil::subdirs(store) {
            if let Some((browser, rev)) = browser_revision(&name) {
                let e = newest.entry(browser.clone()).or_default();
                *e = (*e).max(rev);
                found.push((browser, rev, name));
            }
        }
        for (browser, rev, name) in found {
            if newest.get(&browser).is_some_and(|n| *n == rev) {
                continue;
            }
            let path = store.join(&name);
            let m = measure(&path, self.opts.max_entries);
            let mut it = self.item(entry, path, name, None, &m, None);
            // an install date says nothing about use: old revisions are not kept
            it.keep = None;
            out.push(it);
        }
    }
}

/// Leftovers of a session active this recently are blocked (it may still be working).
pub const LIVE_HOURS: i64 = 24;

/// `true` when this process works inside `path` (its current folder or its program).
fn in_use_here(path: &Path) -> bool {
    let key = worktrees::real_key(path);
    [std::env::current_dir().ok(), std::env::current_exe().ok()]
        .into_iter()
        .flatten()
        .any(|p| within(&worktrees::real_key(&p), &key))
}

/// Folders Claude Code itself makes in `%TEMP%\claude`: a project's folder (its
/// path encoded, `D--Work-app`, `-home-me-app`) or a tool folder it is known to use.
#[must_use]
pub fn is_claude_temp_name(name: &str) -> bool {
    let b = name.as_bytes();
    (b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b'-' && b[2] == b'-')
        || name.starts_with('-')
        || ["bash-edit-diff"].contains(&name)
}

/// `true` for a session id folder name (`3f84adc8-0277-448e-b6ab-422b34ca8709`).
#[must_use]
pub fn is_session_id(name: &str) -> bool {
    name.len() == 36
        && name.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// `(browser, revision)` of a Playwright store folder (`chromium_headless_shell-1248`);
/// `None` for anything else (`mcp-chrome-…` profiles, `.links`).
#[must_use]
pub fn browser_revision(name: &str) -> Option<(String, u64)> {
    nfd_rules::revision_of(name)
}

fn job_of(path: &Path, wt: &Worktree, totals: &Totals) -> WorktreeJob {
    let ignored_bytes: u64 = wt.ignored.iter().map(|i| i.totals.bytes).sum();
    let ignored_files: u64 = wt.ignored.iter().map(|i| i.totals.files).sum();
    WorktreeJob {
        repo: wt.repo.clone(),
        path: path.to_path_buf(),
        branch: wt.branch.clone(),
        head: wt.head.clone(),
        prune_only: wt.prunable,
        bytes: totals.bytes.saturating_sub(ignored_bytes),
        files: totals.files.saturating_sub(ignored_files),
    }
}

/// Find the leftovers: projects from the sessions' working directories and
/// [`Options::roots`], `%TEMP%\claude`, Playwright's browser store.
#[must_use]
pub fn scan(
    opts: &Options,
    sources: &Sources,
    git: Option<&Path>,
    gate: &SafetyGate,
    cancel: Option<&AtomicBool>,
) -> Report {
    let stopped = || cancel.is_some_and(|c| c.load(Ordering::Relaxed));
    let index = SessionIndex::new(sessions::load(&sources.sessions));
    let ctx = Ctx {
        opts,
        gate,
        git,
        index: &index,
    };
    let not_projects: Vec<String> = sources
        .not_projects
        .iter()
        .map(|p| norm(&p.to_string_lossy()))
        .collect();
    let mut candidates: Vec<PathBuf> = index.folders().iter().map(|c| project_of(c)).collect();
    for r in &opts.roots {
        repos_below(r, opts.root_depth, &mut candidates);
    }
    let mut projects: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for p in candidates {
        let k = norm(&p.to_string_lossy());
        if seen.contains(&k)
            || !is_dir(&p)
            || not_projects.iter().any(|n| within(&k, n))
            || gate.check_path(&p, Origin::Manual).is_err()
        {
            continue;
        }
        seen.push(k);
        projects.push(p);
    }
    projects.sort();
    let mut report = Report {
        sessions: index.sessions.len(),
        git: git.is_some(),
        keep_recent_days: opts.keep_recent_days,
        ..Report::default()
    };
    for p in &projects {
        if stopped() {
            report.cancelled = true;
            break;
        }
        ctx.project_items(p, &mut report.items, &mut report.kept);
    }
    for t in &sources.temp_claude {
        if !stopped() {
            ctx.temp_items(t, &mut report.items);
        }
    }
    for s in &sources.playwright {
        if !stopped() {
            ctx.browser_items(s, &mut report.items);
        }
    }
    report.cancelled |= stopped();
    report.projects = projects;
    // a folder nested in another proposed one is shown once (the outer one)
    let outer: Vec<String> = report
        .items
        .iter()
        .map(|i| norm(&i.path.to_string_lossy()))
        .collect();
    let mut keep_flags = vec![true; report.items.len()];
    for (i, a) in outer.iter().enumerate() {
        if outer
            .iter()
            .enumerate()
            .any(|(j, b)| i != j && a != b && within(a, b))
        {
            keep_flags[i] = false;
        }
    }
    let mut flags = keep_flags.into_iter();
    report.items.retain(|_| flags.next().unwrap_or(true));
    for it in &mut report.items {
        it.selected = it.blocked.is_none() && it.level == RiskLevel::Safe && it.keep.is_none();
    }
    report.items.sort_by(|a, b| {
        a.project
            .cmp(&b.project)
            .then(a.tool.cmp(b.tool))
            .then(b.totals.bytes.cmp(&a.totals.bytes))
    });
    report.total_bytes = report
        .items
        .iter()
        .filter(|i| i.blocked.is_none())
        .map(|i| i.totals.bytes)
        .sum::<u64>()
        + report
            .items
            .iter()
            .filter_map(|i| i.worktree.as_ref())
            .flat_map(|w| &w.artifacts)
            .filter(|a| a.blocked.is_none())
            .map(|a| a.totals.bytes)
            .sum::<u64>();
    report.selected_bytes = report
        .items
        .iter()
        .filter(|i| i.selected)
        .map(|i| i.totals.bytes)
        .sum::<u64>()
        + report
            .items
            .iter()
            .filter_map(|i| i.worktree.as_ref())
            .flat_map(|w| &w.artifacts)
            .filter(|a| a.selected && a.blocked.is_none())
            .map(|a| a.totals.bytes)
            .sum::<u64>();
    report
}

/// What cleaning the `picked` paths takes: a plan of folders for the quarantine
/// (hand-picked items, so the SafetyGate checks them again before acting) and the
/// worktrees git removes afterwards.
#[derive(Debug, Clone, Default)]
pub struct Prepared {
    pub plan: CleanPlan,
    pub jobs: Vec<WorktreeJob>,
}

/// Turn the `picked` paths of `report` (items or worktree artifacts) into a
/// [`Prepared`] clean. Blocked items and unknown paths are ignored.
#[must_use]
pub fn prepare(report: &Report, picked: &[PathBuf]) -> Prepared {
    let wanted: Vec<String> = picked.iter().map(|p| norm(&p.to_string_lossy())).collect();
    let is_wanted = |p: &Path| wanted.contains(&norm(&p.to_string_lossy()));
    let mut items: Vec<PlanItem> = Vec::new();
    let mut jobs = Vec::new();
    for it in &report.items {
        if let Some(wt) = &it.worktree {
            let arts = wt
                .artifacts
                .iter()
                .filter(|a| a.blocked.is_none() && is_wanted(&a.path));
            items.extend(repos::plan(arts).items);
        }
        if it.blocked.is_some() || !is_wanted(&it.path) {
            continue;
        }
        match &it.worktree {
            Some(wt) if wt.registered => {
                if !wt.removable {
                    continue;
                }
                for ig in &wt.ignored {
                    items.push(PlanItem::manual(
                        it.path.join(&ig.rel),
                        ig.is_dir,
                        ig.totals.bytes,
                        ig.totals.files,
                        ig.mtime,
                    ));
                }
                jobs.push(job_of(&it.path, wt, &it.totals));
            }
            _ => items.push(PlanItem::manual(
                it.path.clone(),
                is_dir(&it.path) || it.totals.files != 1,
                it.totals.bytes,
                it.totals.files,
                it.mtime,
            )),
        }
    }
    let mut plan = plan::build(items, PlanOptions::default());
    for item in &mut plan.items {
        item.mode = DeleteMode::Quarantine;
    }
    Prepared { plan, jobs }
}

/// Reasons an item is held back that are only this module's judgment (an active
/// session, git state, a recent write…): the user may overrule them ("Delete
/// anyway"). Every other code — SafetyGate refusals, conversation history inside —
/// is a hard protection.
pub const SOFT_BLOCKS: &[&str] = &[
    "session-live",
    "session-open",
    "session-recent",
    "recent-write",
    "dirty",
    "not-merged",
    "not-removable",
    "locked",
    "unverified",
    "git-failed",
    "no-git",
    "foreign-worktree",
    "too-many-ignored",
    "too-large",
    "tracked",
    "not-ignored",
];

/// `true` when `code` is only a judgment the user may overrule ([`SOFT_BLOCKS`]).
#[must_use]
pub fn soft_block(code: &str) -> bool {
    SOFT_BLOCKS.contains(&code)
}

/// What "Delete anyway" does with the `picked` items of `report` that are held back
/// only by a judgment ([`soft_block`]): a registered worktree git may remove (clean,
/// merged or pushed) still goes the git way — ignored files to the quarantine, then
/// `git worktree remove`, the branch stays; anything else (uncommitted changes, an
/// unmerged branch, a live session's scratchpad) moves to the quarantine as a whole,
/// restorable for 7 days. Items with a hard block are left out and returned.
#[must_use]
pub fn prepare_forced(report: &Report, picked: &[PathBuf]) -> (Prepared, Vec<PathBuf>) {
    let wanted: Vec<String> = picked.iter().map(|p| norm(&p.to_string_lossy())).collect();
    let is_wanted = |p: &Path| wanted.contains(&norm(&p.to_string_lossy()));
    let mut items: Vec<PlanItem> = Vec::new();
    let mut jobs = Vec::new();
    let mut refused = Vec::new();
    for it in report.items.iter().filter(|i| is_wanted(&i.path)) {
        if it.blocked.as_deref().is_some_and(|b| !soft_block(b)) {
            refused.push(it.path.clone());
            continue;
        }
        match &it.worktree {
            Some(wt) if wt.registered && wt.removable => {
                for ig in &wt.ignored {
                    items.push(PlanItem::manual(
                        it.path.join(&ig.rel),
                        ig.is_dir,
                        ig.totals.bytes,
                        ig.totals.files,
                        ig.mtime,
                    ));
                }
                jobs.push(job_of(&it.path, wt, &it.totals));
            }
            _ => items.push(PlanItem::manual(
                it.path.clone(),
                is_dir(&it.path) || it.totals.files != 1,
                it.totals.bytes,
                it.totals.files,
                it.mtime,
            )),
        }
    }
    let mut plan = plan::build(items, PlanOptions::default());
    for item in &mut plan.items {
        item.mode = DeleteMode::Quarantine;
    }
    (Prepared { plan, jobs }, refused)
}

/// The paths "Clean finished sessions (safe only)" picks: every pre-selected item
/// and pre-selected build output of worktrees that cannot be removed.
#[must_use]
pub fn preselected(report: &Report) -> Vec<PathBuf> {
    report
        .items
        .iter()
        .filter(|i| i.selected)
        .map(|i| i.path.clone())
        .chain(
            report
                .items
                .iter()
                .filter_map(|i| i.worktree.as_ref())
                .flat_map(|w| &w.artifacts)
                .filter(|a| a.selected && a.blocked.is_none())
                .map(|a| a.path.clone()),
        )
        .collect()
}

#[cfg(test)]
mod tests;
