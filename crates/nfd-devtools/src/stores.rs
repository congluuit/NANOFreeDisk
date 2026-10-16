//! Package-store cleanup with the tools' own commands (PLAN §13.2 C4):
//! `npm cache clean --force`, `pnpm store prune`, `pip cache purge`, `go clean -cache`,
//! `dotnet nuget locals all --clear`, … A tool knows which parts of its store are still
//! referenced (pnpm hard links, Conda environments), so its command never breaks a
//! project the way deleting the folder can.
//!
//! Rules of the road (§13.5):
//! * only commands whose result can be undone by **downloading again**
//!   ([`Undo::Redownload`]) may be run, and only after the exact command line —
//!   absolute program path and arguments — has been shown and confirmed;
//! * commands that delete something you cannot get back (`ollama rm`, `docker system
//!   prune`, `git gc`, `hf cache delete`) are [`Undo::None`]: *Guide* mode, shown and
//!   copied, never run by the app;
//! * programs are looked up on `PATH`, run without a shell, with no input, a timeout,
//!   and can be cancelled; the store is measured before and after, and the run is
//!   recorded as an [`Operation`] (kind `command`, not restorable) for the Activity log.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use nfd_core::RiskLevel;
use nfd_core::exec::{ItemResult, ItemStatus, Operation};
use nfd_core::plan::DeleteMode;
use nfd_rules::expand;
use rayon::prelude::*;
use serde::Serialize;

use crate::fsutil::{self, Totals};
use crate::process::{self, RunOutput};

/// How the effect of a command is undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Undo {
    /// The tool downloads (or rebuilds) what it needs again: may be run.
    Redownload,
    /// Gone for good: Guide mode only.
    None,
}

/// One official cleanup command.
#[derive(Debug, Clone, Copy)]
pub struct StoreCommand {
    pub id: &'static str,
    /// Display name of the tool.
    pub tool: &'static str,
    /// Program looked up on `PATH`.
    pub exe: &'static str,
    pub args: &'static [&'static str],
    /// The folders it cleans (rule-pack templates), measured before and after.
    pub store: &'static [&'static str],
    pub undo: Undo,
    pub level: RiskLevel,
}

