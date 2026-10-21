//! Clean view, Activity log and every delete in the app (PHASE3_CLEAN §2.5).
//!
//! All deletions — rule matches from Clean, items picked in Analyze / Old & Large,
//! empty folders and broken shortcuts from Tidy up — go the same way:
//! `Planner → SafetyGate → Executor → UndoJournal`. Each operation lands in the
//! journal (history database) and the audit log, and can be undone from the
//! Activity log while its quarantine or Recycle Bin copy exists.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nfd_core::RiskLevel;
use nfd_core::exec::{self, ExecOptions, ItemStatus, Operation, QuarantineRoots};
use nfd_core::plan::{self, CleanPlan, DeleteMode, PlanOptions};
use nfd_core::safety::{LiveProbe, SafetyGate};
use nfd_history::{ItemRow, OperationRow, PurgeResult, QuarantineSummary, RestoreResult};
use nfd_platform::Platform;
use nfd_rules::matcher::{MatchOptions, MatchReport, Matcher, RuleSummary};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use super::{AppState, CmdResult, err, owner_handle, with_history, with_session};

/// The last rule scan and the running clean, if any.
#[derive(Debug, Default)]
pub struct CleanState {
    report: Mutex<Option<Cached>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Debug, Clone)]
struct Cached {
    report: MatchReport,
    at: i64,
}

impl CleanState {
    /// Look at the last rule scan, if any (Analyze's "junk elsewhere").
    pub(super) fn with_report<T>(&self, f: impl FnOnce(&MatchReport) -> T) -> Option<T> {
        let guard = self.report.lock().ok()?;
        guard.as_ref().map(|c| f(&c.report))
    }

    /// 🟢 Safe bytes found by the last rule scan (for the Health Score).
    pub fn safe_bytes(&self) -> Option<u64> {
        let guard = self.report.lock().ok()?;
        guard.as_ref().map(|c| {
            c.report
                .candidates
                .iter()
                .filter(|x| x.level == RiskLevel::Safe)
                .map(|x| x.bytes)
                .sum()
        })
    }
}

pub(super) fn app_local(app: &AppHandle) -> Option<PathBuf> {
    crate::paths::local_data(app)
}

pub(super) fn roots(app: &AppHandle) -> QuarantineRoots {
    QuarantineRoots::new(app_local(app).as_deref())
}

pub(super) fn audit_dir(app: &AppHandle) -> Option<PathBuf> {
    app_local(app).map(|d| d.join("logs"))
}

fn gate() -> SafetyGate {
    SafetyGate::new(&nfd_platform::Native.resolver())
}

