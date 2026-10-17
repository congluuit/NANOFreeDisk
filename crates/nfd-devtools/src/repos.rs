//! Repo Finder (PLAN §13.2 C3) with the build artifact policy (C9).
//!
//! Walks a folder the user chose — never following links, never leaving the volume
//! (links are not followed), never entering `.git`, `node_modules`, Windows or program
//! folders, and stopping at [`RepoOptions::max_depth`] — and lists every project: git
//! repositories, plus folders with a project marker outside any repository. For each
//! one: the size of `.git`, the build-output folders ([`crate::artifacts`]), the last
//! commit (read from `.git/logs/HEAD`, no git needed), the last activity and, when git
//! is installed, whether there are uncommitted changes.
//!
//! Safety of the proposals:
//! * a generic name (`build`, `dist`, `bin`, …) is only proposed when `git check-ignore`
//!   confirms it is ignored — no git, no proposal;
//! * a folder holding files tracked by git is never proposed;
//! * projects active in the last [`RepoOptions::keep_recent_days`] days keep their
//!   artifacts unselected;
//! * every folder passes the lexical SafetyGate as a hand-picked item, and deleting goes
//!   through the Planner → SafetyGate → Executor → quarantine ([`plan`]).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nfd_core::RiskLevel;
use nfd_core::plan::{self, CleanPlan, DeleteMode, PlanItem, PlanOptions};
use nfd_core::safety::{Origin, SafetyGate};
use nfd_platform::fsinfo::{self, LinkKind};
use rayon::prelude::*;
use serde::Serialize;

use crate::artifacts::{self, NEVER_DESCEND, Site};
use crate::fsutil::{self, Totals};
use crate::process;

/// Options of [`find`].
#[derive(Debug, Clone, Copy)]
pub struct RepoOptions {
    pub now: i64,
    /// Deepest folder level below the root that is visited.
    pub max_depth: usize,
    /// Projects active this recently keep their artifacts unselected (0 = off).
    pub keep_recent_days: u32,
    /// Projects without activity for this long are flagged stale.
    pub stale_days: u32,
    /// Ask git about uncommitted changes (one `git status` per repository).
    pub git_status: bool,
    /// Entries visited per folder when sizing.
    pub max_entries: u64,
}

impl Default for RepoOptions {
    fn default() -> Self {
        Self {
            now: nfd_rules::matcher::now(),
            max_depth: 8,
            keep_recent_days: 7,
            stale_days: 90,
            git_status: true,
            max_entries: 2_000_000,
        }
    }
}

/// A build-output folder of a project.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub path: PathBuf,
    /// Path relative to the project.
    pub rel: String,
    pub kind: &'static str,
    pub level: RiskLevel,
    #[serde(flatten)]
    pub totals: Totals,
    /// Last write of the folder itself.
    pub mtime: i64,
    /// Pre-selected for cleaning (🟢, project not recently active, allowed).
    pub selected: bool,
    /// Why it cannot be cleaned: a SafetyGate code, `tracked` (git tracks files in
    /// it) or `not-ignored` (a generic name git does not ignore / no git).
    pub blocked: Option<String>,
}

/// A project found by [`find`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub path: PathBuf,
    pub name: String,
    /// A git repository (else a project found by its marker files).
    pub git: bool,
    /// Project types found (`rust`, `node`, …).
    pub kinds: Vec<&'static str>,
    pub git_bytes: u64,
    pub last_commit: Option<i64>,
    pub last_commit_subject: Option<String>,
    /// Newest of the git metadata writes and the artifacts' contents.
    pub last_activity: i64,
    /// `Some(true)` when `git status` reports uncommitted changes.
    pub dirty: Option<bool>,
    pub recent: bool,
    pub stale: bool,
    pub artifacts: Vec<Artifact>,
    pub artifact_bytes: u64,
}

/// Everything [`find`] returns.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoScan {
    pub root: PathBuf,
    pub repos: Vec<Repo>,
    /// git was found on PATH (generic artifact names need it).
    pub git: bool,
    pub folders: u64,
    pub cancelled: bool,
}