/// The catalog (DEVTOOL_CACHE_CATALOG §3 "Cách đúng thay thế").
pub const COMMANDS: &[StoreCommand] = &[
    StoreCommand {
        id: "npm-cache-clean",
        tool: "npm",
        exe: "npm",
        args: &["cache", "clean", "--force"],
        store: &["${LOCALAPPDATA}/npm-cache"],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "pnpm-store-prune",
        tool: "pnpm",
        exe: "pnpm",
        args: &["store", "prune"],
        store: &["${LOCALAPPDATA}/pnpm/store"],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "yarn-cache-clean",
        tool: "Yarn",
        exe: "yarn",
        args: &["cache", "clean"],
        store: &["${LOCALAPPDATA}/Yarn/Cache"],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "pip-cache-purge",
        tool: "pip",
        exe: "pip",
        args: &["cache", "purge"],
        store: &["${LOCALAPPDATA}/pip/Cache"],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "uv-cache-clean",
        tool: "uv",
        exe: "uv",
        args: &["cache", "clean"],
        store: &["${LOCALAPPDATA}/uv/cache"],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "conda-clean-all",
        tool: "Conda",
        exe: "conda",
        args: &["clean", "--all", "--yes"],
        store: &[
            "${HOME}/miniconda3/pkgs",
            "${HOME}/anaconda3/pkgs",
            "${HOME}/miniforge3/pkgs",
        ],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "go-clean-cache",
        tool: "Go",
        exe: "go",
        args: &["clean", "-cache"],
        store: &["${LOCALAPPDATA}/go-build"],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "go-clean-modcache",
        tool: "Go",
        exe: "go",
        args: &["clean", "-modcache"],
        store: &["${HOME}/go/pkg/mod"],
        undo: Undo::Redownload,
        level: RiskLevel::Caution,
    },
    StoreCommand {
        id: "nuget-locals-clear",
        tool: "NuGet (.NET)",
        exe: "dotnet",
        args: &["nuget", "locals", "all", "--clear"],
        store: &[
            "${HOME}/.nuget/packages",
            "${LOCALAPPDATA}/NuGet/v3-cache",
            "${LOCALAPPDATA}/NuGet/plugins-cache",
        ],
        undo: Undo::Redownload,
        level: RiskLevel::Caution,
    },
    StoreCommand {
        id: "cargo-cache-autoclean",
        tool: "Cargo (cargo-cache)",
        exe: "cargo-cache",
        args: &["--autoclean"],
        store: &["${HOME}/.cargo/registry", "${HOME}/.cargo/git"],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "composer-clear-cache",
        tool: "Composer",
        exe: "composer",
        args: &["clear-cache"],
        store: &["${LOCALAPPDATA}/Composer"],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "scoop-cache-rm",
        tool: "Scoop",
        exe: "scoop",
        args: &["cache", "rm", "*"],
        store: &["${HOME}/scoop/cache"],
        undo: Undo::Redownload,
        level: RiskLevel::Safe,
    },
    StoreCommand {
        id: "ollama-rm",
        tool: "Ollama",
        exe: "ollama",
        args: &["rm", "<model>"],
        store: &["${HOME}/.ollama/models"],
        undo: Undo::None,
        level: RiskLevel::Caution,
    },
    StoreCommand {
        id: "hf-cache-delete",
        tool: "Hugging Face",
        exe: "hf",
        args: &["cache", "delete"],
        store: &["${HOME}/.cache/huggingface/hub"],
        undo: Undo::None,
        level: RiskLevel::Caution,
    },
    StoreCommand {
        id: "docker-system-prune",
        tool: "Docker",
        exe: "docker",
        args: &["system", "prune"],
        store: &["${LOCALAPPDATA}/Docker/wsl"],
        undo: Undo::None,
        level: RiskLevel::Advanced,
    },
];

/// A command as it applies on this machine.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreInfo {
    pub id: &'static str,
    pub tool: &'static str,
    /// Absolute path of the program (`None` = not on PATH).
    pub program: Option<PathBuf>,
    pub args: Vec<String>,
    /// Exactly what runs: quoted program path and arguments.
    pub command_line: String,
    pub stores: Vec<PathBuf>,
    #[serde(flatten)]
    pub totals: Totals,
    pub undo: Undo,
    pub level: RiskLevel,
    /// Found on PATH and undoable by downloading again.
    pub runnable: bool,
}

fn quote(text: &str) -> String {
    if text.contains([' ', '\t']) {
        format!("\"{text}\"")
    } else {
        text.to_owned()
    }
}

/// Exactly what runs, for the confirmation dialog.
#[must_use]
pub fn command_line(program: &Path, args: &[String]) -> String {
    let mut parts = vec![quote(&program.to_string_lossy())];
    parts.extend(args.iter().map(|a| quote(a)));
    parts.join(" ")
}

