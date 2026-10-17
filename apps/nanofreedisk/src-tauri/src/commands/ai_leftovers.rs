//! DevCache → AI leftovers (PLAN §13.2 C10, `docs/AI_LEFTOVERS.md` §2.6): what AI
//! coding agents left behind per session, tied to the session that made it.
//!
//! Cleaning goes the Phase 3 way — folders through `Planner → SafetyGate → Executor →
//! UndoJournal` into the quarantine (kind `ai-leftovers`); worktrees are then removed
//! by git (`git worktree remove`, `git worktree prune`) after a last check, recorded
//! in the Activity log as kind `worktree` (not restorable: the message says how to
//! add the worktree back). Conversation history is never a candidate.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use nfd_core::exec::{self, Operation};
use nfd_core::safety::SafetyGate;
use nfd_devtools::ai_leftovers::{
    self, Options, Report, Sources, sessions::SessionSources, worktrees,
};
use nfd_devtools::process;
use nfd_platform::Platform;
use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use super::clean::{self, OperationDto};
use super::devcache::DevState;
use super::{AppState, CmdResult, err};

/// Entries visited per folder when sizing.
const MAX_ENTRIES: u64 = 2_000_000;

/// The last AI-leftovers scan.
#[derive(Debug, Default)]
pub struct AiState {
    report: Mutex<Option<Report>>,
}

impl AiState {
    /// Bytes the last scan pre-selected (finished sessions, 🟢) — Health Score.
    pub fn selected_bytes(&self) -> Option<u64> {
        let guard = self.report.lock().ok()?;
        guard.as_ref().map(|r| r.selected_bytes)
    }

    /// The last scan (Analyze's advice reuses it, and refines agent worktrees with it).
    pub(super) fn latest(&self) -> Option<Report> {
        self.report.lock().ok()?.clone()
    }

    /// Keep a scan made elsewhere (the advice's own check) for the cleaning commands.
    pub(super) fn set(&self, report: Report) {
        if let Ok(mut slot) = self.report.lock() {
            *slot = Some(report);
        }
    }

    /// Forget the last scan (after a clean: it is stale).
    pub(super) fn clear(&self) {
        if let Ok(mut slot) = self.report.lock() {
            *slot = None;
        }
    }
}

/// Remove `jobs` with git (`git worktree remove`, `prune`) on a worker thread and
/// journal them as kind `worktree`. Returns the item results, a journal problem and
/// whether `cancel` stopped it.
pub(super) async fn remove_worktrees(
    app: &AppHandle,
    state: &State<'_, AppState>,
    jobs: Vec<worktrees::WorktreeJob>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> CmdResult<(Vec<exec::ItemResult>, Option<String>, bool)> {
    let op = tauri::async_runtime::spawn_blocking(move || {
        let started = exec::unix_now();
        let git = process::git();
        let results = jobs
            .iter()
            .enumerate()
            .filter_map(|(n, j)| {
                git.as_deref()
                    .map(|g| worktrees::remove(g, j, n, Some(&cancel)))
            })
            .collect();
        worktrees::operation(results, exec::new_op_id(started), started, exec::unix_now())
    })
    .await
    .map_err(|e| err("internal", e))?;
    let journal_error = if op.items.is_empty() {
        None
    } else {
        clean::journal(app, state, &op)
    };
    Ok((op.items, journal_error, op.cancelled))
}

/// Where the sessions and leftovers are read from: this machine, or (GUI smoke
/// test) a fixture folder laid out like `home/.claude/projects`, `desk`,
/// `temp/claude`, `ms-playwright`.
fn sources(fixture: Option<&Path>) -> Sources {
    match fixture {
        None => {
            let resolve = nfd_platform::Native.resolver();
            let env = |k: &str| std::env::var(k).ok();
            Sources::detect(&resolve, &env)
        }
        Some(f) => Sources {
            sessions: SessionSources {
                claude_projects: vec![f.join("home").join(".claude").join("projects")],
                desktop: vec![f.join("desk")],
                codex: Vec::new(),
            },
            temp_claude: vec![f.join("temp").join("claude")],
            playwright: vec![f.join("ms-playwright")],
            not_projects: vec![f.join("temp")],
        },
    }
}

/// Find the leftovers. `roots` are extra folders searched for repositories (the
/// Repo Finder root); `fixture` is for the GUI smoke test only.
#[tauri::command]
pub async fn ai_leftovers_scan(
    dev: State<'_, DevState>,
    ai: State<'_, AiState>,
    roots: Vec<String>,
    keep_recent_days: u32,
    fixture: Option<String>,
) -> CmdResult<Report> {
    let cancel = dev.start()?;
    let fixture = fixture.filter(|f| !f.trim().is_empty()).map(PathBuf::from);
    let report = tauri::async_runtime::spawn_blocking(move || {
        let opts = Options {
            keep_recent_days,
            max_entries: MAX_ENTRIES,
            roots: roots
                .into_iter()
                .filter(|r| !r.trim().is_empty())
                .map(PathBuf::from)
                .filter(|r| r.is_dir())
                .collect(),
            ..Options::default()
        };
        let gate = SafetyGate::new(&nfd_platform::Native.resolver());
        let git = process::git();
        ai_leftovers::scan(
            &opts,
            &sources(fixture.as_deref()),
            git.as_deref(),
            &gate,
            Some(&cancel),
        )
    })
    .await
    .map_err(|e| err("internal", e))?;
    dev.stop();
    dev.show(report.items.iter().flat_map(|i| {
        std::iter::once(i.path.as_path()).chain(
            i.worktree
                .iter()
                .flat_map(|w| w.artifacts.iter().map(|a| a.path.as_path())),
        )
    }));
    dev.show(report.kept.iter().map(|k| k.path.as_path()));
    *ai.report.lock().map_err(|e| err("internal", e))? = Some(report.clone());
    Ok(report)
}

/// What a clean of `paths` runs: git command lines (shown in the confirmation).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPlanDto {
    folders: usize,
    bytes: u64,
    commands: Vec<String>,
}