struct Raw {
    path: PathBuf,
    git: bool,
    kinds: Vec<&'static str>,
    sites: Vec<(PathBuf, Site)>,
}

struct Walker<'a> {
    opts: RepoOptions,
    cancel: Option<&'a AtomicBool>,
    projects: Vec<Raw>,
    folders: u64,
}

fn listing(dir: &Path) -> Option<(Vec<String>, Vec<String>, bool)> {
    let items = fsinfo::read_dir(dir, false).ok()?;
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut git = false;
    for i in items {
        let Ok(info) = i.info else { continue };
        let name = i.name.to_string_lossy().into_owned();
        if name == ".git" {
            // a worktree / submodule has a `.git` file
            git = info.link == LinkKind::None;
            continue;
        }
        if info.link != LinkKind::None {
            continue;
        }
        if info.is_dir {
            dirs.push(name);
        } else {
            files.push(name);
        }
    }
    Some((files, dirs, git))
}

impl Walker<'_> {
    fn stopped(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
    }

    fn walk(&mut self, dir: &Path, depth: usize, project: Option<usize>) {
        if self.stopped() {
            return;
        }
        self.folders += 1;
        let Some((files, dirs, git)) = listing(dir) else {
            return;
        };
        let exists = |rel: &str| dir.join(rel).exists();
        let kinds: Vec<&'static str> = artifacts::kinds(&files, &exists)
            .iter()
            .map(|p| p.kind)
            .collect();
        let mut current = project;
        if git || (project.is_none() && !kinds.is_empty()) {
            self.projects.push(Raw {
                path: dir.to_path_buf(),
                git,
                kinds: Vec::new(),
                sites: Vec::new(),
            });
            current = Some(self.projects.len() - 1);
        }
        let sites = artifacts::sites(&files, &dirs, &exists);
        if let Some(i) = current {
            let p = &mut self.projects[i];
            for k in kinds {
                if !p.kinds.contains(&k) {
                    p.kinds.push(k);
                }
            }
            for s in &sites {
                p.sites.push((dir.join(&s.rel), s.clone()));
            }
        }
        if depth >= self.opts.max_depth {
            return;
        }
        for name in dirs {
            let lower = name.to_lowercase();
            if NEVER_DESCEND.contains(&lower.as_str())
                || name.starts_with('.')
                || sites
                    .iter()
                    .any(|s| s.rel.split('/').next() == Some(name.as_str()))
            {
                continue;
            }
            self.walk(&dir.join(&name), depth + 1, current);
        }
    }
}

/// Last line of `.git/logs/HEAD`: (time, subject).
fn last_commit(git_dir: &Path) -> Option<(i64, String)> {
    let text = std::fs::read_to_string(git_dir.join("logs").join("HEAD")).ok()?;
    let line = text.lines().rev().find(|l| !l.trim().is_empty())?;
    let (head, msg) = line.split_once('\t').unwrap_or((line, ""));
    let mut parts = head.split_whitespace().rev();
    let _tz = parts.next()?;
    let ts: i64 = parts.next()?.parse().ok()?;
    let subject: String = msg.chars().take(100).collect();
    Some((ts, subject))
}

/// The real git directory of a repository (`.git`, or where a `.git` file points).
pub(crate) fn git_dir(repo: &Path) -> PathBuf {
    let dot = repo.join(".git");
    if dot.is_file() {
        if let Ok(text) = std::fs::read_to_string(&dot) {
            if let Some(target) = text.trim().strip_prefix("gitdir:") {
                let p = PathBuf::from(target.trim());
                return if p.is_absolute() { p } else { repo.join(p) };
            }
        }
    }
    dot
}

fn mtime(path: &Path) -> i64 {
    fsinfo::file_info(path).map_or(0, |i| i.mtime)
}

