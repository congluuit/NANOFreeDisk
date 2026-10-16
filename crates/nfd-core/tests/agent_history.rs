//! PLAN §5.1: the conversation / session history of AI agents (Claude Code, Codex,
//! Gemini CLI, Antigravity, Cursor, Copilot, Windsurf, Devin, ZCode, Trae…) is never
//! deleted — not by a rule of any level, not by DevCache or Duplicates, not by hand.

use std::fs;
use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::exec::{self, ExecOptions, ItemStatus, QuarantineRoots};
use nfd_core::plan::{CleanPlan, DeleteMode, PlanItem};
use nfd_core::safety::{
    AGENT_HISTORY_NAMES, AGENT_HISTORY_TEMPLATES, Blocked, LiveProbe, Origin, SafetyGate,
    is_protected_template,
};

fn resolver(var: &str) -> Option<String> {
    let base = if cfg!(windows) { "C:" } else { "" };
    Some(match var {
        "SYSTEMROOT" => format!("{base}/Windows"),
        "SYSTEMDRIVE" => base.to_owned(),
        "HOME" | "USERPROFILE" => format!("{base}/Users/nano"),
        "APPDATA" => format!("{base}/Users/nano/AppData/Roaming"),
        "LOCALAPPDATA" => format!("{base}/Users/nano/AppData/Local"),
        "TEMP" => format!("{base}/Users/nano/AppData/Local/Temp"),
        "PROGRAMDATA" => format!("{base}/ProgramData"),
        "PROGRAMFILES" => format!("{base}/Program Files"),
        "PROGRAMFILES_X86" => format!("{base}/Program Files (x86)"),
        _ => return None,
    })
}

/// Every origin a deletion can come from.
const ORIGINS: [Origin; 4] = [
    Origin::Manual,
    Origin::Rule {
        level: RiskLevel::Safe,
    },
    Origin::Rule {
        level: RiskLevel::Caution,
    },
    Origin::Rule {
        level: RiskLevel::Advanced,
    },
];

/// A template expanded with the test resolver, wildcards filled in.
fn expand(template: &str) -> PathBuf {
    let rest = template.strip_prefix("${").expect("template");
    let (var, tail) = rest.split_once('}').expect("template");
    let base = resolver(var).expect("known variable");
    PathBuf::from(format!(
        "{base}{}",
        tail.replace('*', "1").replace('?', "x")
    ))
}

#[test]
fn wildcards_only_in_the_last_component() {
    for t in AGENT_HISTORY_TEMPLATES {
        let (parent, _) = t.rsplit_once('/').expect("has a parent");
        assert!(
            !parent.contains(['*', '?']),
            "{t}: a wildcard before the last component makes parent checks inexact"
        );
    }
}

#[test]
fn every_location_and_everything_inside_is_refused_for_every_origin() {
    let gate = SafetyGate::new(&resolver);
    for t in AGENT_HISTORY_TEMPLATES {
        let at = expand(t);
        for path in [
            at.clone(),
            at.join("session.jsonl"),
            at.join("a").join("b.db"),
        ] {
            for origin in ORIGINS {
                assert!(
                    matches!(
                        gate.check_path(&path, origin),
                        Err(Blocked::Protected { .. })
                    ),
                    "{} ({origin:?}) from {t} must be protected, got {:?}",
                    path.display(),
                    gate.check_path(&path, origin)
                );
            }
        }
        // the folder holding it can never go as a whole either
        let parent = at.parent().expect("parent");
        assert!(
            gate.check_path(parent, Origin::Manual).is_err(),
            "{} holds {t}",
            parent.display()
        );
    }
}

#[test]
fn template_check_flags_rules_that_reach_into_history() {
    for rule_path in [
        "${HOME}/.claude/projects/*/*.jsonl",
        "${HOME}/.codex/sessions/**",
        "${HOME}/.codex/thread_history_1.sqlite",
        "${HOME}/.gemini/tmp/**",
        "${HOME}/.gemini/antigravity-cli/conversations/**",
        "${APPDATA}/Code/User/workspaceStorage/**",
        "${APPDATA}/Cursor/User/globalStorage/**",
        "${APPDATA}/Devin/cli/sessions.db-wal",
        "${HOME}/.zcode/cli/rollout/**",
        "${HOME}/proj/.specstory/**",
    ] {
        assert!(is_protected_template(rule_path), "{rule_path}");
    }
    for rule_path in [
        "${HOME}/.claude/debug/**",
        "${HOME}/.codex/log/**",
        "${HOME}/.codex/*.log",
        "${APPDATA}/Code/User/History/**",
        "${APPDATA}/Cursor/Cache/**",
        "${HOME}/.gemini/skills/**",
    ] {
        assert!(!is_protected_template(rule_path), "{rule_path}");
    }
}

