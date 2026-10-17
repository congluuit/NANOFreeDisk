//! Fixtures for AI Leftovers: a fake home with Claude Code / desktop sessions, a
//! project with agent output, a `%TEMP%\claude` and a Playwright store; real git
//! repositories with worktrees when git is installed. Nothing outside the temporary
//! folders is touched.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nfd_core::exec::{self, ExecOptions, ItemStatus, QuarantineRoots};
use nfd_core::safety::{AGENT_HISTORY_NAMES, AGENT_HISTORY_SUFFIXES, LiveProbe};
use proptest::prelude::*;

use super::*;
use crate::process;

const DAY: i64 = 86_400;

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, vec![b'x'; bytes]).expect("write");
}

fn write_text(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write");
}

fn gate() -> SafetyGate {
    SafetyGate::new(&|_: &str| None)
}

/// `now` a month after the fixture was written: nothing is "recent".
fn later() -> Options {
    Options {
        now: exec::unix_now() + 30 * DAY,
        ..Options::default()
    }
}

fn json_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "\\\\")
}

/// A Claude Code transcript for `cwd`.
fn transcript(home: &Path, id: &str, cwd: &Path, title: &str) {
    write_text(
        &home.join(format!(".claude/projects/x/{id}.jsonl")),
        &format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\",\"timestamp\":\"2020-01-01T00:00:00Z\"}}\n{{\"type\":\"custom-title\",\"customTitle\":\"{title}\"}}\n",
            json_path(cwd)
        ),
    );
}

/// A desktop-app session for transcript `id`.
fn desktop(home: &Path, id: &str, cwd: &Path, archived: bool) {
    write_text(
        &home.join(format!("desk/acc/org/local_{id}.json")),
        &format!(
            "{{\"sessionId\":\"local_{id}\",\"cliSessionId\":\"{id}\",\"cwd\":\"{}\",\"title\":\"Desk {id}\",\"createdAt\":1000,\"lastActivityAt\":2000000,\"isArchived\":{archived}}}",
            json_path(cwd)
        ),
    );
}

fn sources(home: &Path) -> Sources {
    Sources {
        sessions: SessionSources {
            claude_projects: vec![home.join(".claude/projects")],
            desktop: vec![home.join("desk")],
            codex: Vec::new(),
        },
        temp_claude: vec![home.join("temp/claude")],
        playwright: vec![home.join("ms-playwright")],
        not_projects: vec![home.join("temp")],
    }
}

fn item<'a>(r: &'a Report, rel: &str) -> &'a Item {
    r.items.iter().find(|i| i.rel == rel).unwrap_or_else(|| {
        panic!(
            "{rel} not in {:#?}",
            r.items.iter().map(|i| &i.rel).collect::<Vec<_>>()
        )
    })
}