/// The commands whose tool is installed or whose store exists, with the store sizes.
/// `which` finds programs ([`process::which`]).
#[must_use]
pub fn list(
    resolve: &(dyn Fn(&str) -> Option<String> + Sync),
    which: &(dyn Fn(&str) -> Option<PathBuf> + Sync),
    max_entries: u64,
) -> Vec<StoreInfo> {
    COMMANDS
        .par_iter()
        .filter_map(|c| {
            let program = which(c.exe);
            let stores: Vec<PathBuf> = c
                .store
                .iter()
                .filter_map(|t| expand(t, resolve))
                .map(|p| {
                    PathBuf::from(if cfg!(windows) {
                        p.replace('/', "\\")
                    } else {
                        p
                    })
                })
                .filter(|p| p.exists())
                .collect();
            if program.is_none() && stores.is_empty() {
                return None;
            }
            let mut totals = Totals::default();
            for s in &stores {
                let t = fsutil::totals(s, max_entries);
                totals.bytes += t.bytes;
                totals.files += t.files;
                totals.newest = totals.newest.max(t.newest);
                totals.truncated |= t.truncated;
            }
            let args: Vec<String> = c.args.iter().map(|a| (*a).to_owned()).collect();
            let shown = program.clone().unwrap_or_else(|| PathBuf::from(c.exe));
            Some(StoreInfo {
                id: c.id,
                tool: c.tool,
                command_line: command_line(&shown, &args),
                runnable: program.is_some() && c.undo == Undo::Redownload,
                program,
                args,
                stores,
                totals,
                undo: c.undo,
                level: c.level,
            })
        })
        .collect()
}

/// Why a command was not run.
#[derive(Debug)]
pub enum RunError {
    /// Guide-only command or program not found.
    NotRunnable,
    /// The command line changed since it was confirmed.
    Changed,
    Io(io::Error),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRunnable => write!(
                f,
                "this command is guide-only or its program is not installed"
            ),
            Self::Changed => write!(f, "the command changed since it was confirmed"),
            Self::Io(e) => write!(f, "cannot start the program: {e}"),
        }
    }
}

impl std::error::Error for RunError {}

/// Outcome of a run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub id: &'static str,
    pub command_line: String,
    pub before: u64,
    pub after: u64,
    pub freed: u64,
    pub output: RunOutput,
}

/// Run `info`'s command if `confirmed` is exactly its command line (what the user saw).
///
/// # Errors
/// Guide-only / missing program, a different command line, or the program did not
/// start.
pub fn run(
    info: &StoreInfo,
    confirmed: &str,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    max_entries: u64,
) -> Result<RunResult, RunError> {
    let Some(program) = info.program.as_deref().filter(|_| info.runnable) else {
        return Err(RunError::NotRunnable);
    };
    if command_line(program, &info.args) != confirmed {
        return Err(RunError::Changed);
    }
    let size = |stores: &[PathBuf]| -> u64 {
        stores
            .iter()
            .map(|s| fsutil::totals(s, max_entries).bytes)
            .sum()
    };
    let before = size(&info.stores);
    let output = process::run(program, &info.args, None, timeout, cancel).map_err(RunError::Io)?;
    let after = size(&info.stores);
    Ok(RunResult {
        id: info.id,
        command_line: confirmed.to_owned(),
        before,
        after,
        freed: before.saturating_sub(after),
        output,
    })
}

