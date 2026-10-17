use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use nfd_core::RiskLevel;
use nfd_core::model::{ChildIndex, subtree_ends};
use nfd_core::scanner::{self, ScanOptions, ScanTree};
use nfd_devtools::ai_leftovers::catalog::{Kind, Scope};
use nfd_devtools::ai_leftovers::{Item, Report, Worktree};
use nfd_devtools::fsutil::Totals;
use nfd_rules::matcher::MatchOptions;

use super::*;

const PACK: &str = r#"
[pack]
id = "test"
name = "test"
version = "0.0.1"

[[rule]]
id = "test.temp"
vendor = "Microsoft"
app = "Windows"
category = "system"
platform = ["windows", "linux", "macos"]
paths = ["${TEMP}/**"]
level = "safe"
min_age_days = 1
impact = "Temporary files."
impact_vi = "Tệp tạm."
verified_on = 2026-10-17

[[rule]]
id = "test.wintemp"
vendor = "Microsoft"
app = "Windows"
category = "system"
platform = ["windows", "linux", "macos"]
paths = ["${SYSTEMROOT}/Temp/**"]
level = "safe"
impact = "Temporary files of Windows."
impact_vi = "Tệp tạm của Windows."
verified_on = 2026-10-17

[[rule]]
id = "test.cache"
vendor = "Vendor"
app = "App"
category = "app"
platform = ["windows", "linux", "macos"]
paths = ["${LOCALAPPDATA}/App/Cache/**"]
level = "safe"
impact = "The app's cache."
impact_vi = "Bộ đệm của ứng dụng."
verified_on = 2026-10-17

[[rule]]
id = "test.keep"
vendor = "Vendor"
app = "App"
category = "app"
platform = ["windows", "linux", "macos"]
paths = ["${LOCALAPPDATA}/App/Cache/keep/**"]
level = "protected"
impact = "Settings the app keeps in its cache folder."
impact_vi = "Thiết lập ứng dụng để trong thư mục cache."
verified_on = 2026-10-17

[[rule]]
id = "test.npm"
vendor = "npm"
app = "npm"
category = "package-manager"
platform = ["windows", "linux", "macos"]
paths = ["${LOCALAPPDATA}/npm-cache/**"]
level = "caution"
impact = "Downloaded packages; installs download them again."
impact_vi = "Gói đã tải; lần cài sau tải lại."
verified_on = 2026-10-17
"#;

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, vec![b'x'; bytes]).expect("write");
}

fn age(path: &Path, days: u64) {
    let f = fs::File::options().write(true).open(path).expect("open");
    f.set_modified(SystemTime::now() - Duration::from_secs(days * 86_400))
        .expect("mtime");
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    tree: ScanTree,
    children: ChildIndex,
    ends: Vec<u32>,
    k: Knowledge,
}

impl Fixture {
    fn p(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.root.clone(), |p, c| p.join(c))
    }

    fn id(&self, rel: &str) -> u32 {
        locate(&self.tree, &self.children, &self.p(rel)).unwrap_or_else(|| panic!("{rel}"))
    }

    fn unit(&self, a: &Advice, rel: &str) -> Unit {
        a.unit(self.id(rel))
            .unwrap_or_else(|| panic!("no unit for {rel}"))
            .clone()
    }

    fn advise(&self) -> Advice {
        advise(&self.tree, &self.children, &self.ends, &self.k, now())
    }
}

fn now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_secs(),
    )
    .expect("time")
}