#[test]
fn neighbours_stay_cleanable() {
    let gate = SafetyGate::new(&resolver);
    let safe = Origin::Rule {
        level: RiskLevel::Safe,
    };
    for path in [
        "/Users/nano/.claude/debug/x.txt",
        "/Users/nano/.claude/shell-snapshots/s.sh",
        "/Users/nano/.codex/log/codex-tui.log",
        "/Users/nano/.codex/thread_history",
        "/Users/nano/AppData/Roaming/Code/User/History/a",
        "/Users/nano/AppData/Roaming/Cursor/Cache/x",
        "/Users/nano/AppData/Roaming/Claude/Cache/y",
    ] {
        let path = if cfg!(windows) {
            PathBuf::from(format!("C:{path}"))
        } else {
            PathBuf::from(path)
        };
        assert_eq!(gate.check_path(&path, safe), Ok(()), "{}", path.display());
    }
}

#[test]
fn chat_logs_inside_projects_are_refused_by_name() {
    let gate = SafetyGate::new(&resolver);
    let base = if cfg!(windows) {
        "D:/dev/app"
    } else {
        "/dev/app"
    };
    for name in AGENT_HISTORY_NAMES {
        for path in [
            format!("{base}/{name}"),
            format!("{base}/{}/inner.md", name.to_uppercase()),
        ] {
            for origin in ORIGINS {
                assert!(
                    matches!(
                        gate.check_path(Path::new(&path), origin),
                        Err(Blocked::Protected { .. })
                    ),
                    "{path} ({origin:?})"
                );
            }
        }
    }
}

#[test]
fn the_executor_never_moves_a_transcript() {
    let dir = tempfile::tempdir().expect("tmp");
    let home = dir.path().join("home");
    let transcript = home.join(".claude/projects/-d-app/0001.jsonl");
    let rollout = home.join(".codex/sessions/2026/10/16/rollout.jsonl");
    let chat = home.join(".gemini/tmp/0a1b/chats/session.json");
    for f in [&transcript, &rollout, &chat] {
        fs::create_dir_all(f.parent().expect("parent")).expect("mkdir");
        fs::write(f, b"{\"type\":\"user\"}").expect("write");
    }
    let h = home.to_string_lossy().replace('\\', "/");
    let resolve = move |v: &str| (v == "HOME").then(|| h.clone());
    let gate = SafetyGate::new(&resolve);
    let item = |path: &Path, origin: Origin, mode: DeleteMode| {
        let info = nfd_platform::fsinfo::file_info(path).expect("info");
        PlanItem {
            path: path.to_path_buf(),
            origin,
            rule_id: None,
            is_dir: info.is_dir,
            bytes: info.on_disk,
            files: 1,
            size: info.logical,
            mtime: info.mtime,
            requires_closed: Vec::new(),
            mode,
            selected: true,
        }
    };
    let plan = CleanPlan {
        items: vec![
            item(&transcript, Origin::Manual, DeleteMode::RecycleBin),
            item(
                &rollout,
                Origin::Rule {
                    level: RiskLevel::Safe,
                },
                DeleteMode::Permanent,
            ),
            item(
                &home.join(".gemini/tmp/0a1b"),
                Origin::Manual,
                DeleteMode::Quarantine,
            ),
        ],
    };
    let opts = ExecOptions {
        op_id: "20261016-000000-beef".into(),
        kind: "manual".into(),
        owner: 0,
        roots: QuarantineRoots {
            system: Some(dir.path().join("q")),
            other_volumes: false,
        },
        now: exec::unix_now(),
    };
    let op = exec::execute(&plan, &gate, &LiveProbe, &opts, &mut |_, _, _| true);
    for r in &op.items {
        assert_eq!(r.status, ItemStatus::Skipped, "{r:#?}");
        assert_eq!(r.reason.as_deref(), Some("protected"), "{r:#?}");
    }
    assert!(transcript.exists() && rollout.exists() && chat.exists());
}