#[test]
fn finds_project_temp_and_browser_leftovers_and_keeps_history() {
    let dir = tempfile::tempdir().expect("tmp");
    let home = dir.path();
    let proj = home.join("proj");
    write(&proj.join("README.md"), 3);
    write(&proj.join(".playwright-mcp/console-1.log"), 100);
    write(&proj.join(".aider.tags.cache.v4/cache.db"), 50);
    write(&proj.join(".aider.chat.history.md"), 10);
    write(&proj.join(".specstory/history/2026-10-16.md"), 10);
    // Codex home moved into the project: logs go, sessions stay
    write(&proj.join(".codex/log/codex-tui.log"), 20);
    write(&proj.join(".codex/sessions/2026/rollout.jsonl"), 20);
    write(&proj.join(".codex/config.toml"), 5);
    // agent output holding a chat log: refused as a whole
    write(&proj.join("web/.playwright-mcp/page.yml"), 30);
    write(&proj.join("web/.playwright-mcp/.specstory/chat.md"), 30);
    // a common name without git: somebody's folder, not offered
    write(&proj.join("screenshots/me.png"), 40);
    transcript(
        home,
        "0a0a0a0a-0000-4000-8000-00000000000a",
        &proj,
        "Old work",
    );

    let temp = home.join("temp/claude/D--proj");
    write(
        &temp.join("0a0a0a0a-0000-4000-8000-00000000000a/scratchpad/build.log"),
        200,
    );
    write(
        &temp.join("0b0b0b0b-0000-4000-8000-00000000000b/tasks/x.output"),
        10,
    );
    write(&home.join("temp/claude/bash-edit-diff/123-456/a.diff"), 10);
    write(&home.join("temp/claude/scr/notes.txt"), 10);

    let pw = home.join("ms-playwright");
    for d in [
        "chromium-1223",
        "chromium-1248",
        "ffmpeg-1011",
        "ffmpeg-1013",
        "mcp-chrome-ab12",
        "b",
    ] {
        write(&pw.join(d).join("x"), 10);
    }

    let r = scan(&later(), &sources(home), None, &gate(), None);
    assert_eq!(r.projects, vec![proj.clone()]);
    let mcp = item(&r, ".playwright-mcp");
    assert_eq!(mcp.level, RiskLevel::Safe);
    assert!(mcp.selected, "{mcp:#?}");
    assert_eq!(
        mcp.session.as_ref().map(|s| s.id.as_str()),
        Some("0a0a0a0a-0000-4000-8000-00000000000a")
    );
    assert!(item(&r, ".aider.tags.cache.v4").selected);
    assert!(item(&r, ".codex/log").selected);
    let nested = item(&r, "web/.playwright-mcp");
    assert_eq!(nested.blocked.as_deref(), Some("contains-history"));
    assert!(!nested.selected);
    assert!(r.items.iter().all(|i| i.rel != "screenshots"));
    let kept: Vec<&str> = r.kept.iter().map(|k| k.what.as_str()).collect();
    assert!(kept.contains(&".aider.chat.history.md"), "{kept:?}");
    assert!(kept.contains(&".specstory"), "{kept:?}");
    assert!(kept.contains(&".codex/sessions"), "{kept:?}");

    let scratch = item(&r, "claude/D--proj/0a0a0a0a-0000-4000-8000-00000000000a");
    assert_eq!(
        scratch.session.as_ref().and_then(|s| s.title.as_deref()),
        Some("Old work")
    );
    assert_eq!(scratch.project.as_deref(), Some(proj.as_path()));
    assert!(scratch.selected);
    assert!(
        item(&r, "claude/D--proj/0b0b0b0b-0000-4000-8000-00000000000b")
            .session
            .is_none()
    );
    assert!(item(&r, "claude/bash-edit-diff").selected);
    let odd = item(&r, "claude/scr");
    assert_eq!(
        odd.level,
        RiskLevel::Caution,
        "not Claude Code's own folder"
    );
    assert!(!odd.selected);
    assert!(is_claude_temp_name("D--Working-app") && is_claude_temp_name("-home-me"));
    assert!(is_session_id("3f84adc8-0277-448e-b6ab-422b34ca8709"));
    assert!(!is_session_id("bash-edit-diff"));

    let browsers: Vec<&str> = r
        .items
        .iter()
        .filter(|i| i.kind == Kind::BrowserRevision)
        .map(|i| i.rel.as_str())
        .collect();
    assert_eq!(browsers.len(), 2, "{browsers:?}");
    assert!(browsers.contains(&"chromium-1223") && browsers.contains(&"ffmpeg-1011"));

    // everything pre-selected lands in a quarantine plan; no history inside
    let prepared = prepare(&r, &preselected(&r));
    assert!(prepared.jobs.is_empty());
    assert!(!prepared.plan.items.is_empty());
    for it in &prepared.plan.items {
        assert_eq!(it.mode, DeleteMode::Quarantine);
        let m = measure(&it.path, 10_000);
        assert!(m.history.is_none(), "{}", it.path.display());
    }
    assert!(
        !prepared
            .plan
            .items
            .iter()
            .any(|i| i.path.ends_with(".playwright-mcp") && i.path.starts_with(proj.join("web")))
    );
    assert_eq!(
        browser_revision("chromium_headless_shell-1248"),
        Some(("chromium_headless_shell".into(), 1248))
    );
    assert_eq!(browser_revision("mcp-chrome-1234"), None);
}