/// The run as an Activity-log operation (kind `command`, not restorable).
#[must_use]
pub fn operation(
    info: &StoreInfo,
    result: &RunResult,
    op_id: String,
    started_at: i64,
    finished_at: i64,
) -> Operation {
    let status = if result.output.success() {
        ItemStatus::Done
    } else if result.output.cancelled {
        ItemStatus::Cancelled
    } else {
        ItemStatus::Failed
    };
    let reason = if result.output.timed_out {
        Some("timeout".to_owned())
    } else if result.output.cancelled {
        Some("cancelled".to_owned())
    } else if !result.output.success() {
        Some(format!("exit-{}", result.output.code.unwrap_or(-1)))
    } else {
        None
    };
    let path = info
        .stores
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from(info.tool));
    Operation {
        id: op_id,
        kind: "command".into(),
        started_at,
        finished_at,
        cancelled: result.output.cancelled,
        items: vec![ItemResult {
            n: 0,
            path,
            rule_id: Some(format!("command:{}", info.id)),
            mode: DeleteMode::Permanent,
            is_dir: true,
            bytes: result.before,
            files: 0,
            freed: result.freed,
            status,
            reason,
            message: Some(result.command_line.clone()),
            stored: None,
            warnings: Vec::new(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        for c in COMMANDS {
            assert!(!c.args.is_empty(), "{}", c.id);
            assert!(!c.store.is_empty(), "{}", c.id);
            assert!(
                c.exe
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
            );
            // anything that throws data away for good is guide-only
            if matches!(
                c.id,
                "ollama-rm" | "hf-cache-delete" | "docker-system-prune"
            ) {
                assert_eq!(c.undo, Undo::None);
            }
            for a in c.args {
                assert!(
                    !a.contains(['&', '|', '>', '<', '^']) || a.starts_with('<'),
                    "{a}"
                );
            }
        }
    }

    #[test]
    fn lists_installed_tools_and_refuses_guide_only_or_changed_commands() {
        let dir = tempfile::tempdir().expect("tmp");
        let local = dir.path().join("Local");
        std::fs::create_dir_all(local.join("npm-cache/_cacache")).expect("mkdir");
        std::fs::write(local.join("npm-cache/_cacache/x"), [0u8; 100]).expect("w");
        let l = local.to_string_lossy().replace('\\', "/");
        let h = dir.path().to_string_lossy().replace('\\', "/");
        let resolve = move |v: &str| match v {
            "LOCALAPPDATA" => Some(l.clone()),
            "HOME" => Some(h.clone()),
            _ => None,
        };
        let fake = dir.path().join("bin");
        let which = move |exe: &str| match exe {
            "npm" | "ollama" => Some(fake.join(format!("{exe}.cmd"))),
            _ => None,
        };
        let list = list(&resolve, &which, 1000);
        let npm = list
            .iter()
            .find(|s| s.id == "npm-cache-clean")
            .expect("npm");
        assert!(npm.runnable);
        assert_eq!(npm.totals.files, 1);
        assert!(npm.command_line.ends_with("npm.cmd cache clean --force"));
        let ollama = list.iter().find(|s| s.id == "ollama-rm").expect("ollama");
        assert!(!ollama.runnable, "guide only");
        assert!(
            list.iter().all(|s| s.id != "go-clean-cache"),
            "not installed, no store"
        );
        assert!(matches!(
            run(
                ollama,
                &ollama.command_line,
                Duration::from_secs(1),
                None,
                10
            ),
            Err(RunError::NotRunnable)
        ));
        assert!(matches!(
            run(npm, "npm cache clean", Duration::from_secs(1), None, 10),
            Err(RunError::Changed)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn runs_a_confirmed_command_and_records_it() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = dir.path().join("store");
        std::fs::create_dir_all(&store).expect("mkdir");
        std::fs::write(store.join("blob"), [0u8; 4096]).expect("w");
        // a stand-in "tool" that empties its store
        let tool = dir.path().join("tool.cmd");
        std::fs::write(
            &tool,
            format!(
                "@echo off\r\ndel /q \"{}\\blob\"\r\necho cleaned\r\n",
                store.display()
            ),
        )
        .expect("w");
        let info = StoreInfo {
            id: "npm-cache-clean",
            tool: "npm",
            program: Some(tool.clone()),
            args: Vec::new(),
            command_line: command_line(&tool, &[]),
            stores: vec![store],
            totals: Totals::default(),
            undo: Undo::Redownload,
            level: RiskLevel::Safe,
            runnable: true,
        };
        let r = run(
            &info,
            &info.command_line,
            Duration::from_secs(30),
            None,
            100,
        )
        .expect("run");
        assert!(r.output.success(), "{:?}", r.output);
        assert!(r.output.stdout.contains("cleaned"));
        assert!(r.before > 0);
        assert_eq!(r.after, 0);
        let op = operation(&info, &r, "op".into(), 1, 2);
        assert_eq!(op.kind, "command");
        assert_eq!(op.items[0].mode, DeleteMode::Permanent);
        assert_eq!(op.freed(), r.freed);
    }
}