fn git_lines(git: &Path, repo: &Path, args: &[&str]) -> Option<Vec<String>> {
    let mut all: Vec<String> = vec!["-C".into(), repo.to_string_lossy().into_owned()];
    all.extend(args.iter().map(|a| (*a).to_owned()));
    let out = process::run(git, &all, None, Duration::from_secs(20), None).ok()?;
    if out.code.is_none() || out.timed_out {
        return None;
    }
    Some(
        out.stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

/// Generic artifacts git ignores, and artifacts holding tracked files (relative paths,
/// `/`-separated).
pub(crate) fn git_checks(git: &Path, repo: &Path, rels: &[String]) -> (Vec<String>, Vec<String>) {
    if rels.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut args: Vec<&str> = vec!["check-ignore", "--"];
    args.extend(rels.iter().map(String::as_str));
    // exit code 1 = nothing ignored: still a valid (empty) answer
    let ignored = git_lines(git, repo, &args).unwrap_or_default();
    let mut args: Vec<&str> = vec!["ls-files", "--"];
    args.extend(rels.iter().map(String::as_str));
    let tracked_files = git_lines(git, repo, &args).unwrap_or_default();
    let tracked = rels
        .iter()
        .filter(|r| {
            let r = r.replace('\\', "/");
            tracked_files
                .iter()
                .any(|f| f == &r || f.starts_with(&format!("{r}/")))
        })
        .cloned()
        .collect();
    let ignored = ignored
        .into_iter()
        .map(|l| l.trim_end_matches('/').to_owned())
        .collect();
    (ignored, tracked)
}

fn rel_of(project: &Path, path: &Path) -> String {
    path.strip_prefix(project)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn finish(raw: Raw, git: Option<&Path>, gate: &SafetyGate, opts: &RepoOptions) -> Repo {
    let name = raw.path.file_name().map_or_else(
        || raw.path.to_string_lossy().into_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    let mut repo = Repo {
        name,
        path: raw.path.clone(),
        git: raw.git,
        kinds: raw.kinds,
        git_bytes: 0,
        last_commit: None,
        last_commit_subject: None,
        last_activity: 0,
        dirty: None,
        recent: false,
        stale: false,
        artifacts: Vec::new(),
        artifact_bytes: 0,
    };
    let rels: Vec<String> = raw
        .sites
        .iter()
        .map(|(p, _)| rel_of(&raw.path, p))
        .collect();
    let (ignored, tracked) = match (raw.git, git) {
        (true, Some(g)) => git_checks(g, &raw.path, &rels),
        _ => (Vec::new(), Vec::new()),
    };
    if raw.git {
        let gd = git_dir(&raw.path);
        repo.git_bytes = fsutil::totals(&gd, opts.max_entries).bytes;
        if let Some((ts, subject)) = last_commit(&gd) {
            repo.last_commit = Some(ts);
            repo.last_commit_subject = Some(subject);
        }
        repo.last_activity = ["index", "HEAD", "FETCH_HEAD", "ORIG_HEAD"]
            .iter()
            .map(|f| mtime(&gd.join(f)))
            .chain([mtime(&gd.join("logs").join("HEAD"))])
            .max()
            .unwrap_or(0);
        if opts.git_status {
            if let Some(g) = git {
                repo.dirty = git_lines(
                    g,
                    &raw.path,
                    &["status", "--porcelain", "--untracked-files=no"],
                )
                .map(|l| !l.is_empty());
            }
        }
    }
    for ((path, site), rel) in raw.sites.into_iter().zip(rels) {
        let totals = fsutil::totals(&path, opts.max_entries);
        let level = artifacts::level_for(&site, totals.bytes);
        let blocked = if tracked.contains(&rel) {
            Some("tracked".to_owned())
        } else if site.generic && !ignored.iter().any(|i| i.eq_ignore_ascii_case(&rel)) {
            Some("not-ignored".to_owned())
        } else {
            gate.check_path(&path, Origin::Manual)
                .err()
                .map(|b| b.code().to_owned())
        };
        repo.last_activity = repo.last_activity.max(totals.newest);
        repo.artifacts.push(Artifact {
            mtime: mtime(&path),
            rel,
            kind: site.kind,
            level,
            totals,
            selected: false,
            blocked,
            path,
        });
    }
    if !raw.git {
        repo.last_activity = repo.last_activity.max(mtime(&raw.path));
    }
    let age = opts.now - repo.last_activity;
    repo.recent = opts.keep_recent_days > 0 && age < i64::from(opts.keep_recent_days) * 86_400;
    repo.stale = opts.stale_days > 0 && age > i64::from(opts.stale_days) * 86_400;
    for a in &mut repo.artifacts {
        a.selected = a.blocked.is_none() && a.level == RiskLevel::Safe && !repo.recent;
    }
    repo.artifacts
        .sort_by_key(|a| std::cmp::Reverse(a.totals.bytes));
    repo.artifact_bytes = repo
        .artifacts
        .iter()
        .filter(|a| a.blocked.is_none())
        .map(|a| a.totals.bytes)
        .sum();
    repo
}

/// Find the projects under `root`. `git` is the git executable (`None` = no generic
/// artifacts, no dirty check).
#[must_use]
pub fn find(
    root: &Path,
    opts: RepoOptions,
    git: Option<&Path>,
    gate: &SafetyGate,
    cancel: Option<&AtomicBool>,
) -> RepoScan {
    let mut w = Walker {
        opts,
        cancel,
        projects: Vec::new(),
        folders: 0,
    };
    w.walk(root, 0, None);
    let cancelled = w.stopped();
    let folders = w.folders;
    let mut repos: Vec<Repo> = w
        .projects
        .into_par_iter()
        .map(|raw| finish(raw, git, gate, &opts))
        .collect();
    repos.sort_by(|a, b| {
        b.artifact_bytes
            .cmp(&a.artifact_bytes)
            .then(a.path.cmp(&b.path))
    });
    RepoScan {
        root: root.to_path_buf(),
        repos,
        git: git.is_some(),
        folders,
        cancelled,
    }
}

/// A plan deleting `artifacts` (hand-picked: the SafetyGate allows them inside user
/// folders) into the **quarantine**, restorable from the Activity log.
#[must_use]
pub fn plan<'a>(artifacts: impl IntoIterator<Item = &'a Artifact>) -> CleanPlan {
    let items = artifacts
        .into_iter()
        .filter(|a| a.blocked.is_none())
        .map(|a| {
            PlanItem::manual(
                a.path.clone(),
                true,
                a.totals.bytes,
                a.totals.files,
                a.mtime,
            )
        })
        .collect();
    let mut plan = plan::build(items, PlanOptions::default());
    for item in &mut plan.items {
        item.mode = DeleteMode::Quarantine;
    }
    plan
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, vec![b'x'; bytes]).expect("write");
    }

    fn gate() -> SafetyGate {
        SafetyGate::new(&|_: &str| None)
    }

    fn opts() -> RepoOptions {
        RepoOptions {
            keep_recent_days: 0,
            git_status: false,
            ..RepoOptions::default()
        }
    }

    #[test]
    fn finds_projects_and_their_build_output_without_git() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        // a repo (git metadata faked) with a Rust and a Node part
        write(&root.join("app/.git/HEAD"), 10);
        write(&root.join("app/.git/logs/HEAD"), 0);
        fs::write(
            root.join("app/.git/logs/HEAD"),
            "0000 1111 Dev <d@x> 1700000000 +0700\tcommit (initial): first\n",
        )
        .expect("w");
        write(&root.join("app/Cargo.toml"), 10);
        write(&root.join("app/target/debug/app.exe"), 1000);
        write(&root.join("app/web/package.json"), 10);
        write(&root.join("app/web/node_modules/x/index.js"), 500);
        write(&root.join("app/web/dist/bundle.js"), 300);
        // a Unity project without git
        write(&root.join("game/ProjectSettings/ProjectVersion.txt"), 10);
        write(&root.join("game/Library/a.bin"), 2000);
        write(&root.join("game/Temp/t.bin"), 10);
        // python caches anywhere inside a project
        write(&root.join("app/tools/__pycache__/m.pyc"), 10);
        // not a project
        write(&root.join("photos/target/x.jpg"), 10);

        let scan = find(root, opts(), None, &gate(), None);
        assert!(!scan.git);
        let names: Vec<&str> = scan.repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        let app = scan.repos.iter().find(|r| r.name == "app").expect("app");
        assert!(app.git);
        assert_eq!(app.last_commit, Some(1_700_000_000));
        assert_eq!(
            app.last_commit_subject.as_deref(),
            Some("commit (initial): first")
        );
        assert!(app.kinds.contains(&"rust") && app.kinds.contains(&"node"));
        let rels: Vec<&str> = app.artifacts.iter().map(|a| a.rel.as_str()).collect();
        assert!(rels.contains(&"target"));
        assert!(rels.contains(&"web/node_modules"));
        assert!(rels.contains(&"tools/__pycache__"));
        // a generic name cannot be confirmed without git
        let dist = app
            .artifacts
            .iter()
            .find(|a| a.rel == "web/dist")
            .expect("dist");
        assert_eq!(dist.blocked.as_deref(), Some("not-ignored"));
        assert!(!dist.selected);
        let target = app
            .artifacts
            .iter()
            .find(|a| a.rel == "target")
            .expect("target");
        assert!(target.selected);
        assert_eq!(target.totals.files, 1);

        let game = scan.repos.iter().find(|r| r.name == "game").expect("game");
        assert!(!game.git);
        let lib = game
            .artifacts
            .iter()
            .find(|a| a.rel == "Library")
            .expect("lib");
        assert_eq!(lib.level, RiskLevel::Caution);
        assert!(!lib.selected, "caution is never pre-selected");

        let p = plan(app.artifacts.iter().filter(|a| a.selected));
        assert!(p.items.iter().all(|i| i.mode == DeleteMode::Quarantine));
        assert!(!p.items.iter().any(|i| i.path.ends_with("dist")));
    }

    #[test]
    fn recent_projects_keep_their_artifacts_unselected() {
        let dir = tempfile::tempdir().expect("tmp");
        write(&dir.path().join("p/Cargo.toml"), 1);
        write(&dir.path().join("p/target/x"), 1);
        let o = RepoOptions {
            keep_recent_days: 7,
            ..opts()
        };
        let scan = find(dir.path(), o, None, &gate(), None);
        let p = &scan.repos[0];
        assert!(p.recent);
        assert!(!p.artifacts[0].selected);
        assert!(p.artifacts[0].blocked.is_none());
    }

    #[test]
    fn with_git_generic_names_need_ignore_and_tracked_files_are_kept() {
        let Some(git) = process::git() else { return };
        let dir = tempfile::tempdir().expect("tmp");
        let repo = dir.path().join("r");
        write(&repo.join("package.json"), 2);
        write(&repo.join("dist/a.js"), 2);
        write(&repo.join("build/b.js"), 2);
        write(&repo.join("node_modules/m/i.js"), 2);
        fs::write(repo.join(".gitignore"), "dist/\nnode_modules/\n").expect("w");
        let run = |args: &[&str]| {
            let mut all = vec!["-C".to_owned(), repo.to_string_lossy().into_owned()];
            all.extend(args.iter().map(|a| (*a).to_owned()));
            process::run(&git, &all, None, Duration::from_secs(30), None).expect("git")
        };
        if !run(&["init", "-q"]).success() {
            return;
        }
        // node_modules is ignored, but one file in it was force-added
        run(&["add", "-f", "node_modules/m/i.js", "package.json"]);
        let scan = find(dir.path(), opts(), Some(&git), &gate(), None);
        let r = &scan.repos[0];
        let get = |rel: &str| r.artifacts.iter().find(|a| a.rel == rel).expect(rel);
        assert_eq!(get("dist").blocked, None, "ignored generic folder");
        assert_eq!(get("build").blocked.as_deref(), Some("not-ignored"));
        assert_eq!(get("node_modules").blocked.as_deref(), Some("tracked"));
    }
}