#[test]
fn active_or_open_sessions_are_never_preselected() {
    let dir = tempfile::tempdir().expect("tmp");
    let home = dir.path();
    let proj = home.join("proj");
    write(&proj.join("README.md"), 3);
    let temp = home.join("temp/claude/D--proj");
    write(
        &temp.join("0c0c0c0c-0000-4000-8000-00000000000c/scratchpad/a"),
        10,
    );
    write(
        &temp.join("0d0d0d0d-0000-4000-8000-00000000000d/scratchpad/a"),
        10,
    );
    write(
        &temp.join("0e0e0e0e-0000-4000-8000-00000000000e/scratchpad/a"),
        10,
    );
    transcript(home, "0c0c0c0c-0000-4000-8000-00000000000c", &proj, "Open");
    transcript(home, "0d0d0d0d-0000-4000-8000-00000000000d", &proj, "Now");
    transcript(home, "0e0e0e0e-0000-4000-8000-00000000000e", &proj, "Done");
    desktop(home, "0c0c0c0c-0000-4000-8000-00000000000c", &proj, false);
    desktop(home, "0e0e0e0e-0000-4000-8000-00000000000e", &proj, true);

    // a month later: only the open (not archived) session keeps its scratchpad
    let r = scan(&later(), &sources(home), None, &gate(), None);
    let open = item(&r, "claude/D--proj/0c0c0c0c-0000-4000-8000-00000000000c");
    assert_eq!(open.keep, Some("session-open"));
    assert!(!open.selected && open.blocked.is_none());
    assert!(item(&r, "claude/D--proj/0e0e0e0e-0000-4000-8000-00000000000e").selected);
    assert!(item(&r, "claude/D--proj/0d0d0d0d-0000-4000-8000-00000000000d").selected);

    // right now: the session that just wrote its transcript is recent
    let now = Options {
        now: exec::unix_now(),
        ..Options::default()
    };
    let r = scan(&now, &sources(home), None, &gate(), None);
    for rel in [
        "claude/D--proj/0d0d0d0d-0000-4000-8000-00000000000d",
        "claude/D--proj/0c0c0c0c-0000-4000-8000-00000000000c",
        "claude/D--proj/0e0e0e0e-0000-4000-8000-00000000000e",
    ] {
        let it = item(&r, rel);
        assert!(!it.selected, "{rel}: {:?}", it.keep);
        assert!(it.keep.is_some());
    }
    assert!(preselected(&r).is_empty());
}

#[test]
fn every_agent_history_location_is_refused() {
    use nfd_core::safety::AGENT_HISTORY_TEMPLATES;
    let base = if cfg!(windows) {
        "C:/Users/nano"
    } else {
        "/home/nano"
    };
    let resolve = move |v: &str| {
        Some(match v {
            "HOME" | "USERPROFILE" => base.to_owned(),
            "APPDATA" => format!("{base}/AppData/Roaming"),
            "LOCALAPPDATA" => format!("{base}/AppData/Local"),
            _ => return None,
        })
    };
    let gate = SafetyGate::new(&resolve);
    for t in AGENT_HISTORY_TEMPLATES {
        let (var, tail) = t[2..].split_once('}').expect("template");
        let Some(b) = resolve(var) else { continue };
        let p = PathBuf::from(format!("{b}{}", tail.replace('*', "1")));
        let m = Measure::default();
        assert!(refuse(&gate, &p, &m).is_some(), "{t}");
        assert!(refuse(&gate, &p.join("x.jsonl"), &m).is_some(), "{t}");
    }
}