/// A small machine: the temp folder is its system drive.
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    // canonicalize adds `\\?\` on Windows; scan the plain form
    let root = PathBuf::from(root.to_string_lossy().trim_start_matches(r"\\?\"));
    let w = |rel: &str, n: usize| write(&rel.split('/').fold(root.clone(), |p, c| p.join(c)), n);
    w("Windows/System32/kernel.dll", 1000);
    w("Windows/WinSxS/comp.dll", 2000);
    w("Windows/Temp/old.tmp", 3000);
    w("Program Files/App/app.exe", 500);
    w("Users/nano/.claude/projects/proj/chat.jsonl", 4000);
    w("Users/nano/.claude/settings.json", 10);
    w("Users/nano/AppData/Local/App/Cache/a.bin", 5000);
    w("Users/nano/AppData/Local/App/Cache/keep/k.bin", 600);
    w(
        "Users/nano/AppData/Local/App/Cache/.specstory/history/h.md",
        300,
    );
    w("Users/nano/AppData/Local/npm-cache/_cacache/x", 700);
    w("Users/nano/AppData/Local/Temp/fresh.tmp", 800);
    w("Users/nano/AppData/Local/Temp/stale.tmp", 900);
    w("Users/nano/Pictures/a.jpg", 900);
    w("Users/nano/Downloads/setup.exe", 1000);
    w("Work/proj/Cargo.toml", 20);
    w("Work/proj/src/main.rs", 30);
    w("Work/proj/target/debug/x.bin", 10_000);
    w("Work/proj/.git/HEAD", 40);
    w("Work/proj/.aider.chat.history.md", 50);
    w("Work/proj/.claude/worktrees/wt1/.git", 60);
    w("Work/proj/.claude/worktrees/wt1/Cargo.toml", 20);
    w("Work/proj/.claude/worktrees/wt1/src/lib.rs", 100);
    w("Work/proj/.claude/worktrees/wt1/target/debug/y.bin", 20_000);
    w("Work/proj/.claude/settings.local.json", 70);
    w("Work/web/node_modules/pkg/index.js", 80);
    w("Work/web2/package.json", 10);
    w("Work/web2/node_modules/x.js", 90);
    w("Media/Photos/p.jpg", 1100);
    let r = |rel: &str| rel.split('/').fold(root.clone(), |p, c| p.join(c));
    age(&r("Users/nano/Downloads/setup.exe"), 60);
    age(&r("Users/nano/AppData/Local/Temp/stale.tmp"), 10);

    let base = root.to_string_lossy().into_owned();
    let home = format!("{base}/Users/nano");
    let resolve = move |var: &str| -> Option<String> {
        Some(match var {
            "SYSTEMROOT" | "WINDIR" => format!("{base}/Windows"),
            "SYSTEMDRIVE" => base.clone(),
            "HOME" | "USERPROFILE" => home.clone(),
            "APPDATA" => format!("{home}/AppData/Roaming"),
            "LOCALAPPDATA" => format!("{home}/AppData/Local"),
            "TEMP" | "TMP" => format!("{home}/AppData/Local/Temp"),
            "PROGRAMDATA" => format!("{base}/ProgramData"),
            "PROGRAMFILES" => format!("{base}/Program Files"),
            "PROGRAMFILES_X86" => format!("{base}/Program Files (x86)"),
            _ => return None,
        })
    };
    let pack = nfd_rules::parse_pack(PACK, None).expect("pack");
    let k = Knowledge::new(
        &[pack],
        nfd_rules::encyclopedia::builtin(),
        &resolve,
        current_platform(),
    );
    let tree = scanner::scan(&root, &ScanOptions::default(), None).expect("scan");
    let children = ChildIndex::build(&tree.entries);
    let ends = subtree_ends(&tree.entries);
    Fixture {
        _dir: dir,
        root,
        tree,
        children,
        ends,
        k,
    }
}

fn on_disk(f: &Fixture, rel: &str) -> u64 {
    f.tree.entries[f.id(rel) as usize].on_disk
}

#[test]
fn system_paths_are_kept_or_explained() {
    let f = fixture();
    let a = f.advise();
    let win = f.unit(&a, "Windows");
    assert_eq!((win.verdict, win.reason), (Verdict::Keep, Reason::System));
    assert_eq!(
        f.unit(&a, "Windows/System32/kernel.dll").entry,
        f.id("Windows")
    );
    let sxs = f.unit(&a, "Windows/WinSxS");
    assert_eq!(
        (sxs.verdict, sxs.reason),
        (Verdict::Guide, Reason::Explained)
    );
    assert_eq!(sxs.source.as_deref(), Some("winsxs"));
    assert_eq!(sxs.action, Action::Guide);
    // the rule may still clean inside Windows\Temp
    let tmp = f.unit(&a, "Windows/Temp/old.tmp");
    assert_eq!((tmp.verdict, tmp.reason), (Verdict::Safe, Reason::Rule));
    assert_eq!(tmp.source.as_deref(), Some("test.wintemp"));
    let pf = f.unit(&a, "Program Files");
    assert_eq!((pf.verdict, pf.reason), (Verdict::Keep, Reason::Programs));
    assert!(!pf.actionable());
    let t = a.totals(&f.tree, f.id("Windows"));
    assert_eq!(t.safe, on_disk(&f, "Windows/Temp/old.tmp"));
    assert_eq!(t.guide, on_disk(&f, "Windows/WinSxS/comp.dll"));
    assert_eq!(t.keep, on_disk(&f, "Windows/System32/kernel.dll"));
}

#[test]
fn conversation_history_is_never_offered() {
    let f = fixture();
    let a = f.advise();
    let projects = f.unit(&a, "Users/nano/.claude/projects");
    assert_eq!(
        (projects.verdict, projects.reason),
        (Verdict::Keep, Reason::AgentHistory)
    );
    assert_eq!(
        f.unit(&a, "Users/nano/.claude/projects/proj/chat.jsonl")
            .entry,
        f.id("Users/nano/.claude/projects")
    );
    // the folder holding it is not a unit: only what is inside is classified
    assert!(a.unit(f.id("Users/nano/.claude")).is_none());
    let settings = f.unit(&a, "Users/nano/.claude/settings.json");
    assert_eq!(settings.reason, Reason::Protected);
    // Aider's chat log inside a project, SpecStory inside a cache rule's folder
    let aider = f.unit(&a, "Work/proj/.aider.chat.history.md");
    assert_eq!(
        (aider.verdict, aider.reason),
        (Verdict::Keep, Reason::AgentHistory)
    );
    let spec = f.unit(
        &a,
        "Users/nano/AppData/Local/App/Cache/.specstory/history/h.md",
    );
    assert_eq!(
        (spec.verdict, spec.reason),
        (Verdict::Keep, Reason::AgentHistory)
    );
    // no suggestion ever covers it
    let s = a.suggestions(&f.tree);
    for g in s.groups.iter().filter(|g| g.reason == Reason::AgentHistory) {
        assert!(!g.actionable && !g.selected && g.verdict == Verdict::Keep);
    }
    let all: Vec<usize> = (0..a.units.len()).collect();
    let p = a.prepare(&f.tree, &f.ends, &all, true, true);
    let history = f.p("Users/nano/.claude/projects");
    assert!(p.quarantine.iter().all(|i| !i.path.starts_with(&history)));
    assert!(p.rule_roots.iter().all(|r| !r.starts_with(&history)));
    assert!(p.ai_paths.iter().all(|r| !r.starts_with(&history)));
}

#[test]
fn rules_keep_rules_ages_and_stores() {
    let f = fixture();
    let a = f.advise();
    let cache = f.unit(&a, "Users/nano/AppData/Local/App/Cache/a.bin");
    assert_eq!((cache.verdict, cache.reason), (Verdict::Safe, Reason::Rule));
    assert!(cache.selected && cache.actionable());
    assert_eq!(cache.app.as_deref(), Some("App"));
    assert_eq!(cache.impact.vi, "Bộ đệm của ứng dụng.");
    let kept = f.unit(&a, "Users/nano/AppData/Local/App/Cache/keep/k.bin");
    assert_eq!(
        (kept.verdict, kept.reason),
        (Verdict::Keep, Reason::KeepRule)
    );
    let npm = f.unit(&a, "Users/nano/AppData/Local/npm-cache/_cacache");
    assert_eq!((npm.verdict, npm.action), (Verdict::Caution, Action::Store));
    assert_eq!(npm.store.as_deref(), Some("npm-cache-clean"));
    // a file written today stays for now (min_age_days = 1)
    let fresh = f.unit(&a, "Users/nano/AppData/Local/Temp/fresh.tmp");
    assert_eq!(fresh.source.as_deref(), Some("test.temp"));
    let t = a.totals(&f.tree, f.id("Users/nano/AppData/Local/Temp"));
    assert_eq!(
        t.keep,
        on_disk(&f, "Users/nano/AppData/Local/Temp/fresh.tmp")
    );
    assert_eq!(
        t.safe,
        on_disk(&f, "Users/nano/AppData/Local/Temp/stale.tmp")
    );

    // the rule's own walk does the work, inside the picked folders only
    let s = a.suggestions(&f.tree);
    let g = s
        .groups
        .iter()
        .find(|g| g.source.as_deref() == Some("test.cache"))
        .expect("cache group");
    let units = a.actionable_units(&f.tree, std::slice::from_ref(&g.key));
    let p = a.prepare(&f.tree, &f.ends, &units, false, false);
    assert_eq!(p.rule_ids, ["test.cache"]);
    let ids = p.rule_ids.clone();
    let report = f.k.matcher().scan(
        &move |r: &nfd_rules::Rule, _: &str| ids.contains(&r.id),
        &MatchOptions {
            now: now(),
            ..MatchOptions::default()
        },
        &[],
    );
    let items = rule_items(&report, &p.rule_roots);
    let paths: Vec<&Path> = items.iter().map(|i| i.path.as_path()).collect();
    assert!(paths.contains(&f.p("Users/nano/AppData/Local/App/Cache/a.bin").as_path()));
    assert!(
        paths
            .iter()
            .all(|p| !p.ends_with("k.bin") && !p.to_string_lossy().contains(".specstory")),
        "{paths:?}"
    );
}

#[test]
fn build_output_needs_its_project() {
    let f = fixture();
    let a = f.advise();
    let target = f.unit(&a, "Work/proj/target");
    assert_eq!(
        (target.verdict, target.reason, target.action),
        (Verdict::Safe, Reason::BuildOutput, Action::Quarantine)
    );
    assert_eq!(target.source.as_deref(), Some("rust"));
    assert!(target.selected);
    let nm = f.unit(&a, "Work/web2/node_modules");
    assert_eq!(
        (nm.verdict, nm.reason),
        (Verdict::Safe, Reason::BuildOutput)
    );
    // without a package.json only the encyclopedia knows it: 🟡, by hand
    let lone = f.unit(&a, "Work/web/node_modules");
    assert_eq!(
        (lone.verdict, lone.reason, lone.action),
        (Verdict::Caution, Reason::Explained, Action::RecycleBin)
    );
    let git = f.unit(&a, "Work/proj/.git");
    assert_eq!(
        (git.verdict, git.source.as_deref()),
        (Verdict::Keep, Some("git-folder"))
    );
    // the project's own files stay unknown
    assert!(a.unit(f.id("Work/proj/src/main.rs")).is_none());
    let units = a.actionable_units(
        &f.tree,
        &a.suggestions(&f.tree)
            .groups
            .iter()
            .filter(|g| g.reason == Reason::BuildOutput && g.actionable)
            .map(|g| g.key.clone())
            .collect::<Vec<_>>(),
    );
    let p = a.prepare(&f.tree, &f.ends, &units, false, false);
    let q: Vec<&Path> = p.quarantine.iter().map(|i| i.path.as_path()).collect();
    assert!(q.contains(&f.p("Work/proj/target").as_path()), "{q:?}");
    assert!(
        q.contains(&f.p("Work/web2/node_modules").as_path()),
        "{q:?}"
    );
}

#[test]
fn claude_worktrees_wait_for_git_then_follow_it() {
    let f = fixture();
    let a = f.advise();
    let wt = f.unit(&a, "Work/proj/.claude/worktrees/wt1");
    assert_eq!(
        (wt.verdict, wt.reason, wt.blocked.as_deref()),
        (Verdict::Caution, Reason::AgentWorktree, Some("git-check"))
    );
    let target = f.unit(&a, "Work/proj/.claude/worktrees/wt1/target");
    assert_eq!(target.reason, Reason::BuildOutput);
    assert_eq!(target.blocked.as_deref(), Some("git-check"));
    assert_eq!(a.agent_projects, [f.p("Work/proj")]);
    let claude = f.id("Work/proj/.claude");
    // nothing can be freed before git has a say
    assert_eq!(a.reclaim(&f.tree, claude), 0);
    let wt_bytes = a
        .totals(&f.tree, f.id("Work/proj/.claude/worktrees/wt1"))
        .total();

    // finished session, clean and merged: the whole worktree goes with git
    let item = |blocked: Option<&str>| Item {
        path: f.p("Work/proj/.claude/worktrees/wt1"),
        entry: "claude-code.worktree",
        tool: "Claude Code",
        kind: Kind::Worktree,
        scope: Scope::Project,
        project: Some(f.p("Work/proj")),
        rel: ".claude/worktrees/wt1".into(),
        level: RiskLevel::Safe,
        totals: Totals {
            bytes: wt_bytes,
            files: 4,
            newest: 0,
            truncated: false,
        },
        mtime: 0,
        session: None,
        keep: None,
        selected: blocked.is_none(),
        blocked: blocked.map(str::to_owned),
        impact: "impact",
        impact_vi: "tác động",
        worktree: Some(Worktree {
            registered: true,
            removable: blocked.is_none(),
            ..Worktree::default()
        }),
    };
    let report = |blocked| Report {
        items: vec![item(blocked)],
        ..Report::default()
    };
    let mut done = a.clone();
    done.apply_ai(&f.tree, &f.children, &f.ends, &report(None));
    let wt = f.unit(&done, "Work/proj/.claude/worktrees/wt1/target/debug/y.bin");
    assert_eq!(
        (wt.verdict, wt.reason, wt.action),
        (Verdict::Safe, Reason::AgentWorktree, Action::AiLeftovers)
    );
    let t = done.totals(&f.tree, claude);
    assert_eq!(t.safe, wt_bytes);
    assert_eq!(done.reclaim(&f.tree, claude), wt_bytes);
    // the project's own settings next to the worktrees stay unknown
    assert_eq!(
        t.unknown,
        on_disk(&f, "Work/proj/.claude/settings.local.json")
    );
    let g = done
        .suggestions(&f.tree)
        .groups
        .into_iter()
        .find(|g| g.reason == Reason::AgentWorktree)
        .expect("worktree group");
    assert!(g.selected && g.actionable && g.bytes == wt_bytes);
    let p = done.prepare(
        &f.tree,
        &f.ends,
        &done.actionable_units(&f.tree, &[g.key]),
        false,
        false,
    );
    assert_eq!(p.ai_paths, [f.p("Work/proj/.claude/worktrees/wt1")]);
    assert!(p.quarantine.is_empty());

    // a session still running: kept as a whole, its build output too
    let mut live = a.clone();
    live.apply_ai(&f.tree, &f.children, &f.ends, &report(Some("session-live")));
    let t = live.totals(&f.tree, claude);
    assert_eq!(t.keep, wt_bytes);
    assert_eq!(t.reclaimable(), 0);
    assert_eq!(live.reclaim(&f.tree, claude), 0);
    let wt = f.unit(&live, "Work/proj/.claude/worktrees/wt1");
    assert_eq!(wt.blocked.as_deref(), Some("session-live"));
    assert!(
        live.prepare(
            &f.tree,
            &f.ends,
            &(0..live.units.len()).collect::<Vec<_>>(),
            true,
            true
        )
        .ai_paths
        .is_empty()
    );
}

#[test]
fn personal_files_and_old_installers() {
    let f = fixture();
    let a = f.advise();
    let pics = f.unit(&a, "Users/nano/Pictures");
    assert_eq!(
        (pics.verdict, pics.reason),
        (Verdict::Unknown, Reason::Personal)
    );
    let photos = f.unit(&a, "Media/Photos");
    assert_eq!(photos.reason, Reason::Personal);
    let setup = f.unit(&a, "Users/nano/Downloads/setup.exe");
    assert_eq!(
        (setup.verdict, setup.reason, setup.action),
        (Verdict::Caution, Reason::OldInstaller, Action::RecycleBin)
    );
    assert!(!setup.selected);
    // 🟡 needs the user's explicit tick
    let u = a
        .unit_index(f.id("Users/nano/Downloads/setup.exe"))
        .expect("unit");
    assert!(
        a.prepare(&f.tree, &f.ends, &[u], false, false)
            .recycle
            .is_empty()
    );
    assert_eq!(
        a.prepare(&f.tree, &f.ends, &[u], true, false).recycle,
        [f.id("Users/nano/Downloads/setup.exe")]
    );
    // a duplicate search marks the extra copy
    let mut d = a.clone();
    d.apply_duplicates(
        &f.tree,
        &f.children,
        &[(f.p("Media/Photos/p.jpg"), f.p("Users/nano/Pictures/a.jpg"))],
    );
    let dup = f.unit(&d, "Media/Photos/p.jpg");
    assert_eq!(
        (dup.reason, dup.action),
        (Reason::Duplicate, Action::Duplicates)
    );
}

#[test]
fn totals_add_up_and_suggestions_are_ordered() {
    let f = fixture();
    let a = f.advise();
    let root = a.totals(&f.tree, 0);
    assert_eq!(root.total(), f.tree.entries[0].on_disk);
    for e in f
        .tree
        .entries
        .iter()
        .filter(|e| e.kind == nfd_core::model::EntryKind::Dir)
    {
        assert_eq!(a.totals(&f.tree, e.id.0).total(), e.on_disk, "{}", e.name);
    }
    let s = a.suggestions(&f.tree);
    assert_eq!(s.totals, root);
    assert!(s.reclaim > 0 && s.reclaim <= root.reclaimable());
    let order: Vec<Verdict> = s.groups.iter().map(|g| g.verdict).collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);
    for g in &s.groups {
        assert!(g.bytes > 0, "{}", g.key);
        assert!(
            !g.impact.en.is_empty() && !g.impact.vi.is_empty(),
            "{}",
            g.key
        );
        if g.selected {
            assert_eq!(g.verdict, Verdict::Safe, "{}", g.key);
            assert!(g.actionable);
        }
        if !g.verdict.reclaimable() {
            assert!(!g.actionable, "{}", g.key);
        }
    }
    // the details of a folder list its parts, biggest first
    let d = a
        .detail(&f.tree, &f.ends, f.id("Work/proj"))
        .expect("detail");
    assert!(d.unit.is_none());
    let wt = Path::new("Work")
        .join("proj")
        .join(".claude")
        .join("worktrees")
        .join("wt1");
    assert_eq!(d.parts[0].path, wt.join("target").to_string_lossy());
    assert!(d.parts.iter().any(|p| p.path == wt.to_string_lossy()));
    assert!(d.parts.windows(2).all(|w| w[0].bytes >= w[1].bytes));
    let codes = a.codes(&[f.id("Windows"), f.id("Work")]);
    assert_eq!(codes, [Verdict::Keep.code(), 255]);
}

#[test]
fn json_shape_is_stable() {
    let f = fixture();
    let a = f.advise();
    let d = a
        .detail(&f.tree, &f.ends, f.id("Work/proj/target"))
        .expect("detail");
    let v = serde_json::to_value(&d).expect("json");
    assert_eq!(v["unit"]["verdict"], "safe");
    assert_eq!(v["unit"]["reason"], "build-output");
    assert_eq!(v["unit"]["action"], "quarantine");
    assert!(
        v["unit"]["impact"]["vi"]
            .as_str()
            .is_some_and(|t| t.contains("Rust"))
    );
    assert_eq!(v["node"]["root"], true);
    assert!(v["node"]["inside"]["safe"].as_u64().is_some_and(|b| b > 0));
}