/// Store a finished operation in the journal and the audit log. Journal problems are
/// returned as text (the operation itself already happened).
pub(super) fn journal(app: &AppHandle, state: &AppState, op: &Operation) -> Option<String> {
    if let Some(dir) = audit_dir(app) {
        let _ = nfd_history::audit::append(&dir, op);
    }
    with_history(app, state, |h| h.record_operation(op))
        .err()
        .map(|e| e.message)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackDto {
    id: String,
    name: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanScanDto {
    at: i64,
    rules: Vec<RuleSummary>,
    packs: Vec<PackDto>,
    safe: u64,
    caution: u64,
    advanced: u64,
}

fn scan_dto(c: &Cached, packs: &[nfd_rules::Pack]) -> CleanScanDto {
    let by_level = c.report.bytes_by_level();
    let by_pack = c.report.bytes_by_pack();
    CleanScanDto {
        at: c.at,
        rules: c.report.rules.clone(),
        packs: packs
            .iter()
            .map(|p| PackDto {
                id: p.pack.id.clone(),
                name: p.pack.name.clone(),
                bytes: by_pack.get(&p.pack.id).copied().unwrap_or(0),
            })
            .collect(),
        safe: by_level.get(&RiskLevel::Safe).copied().unwrap_or(0),
        caution: by_level.get(&RiskLevel::Caution).copied().unwrap_or(0),
        advanced: by_level.get(&RiskLevel::Advanced).copied().unwrap_or(0),
    }
}

/// The built-in packs, or the packs of `rules_dir` (tests), which must validate.
pub(super) fn load_packs(rules_dir: Option<String>) -> CmdResult<Vec<nfd_rules::Pack>> {
    match rules_dir {
        Some(dir) => {
            let packs = nfd_rules::load_dir(Path::new(&dir)).map_err(|e| err("rules", e))?;
            if nfd_rules::has_errors(&nfd_rules::validate(&packs)) {
                return Err(err("rules", "the rule packs do not validate"));
            }
            Ok(packs)
        }
        None => nfd_rules::builtin_packs().map_err(|e| err("rules", e)),
    }
}

/// Match the rule packs on this machine (read-only). `rules_dir` replaces the
/// built-in packs (tests); such packs must validate. Also purges quarantines older
/// than 7 days.
#[tauri::command]
pub async fn clean_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    clean: State<'_, CleanState>,
    rules_dir: Option<String>,
) -> CmdResult<CleanScanDto> {
    let _ = with_history(&app, &state, |h| {
        h.purge_expired(&roots(&app), exec::unix_now(), None)
    });
    let packs = load_packs(rules_dir)?;
    let packs2 = packs.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let resolve = nfd_platform::Native.resolver();
        let matcher = Matcher::new(&packs2, &resolve, std::env::consts::OS);
        let running = nfd_platform::locks::running_processes().unwrap_or_default();
        matcher.scan(&|_, _| true, &MatchOptions::default(), &running)
    })
    .await
    .map_err(|e| err("internal", e))?;
    let cached = Cached {
        report,
        at: exec::unix_now(),
    };
    let dto = scan_dto(&cached, &packs);
    *clean.report.lock().map_err(|e| err("internal", e))? = Some(cached);
    Ok(dto)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDto {
    path: String,
    is_dir: bool,
    bytes: u64,
    files: u64,
    newest: i64,
}

/// The `limit` largest candidates of one rule (preview).
pub(super) fn largest(report: &MatchReport, rule_id: &str, limit: usize) -> Vec<CandidateDto> {
    let mut items: Vec<CandidateDto> = report
        .candidates
        .iter()
        .filter(|c| c.rule_id == rule_id)
        .map(|c| CandidateDto {
            path: c.path.to_string_lossy().into_owned(),
            is_dir: c.is_dir,
            bytes: c.bytes,
            files: c.files,
            newest: c.newest_mtime,
        })
        .collect();
    items.sort_by_key(|c| std::cmp::Reverse(c.bytes));
    items.truncate(limit);
    items
}