/// A path component in random case.
fn cased(name: &str, upper: &[bool]) -> String {
    name.chars()
        .zip(upper.iter().cycle())
        .map(|(c, u)| if *u { c.to_ascii_uppercase() } else { c })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// However deep and in whatever case a chat log sits inside a leftover folder,
    /// the folder is refused and nothing that holds history reaches the plan.
    #[test]
    fn history_inside_a_leftover_never_reaches_the_plan(
        which in 0usize..(AGENT_HISTORY_NAMES.len() + AGENT_HISTORY_SUFFIXES.len()),
        depth in 0usize..4,
        upper in proptest::collection::vec(any::<bool>(), 1..8),
        leftover in 0usize..3,
    ) {
        let dir = tempfile::tempdir().expect("tmp");
        let home = dir.path();
        let proj = home.join("proj");
        write(&proj.join("README.md"), 1);
        let base = [".playwright-mcp", ".aider.tags.cache.v4", ".codex/log"][leftover];
        let mut inner = proj.join(base);
        for d in 0..depth {
            inner = inner.join(format!("d{d}"));
        }
        let hist = if which < AGENT_HISTORY_NAMES.len() {
            inner.join(cased(AGENT_HISTORY_NAMES[which], &upper))
        } else {
            let (f, n) = AGENT_HISTORY_SUFFIXES[which - AGENT_HISTORY_NAMES.len()];
            inner.join(cased(f, &upper)).join(n)
        };
        write(&hist.join("chat.jsonl"), 5);
        write(&proj.join(base).join("other.log"), 5);
        transcript(home, "s1", &proj, "Work");
        let r = scan(&later(), &sources(home), None, &gate(), None);
        let all: Vec<PathBuf> = r.items.iter().map(|i| i.path.clone()).collect();
        let prepared = prepare(&r, &all);
        for it in &prepared.plan.items {
            prop_assert!(measure(&it.path, 10_000).history.is_none(), "{}", it.path.display());
            prop_assert!(!hist.starts_with(&it.path), "{} holds {}", it.path.display(), hist.display());
        }
        let hit = r.items.iter().find(|i| proj.join(base) == i.path);
        prop_assert!(hit.is_some_and(|i| i.blocked.as_deref() == Some("contains-history") && !i.selected));
    }
}

// ───────────────────────────── with git ─────────────────────────────

struct Git(PathBuf);

impl Git {
    fn new() -> Option<Self> {
        process::git().map(Self)
    }

    fn run(&self, dir: &Path, args: &[&str]) -> process::RunOutput {
        let mut all = vec![
            "-c".to_owned(),
            "user.name=t".to_owned(),
            "-c".to_owned(),
            "user.email=t@example.com".to_owned(),
            "-c".to_owned(),
            "commit.gpgsign=false".to_owned(),
            "-C".to_owned(),
            dir.to_string_lossy().into_owned(),
        ];
        all.extend(args.iter().map(|a| (*a).to_owned()));
        let out = process::run(&self.0, &all, None, Duration::from_secs(60), None).expect("git");
        assert!(out.success(), "git {args:?}: {}", out.stderr);
        out
    }
}

/// A repository on `main` with `.claude/worktrees/` ignored, one commit.
fn repo(git: &Git, root: &Path) -> PathBuf {
    let repo = root.join("repo");
    write_text(
        &repo.join(".gitignore"),
        "target/\n.claude/worktrees/\ntest-results/\n",
    );
    write_text(&repo.join("Cargo.toml"), "[package]\n");
    write_text(&repo.join("docs/screenshots/shot.png"), "png");
    git.run(&repo, &["init", "-q", "-b", "main"]);
    git.run(&repo, &["add", "."]);
    git.run(&repo, &["commit", "-q", "-m", "init"]);
    repo
}

fn add_worktree(git: &Git, repo: &Path, name: &str) -> PathBuf {
    let wt = repo.join(".claude/worktrees").join(name);
    git.run(
        repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("cong/{name}"),
            &wt.to_string_lossy(),
        ],
    );
    write(&wt.join("target/debug/app.exe"), 4096);
    wt
}

fn quarantine_opts(dir: &Path) -> ExecOptions {
    ExecOptions {
        op_id: "20261017-000000-a1ef".into(),
        kind: "ai-leftovers".into(),
        owner: 0,
        roots: QuarantineRoots {
            system: Some(dir.join("q")),
            other_volumes: false,
        },
        now: exec::unix_now(),
    }
}