#[tauri::command]
pub fn ai_leftovers_plan(ai: State<'_, AiState>, paths: Vec<String>) -> CmdResult<AiPlanDto> {
    let guard = ai.report.lock().map_err(|e| err("internal", e))?;
    let report = guard
        .as_ref()
        .ok_or_else(|| err("no-ai-scan", "no AI leftovers scan yet"))?;
    let picked: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let p = ai_leftovers::prepare(report, &picked);
    Ok(AiPlanDto {
        folders: p.plan.items.len(),
        bytes: p.plan.selected_bytes() + p.jobs.iter().map(|j| j.bytes).sum::<u64>(),
        commands: p
            .jobs
            .iter()
            .map(worktrees::WorktreeJob::command_line)
            .collect(),
    })
}

/// Clean the chosen leftovers of the last scan: folders to the quarantine, then the
/// worktrees with git. Progress streams as `clean-progress`.
#[tauri::command]
pub async fn ai_leftovers_clean(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    dev: State<'_, DevState>,
    ai: State<'_, AiState>,
    paths: Vec<String>,
) -> CmdResult<OperationDto> {
    let prepared = {
        let guard = ai.report.lock().map_err(|e| err("internal", e))?;
        let report = guard
            .as_ref()
            .ok_or_else(|| err("no-ai-scan", "no AI leftovers scan yet"))?;
        let picked: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        ai_leftovers::prepare(report, &picked)
    };
    if prepared.plan.items.is_empty() && prepared.jobs.is_empty() {
        return Err(err("nothing-to-clean", "nothing to clean"));
    }
    let cancel = dev.start()?;
    let mut items = Vec::new();
    let mut journal_error = None;
    let mut cancelled = false;
    if !prepared.plan.items.is_empty() {
        let result = clean::execute_plan_returning(
            &app,
            &window,
            &state,
            prepared.plan,
            "ai-leftovers",
            cancel.clone(),
            Box::new(|_| Ok(())),
        )
        .await;
        match result {
            Ok((dto, op)) => {
                journal_error = dto.journal_error;
                cancelled |= op.cancelled;
                items.extend(op.items);
            }
            Err(e) => {
                dev.stop();
                return Err(e);
            }
        }
    }
    if !prepared.jobs.is_empty() && !cancelled {
        let (done, problem, stopped) =
            remove_worktrees(&app, &state, prepared.jobs, cancel.clone()).await?;
        journal_error = journal_error.or(problem);
        cancelled |= stopped;
        items.extend(done);
    }
    dev.stop();
    *ai.report.lock().map_err(|e| err("internal", e))? = None;
    // one result for the view: both operations' items
    let all = Operation {
        id: String::new(),
        kind: "ai-leftovers".into(),
        started_at: 0,
        finished_at: 0,
        items,
        cancelled,
    };
    Ok(clean::op_dto(&all, journal_error))
}