/// The largest items one rule would clean (preview).
#[tauri::command]
pub fn clean_items(
    clean: State<'_, CleanState>,
    rule_id: String,
    limit: usize,
) -> CmdResult<Vec<CandidateDto>> {
    let guard = clean.report.lock().map_err(|e| err("internal", e))?;
    let cached = guard
        .as_ref()
        .ok_or_else(|| err("no-clean-scan", "no rule scan yet"))?;
    Ok(largest(&cached.report, &rule_id, limit))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanProgress {
    done: usize,
    total: usize,
    freed: u64,
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDto {
    path: String,
    status: &'static str,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationDto {
    id: String,
    freed: u64,
    done: usize,
    partial: usize,
    skipped: usize,
    failed: usize,
    cancelled: bool,
    /// First problems (skipped, partial, failed), for the result message.
    problems: Vec<ProblemDto>,
    pub(super) journal_error: Option<String>,
}

pub(super) fn op_dto(op: &Operation, journal_error: Option<String>) -> OperationDto {
    OperationDto {
        id: op.id.clone(),
        freed: op.freed(),
        done: op.count(ItemStatus::Done),
        partial: op.count(ItemStatus::Partial),
        skipped: op.count(ItemStatus::Skipped),
        failed: op.count(ItemStatus::Failed),
        cancelled: op.cancelled,
        problems: op
            .items
            .iter()
            .filter(|i| {
                matches!(
                    i.status,
                    ItemStatus::Skipped | ItemStatus::Partial | ItemStatus::Failed
                )
            })
            .take(50)
            .map(|i| ProblemDto {
                path: i.path.to_string_lossy().into_owned(),
                status: i.status.as_str(),
                reason: i.reason.clone(),
            })
            .collect(),
        journal_error,
    }
}

/// A plan for every candidate of the ticked `rule_ids`: 🟠 Advanced rules only in
/// Expert mode, `permanent` (Expert, 🟢 Safe only) skips the quarantine.
pub(super) fn rules_plan(
    report: &MatchReport,
    rule_ids: &[String],
    expert: bool,
    permanent: bool,
) -> CleanPlan {
    let items = report
        .plan_items()
        .into_iter()
        .filter(|i| i.rule_id.as_ref().is_some_and(|r| rule_ids.contains(r)))
        .collect();
    let mut plan = plan::build(
        items,
        PlanOptions {
            expert,
            include_caution: true,
            permanent: permanent && expert,
        },
    );
    // the user ticked these rules: everything they cover may go, Advanced in
    // Expert mode only (the planner already dropped Protected)
    for item in &mut plan.items {
        item.selected = item.level() != RiskLevel::Advanced || expert;
    }
    plan
}

/// Run `plan` through the SafetyGate and the executor on a worker thread, streaming
/// `clean-progress`; `cancel` stops between items. The operation is journaled as
/// `kind`.
pub(super) async fn execute_plan(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    plan: CleanPlan,
    kind: &str,
    cancel: Arc<AtomicBool>,
) -> CmdResult<OperationDto> {
    execute_plan_returning(app, window, state, plan, kind, cancel, Box::new(|_| Ok(())))
        .await
        .map(|(dto, _)| dto)
}

/// A last feature-specific check per item, run on the worker thread right before
/// the item is touched (Duplicates: keeper and copy hashed again).
pub(super) type Verify = Box<dyn FnMut(&plan::PlanItem) -> Result<(), exec::Refusal> + Send>;

/// [`execute_plan`] with a [`Verify`] check, also returning the operation itself.
pub(super) async fn execute_plan_returning(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    plan: CleanPlan,
    kind: &str,
    cancel: Arc<AtomicBool>,
    mut verify: Verify,
) -> CmdResult<(OperationDto, Operation)> {
    if plan.selected().next().is_none() {
        return Err(err("nothing-to-clean", "the rules have nothing to clean"));
    }
    let opts = ExecOptions {
        op_id: exec::new_op_id(exec::unix_now()),
        kind: kind.into(),
        owner: owner_handle(window),
        roots: roots(app),
        now: exec::unix_now(),
    };
    let emitter = app.clone();
    let op = tauri::async_runtime::spawn_blocking(move || {
        let mut freed = 0;
        exec::execute_verified(
            &plan,
            &gate(),
            &LiveProbe,
            &opts,
            &mut verify,
            &mut |done, total, r| {
                freed += r.freed;
                let _ = emitter.emit(
                    "clean-progress",
                    CleanProgress {
                        done,
                        total,
                        freed,
                        path: r.path.to_string_lossy().into_owned(),
                    },
                );
                !cancel.load(Ordering::Relaxed)
            },
        )
    })
    .await
    .map_err(|e| err("internal", e))?;
    let journal_error = journal(app, state, &op);
    Ok((op_dto(&op, journal_error), op))
}

/// Clean every candidate of `rule_ids` from the last rule scan. 🟠 Advanced rules
/// need `expert`; `permanent` (Expert, 🟢 Safe only) skips the quarantine. Progress
/// streams as `clean-progress`; [`clean_cancel`] stops between items.
#[tauri::command]
pub async fn clean_run(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    clean: State<'_, CleanState>,
    rule_ids: Vec<String>,
    expert: bool,
    permanent: bool,
) -> CmdResult<OperationDto> {
    let plan = {
        let guard = clean.report.lock().map_err(|e| err("internal", e))?;
        let cached = guard
            .as_ref()
            .ok_or_else(|| err("no-clean-scan", "no rule scan yet"))?;
        rules_plan(&cached.report, &rule_ids, expert, permanent)
    };
    let cancel = Arc::new(AtomicBool::new(false));
    *clean.cancel.lock().map_err(|e| err("internal", e))? = Some(cancel.clone());
    let result = execute_plan(&app, &window, &state, plan, "clean", cancel).await;
    *clean.cancel.lock().map_err(|e| err("internal", e))? = None;
    // the candidates are stale now: the next visit rescans
    *clean.report.lock().map_err(|e| err("internal", e))? = None;
    result
}

/// Stop a running clean after the current item.
#[tauri::command]
pub fn clean_cancel(clean: State<'_, CleanState>) -> CmdResult<()> {
    if let Some(c) = clean
        .cancel
        .lock()
        .map_err(|e| err("internal", e))?
        .as_ref()
    {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Result of one hand-picked entry (Analyze, Old & Large, Tidy up).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteOutcome {
    pub id: u32,
    /// `done`, `partial`, `declined`, `blocked` or `failed`.
    pub status: &'static str,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub on_disk: u64,
}

/// Delete entries of the loaded scan: `mode` = `recycle-bin` (default) or
/// `quarantine`; `kind` labels the operation in the Activity log (`manual`, `tidy`).
pub async fn delete_entries_impl(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    ids: &[u32],
    mode: DeleteMode,
    kind: &str,
) -> CmdResult<Vec<DeleteOutcome>> {
    let resolve = nfd_platform::Native.resolver();
    let delete_plan = with_session(state, |s| s.plan_delete(ids, &resolve))?;
    let mut outcomes: Vec<DeleteOutcome> = Vec::new();
    let mut plan = CleanPlan::default();
    let mut planned_ids = Vec::new();
    for item in &delete_plan.items {
        if !item.allowed {
            outcomes.push(DeleteOutcome {
                id: item.id,
                status: "blocked",
                reason: item.reason.map(str::to_owned),
                message: None,
                on_disk: item.on_disk,
            });
            continue;
        }
        if let Some(p) = with_session(state, |s| s.plan_item(item.id, mode))? {
            plan.items.push(p);
            planned_ids.push((item.id, item.on_disk));
        }
    }
    if plan.items.is_empty() {
        return Ok(outcomes);
    }
    let opts = ExecOptions {
        op_id: exec::new_op_id(exec::unix_now()),
        kind: kind.to_owned(),
        owner: owner_handle(window),
        roots: roots(app),
        now: exec::unix_now(),
    };
    // the shell's Recycle Bin prompt is modal on the calling thread: run here
    let op = exec::execute(&plan, &gate(), &LiveProbe, &opts, &mut |_, _, _| true);
    for (r, (id, on_disk)) in op.items.iter().zip(&planned_ids) {
        let status = match r.status {
            ItemStatus::Done => "done",
            ItemStatus::Partial => "partial",
            ItemStatus::Declined => "declined",
            ItemStatus::Skipped => "blocked",
            _ => "failed",
        };
        if r.status == ItemStatus::Done {
            let mut guard = state.session.write().map_err(|e| err("internal", e))?;
            if let Some(s) = guard.as_mut() {
                s.mark_removed(*id);
            }
        }
        outcomes.push(DeleteOutcome {
            id: *id,
            status,
            reason: r.reason.clone(),
            message: r.message.clone(),
            on_disk: *on_disk,
        });
    }
    let _ = journal(app, state, &op);
    // the advice's totals changed with the scan
    super::advice::refresh(app);
    Ok(outcomes)
}

/// Delete entries of the loaded scan through the Planner / SafetyGate / Executor.
#[tauri::command]
pub async fn delete_entries(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    ids: Vec<u32>,
    mode: Option<String>,
    kind: Option<String>,
) -> CmdResult<Vec<DeleteOutcome>> {
    let mode = match mode.as_deref() {
        None | Some("recycle-bin") => DeleteMode::RecycleBin,
        Some("quarantine") => DeleteMode::Quarantine,
        Some(other) => return Err(err("bad-mode", other)),
    };
    let kind = kind.unwrap_or_else(|| "manual".into());
    if !matches!(kind.as_str(), "manual" | "tidy") {
        return Err(err("bad-kind", kind));
    }
    delete_entries_impl(&app, &window, &state, &ids, mode, &kind).await
}

/// Operations, newest first (also purges expired quarantines).
#[tauri::command]
pub async fn activity_list(
    app: AppHandle,
    state: State<'_, AppState>,
    limit: usize,
) -> CmdResult<Vec<OperationRow>> {
    let r = roots(&app);
    with_history(&app, &state, |h| {
        let now = exec::unix_now();
        let _ = h.purge_expired(&r, now, None);
        let _ = h.prune_operations(now, nfd_history::audit::KEEP_DAYS);
        h.operations(limit)
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDetail {
    operation: OperationRow,
    items: Vec<ItemRow>,
}

#[tauri::command]
pub async fn activity_items(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<ActivityDetail> {
    let (operation, items) = with_history(&app, &state, |h| h.operation(&id))?;
    Ok(ActivityDetail { operation, items })
}

/// Put an operation's items back (all, or only `items`).
#[tauri::command]
pub async fn activity_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    items: Option<Vec<u64>>,
) -> CmdResult<Vec<RestoreResult>> {
    let out = with_history(&app, &state, |h| h.restore_operation(&id, items.as_deref()))?;
    if let Some(dir) = audit_dir(&app) {
        let _ = nfd_history::audit::note(
            &dir,
            exec::unix_now(),
            &format!("{id}\trestore\t{} item(s)", out.len()),
        );
    }
    Ok(out)
}

/// Delete what the quarantine holds of operation `id` for good: everything, or
/// only `items` (item numbers). Recycle Bin items are left to Windows.
#[tauri::command]
pub async fn activity_purge(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    items: Option<Vec<u64>>,
) -> CmdResult<PurgeResult> {
    let out = with_history(&app, &state, |h| h.purge_items(&id, items.as_deref()))?;
    if let Some(dir) = audit_dir(&app) {
        let _ = nfd_history::audit::note(
            &dir,
            exec::unix_now(),
            &format!("{id}\tpurge\t{} item(s)", out.items),
        );
    }
    Ok(out)
}

/// What the quarantine holds, and where.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineDto {
    #[serde(flatten)]
    summary: QuarantineSummary,
    /// The quarantine folders that exist on this machine.
    roots: Vec<String>,
}

#[tauri::command]
pub async fn quarantine_summary(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<QuarantineDto> {
    let summary = with_history(&app, &state, |h| h.quarantine_summary())?;
    let roots = roots(&app)
        .existing()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    Ok(QuarantineDto { summary, roots })
}

/// Delete everything the quarantine holds for good (the user confirmed it).
#[tauri::command]
pub async fn quarantine_empty(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<PurgeResult> {
    let r = roots(&app);
    let out = with_history(&app, &state, |h| h.empty_quarantine(&r, exec::unix_now()))?;
    nfd_history::uninstall::remove_empty_volume_quarantines(&r);
    if let Some(dir) = audit_dir(&app) {
        let _ = nfd_history::audit::note(
            &dir,
            exec::unix_now(),
            &format!("quarantine\tempty\t{} item(s)", out.items),
        );
    }
    Ok(out)
}

/// Open quarantine folder `index` (of [`quarantine_summary`]'s `roots`) in
/// Explorer.
#[tauri::command]
pub fn quarantine_reveal(app: AppHandle, index: usize) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let root = roots(&app)
        .existing()
        .into_iter()
        .nth(index)
        .ok_or_else(|| err("not-found", index))?;
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(|e| err("os", e))
}

/// Remove an operation from the Activity log (its quarantine is purged first).
#[tauri::command]
pub async fn activity_forget(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    with_history(&app, &state, |h| {
        let _ = h.purge_operation(&id);
        h.forget_operation(&id)
    })
}