#[test]
fn worktrees_are_removed_by_git_only_when_clean_and_merged() {
    let Some(git) = Git::new() else { return };
    let dir = tempfile::tempdir().expect("tmp");
    let home = dir.path();
    let repo = repo(&git, home);
    let done = add_worktree(&git, &repo, "done");
    let dirty = add_worktree(&git, &repo, "dirty");
    write_text(&dirty.join("Cargo.toml"), "[package]\nname = \"changed\"\n");
    let untracked = add_worktree(&git, &repo, "untracked");
    write_text(&untracked.join("notes.txt"), "my work");
    let ahead = add_worktree(&git, &repo, "ahead");
    write_text(&ahead.join("src.rs"), "fn main() {}");
    git.run(&ahead, &["add", "."]);
    git.run(&ahead, &["commit", "-q", "-m", "unmerged work"]);
    let orphan = repo.join(".claude/worktrees/orphan");
    write(&orphan.join("target/x"), 10);
    // generic names inside the repository itself
    write(&repo.join("test-results/trace.zip"), 100);
    write(&repo.join("scratch/tmp.txt"), 100);
    transcript(
        home,
        "0e0e0e0e-0000-4000-8000-00000000000e",
        &done,
        "Finished work",
    );
    let mut src = sources(home);
    src.not_projects.clear();

    let r = scan(&later(), &src, Some(&git.0), &gate(), None);
    let get = |name: &str| item(&r, &format!(".claude/worktrees/{name}"));
    let d = get("done");
    assert_eq!(d.blocked, None, "{d:#?}");
    assert_eq!(d.level, RiskLevel::Safe);
    assert!(d.selected);
    assert_eq!(
        d.session.as_ref().and_then(|s| s.title.as_deref()),
        Some("Finished work")
    );
    let wt = d.worktree.as_ref().expect("wt");
    assert!(wt.merged && wt.removable);
    assert_eq!(wt.branch.as_deref(), Some("cong/done"));
    assert!(
        wt.ignored.iter().any(|i| i.rel == "target"),
        "{:?}",
        wt.ignored
    );

    for (name, why) in [
        ("dirty", "dirty"),
        ("untracked", "dirty"),
        ("ahead", "not-merged"),
    ] {
        let it = get(name);
        assert_eq!(it.blocked.as_deref(), Some(why), "{name}");
        assert!(!it.selected);
        assert_eq!(it.level, RiskLevel::Caution);
        // its build output may still go (C9)
        let w = it.worktree.as_ref().expect("wt");
        assert!(
            w.artifacts.iter().any(|a| a.rel == "target"),
            "{name}: {:?}",
            w.artifacts
        );
    }
    let o = get("orphan");
    assert_eq!(o.level, RiskLevel::Caution);
    assert!(!o.selected && !o.worktree.as_ref().expect("wt").registered);

    // common names: ignored → 🟢, untracked → 🟡, tracked → never offered
    assert_eq!(item(&r, "test-results").level, RiskLevel::Safe);
    assert_eq!(item(&r, "scratch").level, RiskLevel::Caution);
    assert!(!item(&r, "scratch").selected);
    assert!(r.items.iter().all(|i| i.rel != "docs/screenshots"));

    // picking everything never touches a dirty or unmerged worktree
    let all: Vec<PathBuf> = r.items.iter().map(|i| i.path.clone()).collect();
    let prepared = prepare(&r, &all);
    assert_eq!(prepared.jobs.len(), 1, "{:?}", prepared.jobs);
    assert_eq!(prepared.jobs[0].path, done);
    for blocked in [&dirty, &untracked, &ahead] {
        assert!(
            !prepared
                .plan
                .items
                .iter()
                .any(|i| i.path.starts_with(blocked))
        );
    }

    // the clean: ignored files to the quarantine, then git removes the worktree
    let q = quarantine_opts(home);
    let plan = prepare(&r, std::slice::from_ref(&done));
    let op = exec::execute(&plan.plan, &gate(), &LiveProbe, &q, &mut |_, _, _| true);
    assert!(
        op.items.iter().all(|i| i.status == ItemStatus::Done),
        "{op:#?}"
    );
    let res = worktrees::remove(&git.0, &plan.jobs[0], 0, None);
    assert_eq!(res.status, ItemStatus::Done, "{res:#?}");
    assert!(!done.exists());
    assert!(
        res.message
            .as_deref()
            .is_some_and(|m| m.contains("worktree add"))
    );
    let branches = git.run(&repo, &["branch", "--list", "cong/done"]).stdout;
    assert!(
        branches.contains("cong/done"),
        "the branch is never deleted"
    );
    let listed = git.run(&repo, &["worktree", "list", "--porcelain"]).stdout;
    assert!(!listed.contains("worktrees/done"));
    // the build output is restorable from the quarantine
    assert!(op.items[0].stored.as_ref().is_some_and(|s| s.exists()));
    assert!(dirty.exists() && untracked.exists() && ahead.exists());
}

#[test]
fn a_worktree_that_changed_since_the_plan_is_left_alone() {
    let Some(git) = Git::new() else { return };
    let dir = tempfile::tempdir().expect("tmp");
    let home = dir.path();
    let repo = repo(&git, home);
    let wt = add_worktree(&git, &repo, "late");
    let mut src = sources(home);
    src.not_projects.clear();
    // no session knows the repository: found under the Repo Finder root
    let opts = Options {
        roots: vec![home.to_path_buf()],
        ..later()
    };
    let r = scan(&opts, &src, Some(&git.0), &gate(), None);
    let prepared = prepare(&r, std::slice::from_ref(&wt));
    assert_eq!(
        prepared.jobs.len(),
        1,
        "{:#?}",
        item(&r, ".claude/worktrees/late")
    );
    // the user (or the agent) starts editing after the scan
    write_text(&wt.join("new.rs"), "work in progress");
    let res = worktrees::remove(&git.0, &prepared.jobs[0], 0, None);
    assert_eq!(res.status, ItemStatus::Skipped);
    assert_eq!(res.reason.as_deref(), Some("dirty"));
    assert!(wt.join("new.rs").exists());
    // ignored files that were not moved away block the removal too
    fs::remove_file(wt.join("new.rs")).expect("rm");
    let res = worktrees::remove(&git.0, &prepared.jobs[0], 0, None);
    assert_eq!(res.reason.as_deref(), Some("ignored-left"));
    assert!(wt.join("target/debug/app.exe").exists());
}

#[test]
fn a_recent_session_keeps_its_worktree() {
    let Some(git) = Git::new() else { return };
    let dir = tempfile::tempdir().expect("tmp");
    let home = dir.path();
    let repo = repo(&git, home);
    let wt = add_worktree(&git, &repo, "busy");
    transcript(home, "s-busy", &wt, "Busy");
    desktop(home, "s-busy", &wt, false);
    let mut src = sources(home);
    src.not_projects.clear();
    let r = scan(&later(), &src, Some(&git.0), &gate(), None);
    let it = item(&r, ".claude/worktrees/busy");
    assert!(it.blocked.is_none());
    assert_eq!(it.keep, Some("session-open"));
    assert!(!it.selected);
    assert!(preselected(&r).iter().all(|p| !p.starts_with(&wt)));
}

#[test]
fn a_live_session_blocks_its_worktree_and_scratchpad() {
    let Some(git) = Git::new() else { return };
    let dir = tempfile::tempdir().expect("tmp");
    let home = dir.path();
    let repo = repo(&git, home);
    let wt = add_worktree(&git, &repo, "live");
    write_text(&wt.join("wip.rs"), "work in progress");
    let id = "0f0f0f0f-0000-4000-8000-00000000000f";
    transcript(home, id, &wt, "Working right now");
    write(
        &home.join(format!("temp/claude/D--live/{id}/scratchpad/x")),
        10,
    );
    let mut src = sources(home);
    src.not_projects.clear();
    let now = Options {
        now: exec::unix_now(),
        ..Options::default()
    };
    let r = scan(&now, &src, Some(&git.0), &gate(), None);
    let w = item(&r, ".claude/worktrees/live");
    assert_eq!(w.blocked.as_deref(), Some("session-live"));
    assert!(
        w.worktree.as_ref().expect("wt").artifacts.is_empty(),
        "not even its build output"
    );
    let t = item(&r, &format!("claude/D--live/{id}"));
    assert_eq!(t.blocked.as_deref(), Some("session-live"));
    // even when picked by hand
    let prepared = prepare(&r, &[wt.clone(), t.path.clone()]);
    assert!(prepared.jobs.is_empty() && prepared.plan.items.is_empty());
}
