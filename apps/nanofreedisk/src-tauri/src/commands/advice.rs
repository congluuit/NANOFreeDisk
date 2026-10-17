//! Analyze → Suggestions: what can be deleted, what needs care, what must stay, for
//! every path of the loaded scan ([`nfd_advice`]).
//!
//! The advice is computed in the background after every scan and every deletion
//! (event `advice-ready`); agent worktrees are then checked with git through the AI
//! leftovers module (a second `advice-ready` with `enriched`). Cleaning picked
//! suggestions goes the existing ways: rules by their own walk and everything else
//! hand-picked through `Planner → SafetyGate → Executor → UndoJournal` into the
//! quarantine (kind `advice`), worktrees with git (kind `worktree`), personal files
//! into the Recycle Bin (kind `manual`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use nfd_advice::{Advice, EntryAdvice, Group, Knowledge, Suggestions};
use nfd_core::RiskLevel;
use nfd_core::exec::{self, Operation};
use nfd_core::plan::{self, CleanPlan, DeleteMode, PlanOptions};
use nfd_core::safety::SafetyGate;
use nfd_devtools::ai_leftovers::{self, Sources};
use nfd_devtools::process;
use nfd_platform::Platform;
use nfd_rules::matcher::MatchOptions;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use super::ai_leftovers::{self as ai_cmd, AiState};
use super::clean::{self, CleanState, DeleteOutcome, OperationDto};
use super::dupes::DupeState;
use super::{AppState, CmdResult, err, with_session};

/// Knowledge, the running refresh and the running clean.
#[derive(Debug, Default)]
pub struct AdviceState {
    knowledge: Mutex<Option<Arc<Knowledge>>>,
    /// Rule packs used instead of the built-in ones (GUI smoke test only).
    rules_dir: Mutex<Option<String>>,
    /// Number of the latest refresh: an older one drops its result.
    run: AtomicU32,
    /// Refreshes running.
    busy: AtomicU32,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl AdviceState {
    fn knowledge(&self) -> CmdResult<Arc<Knowledge>> {
        let mut slot = self.knowledge.lock().map_err(|e| err("internal", e))?;
        if let Some(k) = slot.as_ref() {
            return Ok(k.clone());
        }
        let resolve = nfd_platform::Native.resolver();
        let dir = self
            .rules_dir
            .lock()
            .map_err(|e| err("internal", e))?
            .clone();
        let k = match dir {
            Some(d) => {
                let packs = clean::load_packs(Some(d))?;
                Knowledge::new(
                    &packs,
                    nfd_rules::encyclopedia::builtin(),
                    &resolve,
                    nfd_advice::current_platform(),
                )
            }
            None => Knowledge::builtin(&resolve).map_err(|e| err("rules", e))?,
        };
        let k = Arc::new(k);
        *slot = Some(k.clone());
        Ok(k)
    }
}

/// Sent when an advice is stored.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdviceReady {
    seq: u32,
    enriched: bool,
}

fn now() -> i64 {
    nfd_rules::matcher::now()
}

/// Recompute the advice of the loaded scan in the background.
pub fn refresh(app: &AppHandle) {
    let st = app.state::<AdviceState>();
    let run = st.run.fetch_add(1, Ordering::SeqCst) + 1;
    st.busy.fetch_add(1, Ordering::SeqCst);
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("advice".into())
        .spawn(move || {
            compute(&app, run);
            app.state::<AdviceState>()
                .busy
                .fetch_sub(1, Ordering::SeqCst);
        });
    if spawned.is_err() {
        st.busy.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Store `advice` if the session is still scan `seq` and no newer refresh started.
fn store(app: &AppHandle, run: u32, seq: u32, advice: Advice) -> bool {
    if app.state::<AdviceState>().run.load(Ordering::SeqCst) != run {
        return false;
    }
    let state = app.state::<AppState>();
    let Ok(mut guard) = state.session.write() else {
        return false;
    };
    match guard.as_mut() {
        Some(s) if s.seq == seq => {
            let enriched = advice.enriched;
            s.advice = Some(Arc::new(advice));
            drop(guard);
            let _ = app.emit("advice-ready", AdviceReady { seq, enriched });
            true
        }
        _ => false,
    }
}

fn compute(app: &AppHandle, run: u32) {
    let Ok(k) = app.state::<AdviceState>().knowledge() else {
        return;
    };
    let copies = app.state::<DupeState>().copies();
    let state = app.state::<AppState>();
    let first = {
        let Ok(guard) = state.session.read() else {
            return;
        };
        let Some(s) = guard.as_ref() else {
            return;
        };
        let mut a = nfd_advice::advise(&s.tree, &s.children, &s.ends, &k, now());
        if !copies.is_empty() {
            a.apply_duplicates(&s.tree, &s.children, &copies);
        }
        let resolve = nfd_platform::Native.resolver();
        let env = |key: &str| std::env::var(key).ok();
        let sources = Sources::detect(&resolve, &env);
        let agent_dirs = sources
            .temp_claude
            .iter()
            .chain(&sources.playwright)
            .any(|p| nfd_advice::in_scan(&s.tree, p));
        (s.seq, a, agent_dirs, sources)
    };
    let (seq, advice, agent_dirs, sources) = first;
    if !store(app, run, seq, advice.clone()) {
        return;
    }
    if advice.agent_projects.is_empty() && !agent_dirs {
        return;
    }
    // agent worktrees: the AI leftovers check says what git and the sessions allow
    let ai = app.state::<AiState>();
    let covered = ai.latest().filter(|r| {
        advice
            .agent_projects
            .iter()
            .all(|p| r.projects.iter().any(|q| q == p))
    });
    let report = covered.unwrap_or_else(|| {
        let resolve = nfd_platform::Native.resolver();
        let gate = SafetyGate::new(&resolve);
        let r = ai_leftovers::scan(
            &ai_leftovers::Options {
                roots: advice.agent_projects.clone(),
                ..ai_leftovers::Options::default()
            },
            &sources,
            process::git().as_deref(),
            &gate,
            None,
        );
        ai.set(r.clone());
        r
    });
    let enriched = {
        let Ok(guard) = state.session.read() else {
            return;
        };
        let Some(s) = guard.as_ref().filter(|s| s.seq == seq) else {
            return;
        };
        let mut a = advice;
        a.apply_ai(&s.tree, &s.children, &s.ends, &report);
        a
    };
    store(app, run, seq, enriched);
}

/// What the Suggestions panel shows.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdviceSummary {
    /// A refresh is running.
    computing: bool,
    seq: u32,
    root: String,
    /// `None` until the first advice of this scan is ready.
    suggestions: Option<Suggestions>,
    /// Junk the rule packs found outside the scanned folder.
    elsewhere: Vec<Group>,
}

#[tauri::command]
pub fn advice_summary(
    state: State<'_, AppState>,
    adv: State<'_, AdviceState>,
    clean_state: State<'_, CleanState>,
) -> CmdResult<AdviceSummary> {
    let (seq, root, suggestions) = with_session(&state, |s| {
        (
            s.seq,
            s.tree.root.clone(),
            s.advice.as_ref().map(|a| a.suggestions(&s.tree)),
        )
    })?;
    let elsewhere = adv
        .knowledge()
        .ok()
        .and_then(|k| clean_state.with_report(|r| nfd_advice::elsewhere(r, &root, &k)))
        .unwrap_or_default();
    Ok(AdviceSummary {
        computing: adv.busy.load(Ordering::SeqCst) > 0,
        seq,
        root: root.to_string_lossy().into_owned(),
        suggestions,
        elsewhere,
    })
}

#[tauri::command]
pub fn advice_detail(state: State<'_, AppState>, id: u32) -> CmdResult<Option<EntryAdvice>> {
    let resolve = nfd_platform::Native.resolver();
    with_session(&state, |s| {
        let mut d = s
            .advice
            .as_ref()
            .and_then(|a| a.detail(&s.tree, &s.ends, id))?;
        // "Delete anyway" also needs the SafetyGate's lexical yes for this path
        if d.override_block.is_none() {
            d.override_block = override_gate(s, id, &resolve);
        }
        Some(d)
    })
}

/// The SafetyGate's refusal of `id` for a hand-picked delete, if any (`root` for the
/// scanned folder itself).
fn override_gate(
    s: &crate::session::Session,
    id: u32,
    resolve: &impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let plan = s.plan_delete(&[id], resolve);
    match plan.items.first() {
        None => Some("root".to_owned()),
        Some(i) if !i.allowed => Some(i.reason.unwrap_or("blocked").to_owned()),
        Some(_) => None,
    }
}

/// Compute the advice again; `rules_dir` replaces the built-in rule packs (GUI smoke
/// test only — `None` goes back to the built-in ones).
#[tauri::command]
pub fn advice_refresh(
    app: AppHandle,
    adv: State<'_, AdviceState>,
    rules_dir: Option<String>,
) -> CmdResult<()> {
    let dir = rules_dir.filter(|d| !d.trim().is_empty());
    {
        let mut slot = adv.rules_dir.lock().map_err(|e| err("internal", e))?;
        if *slot != dir {
            *slot = dir;
            *adv.knowledge.lock().map_err(|e| err("internal", e))? = None;
        }
    }
    refresh(&app);
    Ok(())
}

/// Result of cleaning picked suggestions.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdviceOutcome {
    /// Rules, build output, AI leftovers and worktrees (quarantine / git).
    operation: Option<OperationDto>,
    /// Personal picks moved to the Recycle Bin.
    recycled: Vec<DeleteOutcome>,
    /// Units left out: (entry id, `caution-not-confirmed` / `expert-only` /
    /// `contains-kept` / `blocked`).
    skipped: Vec<(u32, &'static str)>,
}

/// Clean the suggestions `keys` (group keys of [`advice_summary`], `elsewhere|<rule>`
/// for junk outside the scan). 🟡 groups need `caution` (ticked knowingly), 🟠
/// Advanced ones `expert`. Kept, guide-only and blocked units are never acted on.
#[tauri::command]
pub async fn advice_clean(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    adv: State<'_, AdviceState>,
    keys: Vec<String>,
    caution: bool,
    expert: bool,
) -> CmdResult<AdviceOutcome> {
    let k = adv.knowledge()?;
    let (prepared, root) = with_session(&state, |s| {
        s.advice.as_ref().map(|a| {
            let units = a.actionable_units(&s.tree, &s.ends, &keys);
            (
                a.prepare(&s.tree, &s.ends, &units, caution, expert),
                s.tree.root.clone(),
            )
        })
    })?
    .ok_or_else(|| err("no-advice", "the advice is not ready yet"))?;
    {
        let mut slot = adv.cancel.lock().map_err(|e| err("internal", e))?;
        if slot.is_some() {
            return Err(err("busy", "a clean is already running"));
        }
        *slot = Some(Arc::new(AtomicBool::new(false)));
    }
    let result = run_clean(
        &app, &window, &state, &k, &keys, prepared, &root, caution, expert,
    )
    .await;
    *adv.cancel.lock().map_err(|e| err("internal", e))? = None;
    refresh(&app);
    result
}

/// "Delete anyway": the user overrules the app's judgment on `ids` (kept, guide
/// only, blocked by a session or by git). Never on a hard protection — the entry is,
/// lies inside or holds something the SafetyGate refuses or conversation history —
/// and only with `ack`. Nothing is deleted for good: an agent worktree git may remove
/// still goes the git way (`git worktree remove`, the branch stays), everything else
/// moves to the quarantine (Activity kind `override`, restorable for 7 days).
#[tauri::command]
pub async fn advice_override(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    adv: State<'_, AdviceState>,
    ids: Vec<u32>,
    ack: bool,
) -> CmdResult<AdviceOutcome> {
    if !ack {
        return Err(err(
            "not-confirmed",
            "Delete anyway needs the acknowledgement",
        ));
    }
    let resolve = nfd_platform::Native.resolver();
    let (plain, ai_paths, mut skipped) = with_session(&state, |s| {
        let a = s.advice.as_ref()?;
        let mut plain = Vec::new();
        let mut ai_paths = Vec::new();
        let mut skipped: Vec<(u32, &'static str)> = Vec::new();
        let mut ids = ids.clone();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            if let Some(code) = a.override_block(&s.tree, id) {
                skipped.push((id, code));
                continue;
            }
            let plan = s.plan_delete(&[id], &resolve);
            match plan.items.first() {
                None => {
                    skipped.push((id, "root"));
                    continue;
                }
                Some(i) if !i.allowed => {
                    skipped.push((id, i.reason.unwrap_or("blocked")));
                    continue;
                }
                Some(_) => {}
            }
            let agent = a.unit(id).is_some_and(|u| {
                u.entry == id
                    && matches!(
                        u.reason,
                        nfd_advice::Reason::AgentWorktree | nfd_advice::Reason::AgentLeftover
                    )
            }) && a.enriched;
            match (agent, s.path(id)) {
                (true, Some(p)) => ai_paths.push((id, p)),
                _ => plain.push(id),
            }
        }
        Some((plain, ai_paths, skipped))
    })?
    .ok_or_else(|| err("no-advice", "the advice is not ready yet"))?;
    let cancel = {
        let mut slot = adv.cancel.lock().map_err(|e| err("internal", e))?;
        if slot.is_some() {
            return Err(err("busy", "a clean is already running"));
        }
        let c = Arc::new(AtomicBool::new(false));
        *slot = Some(c.clone());
        c
    };
    let result = async {
        let mut items = Vec::new();
        for id in &plain {
            if let Some(p) = with_session(&state, |s| s.plan_item(*id, DeleteMode::Quarantine))? {
                items.push(p);
            }
        }
        let mut jobs = Vec::new();
        if !ai_paths.is_empty() {
            let report = app.state::<AiState>().latest();
            let paths: Vec<PathBuf> = ai_paths.iter().map(|(_, p)| p.clone()).collect();
            match report {
                Some(r) => {
                    let (p, refused) = ai_leftovers::prepare_forced(&r, &paths);
                    items.extend(p.plan.items);
                    jobs = p.jobs;
                    for (id, path) in &ai_paths {
                        if refused.contains(path) {
                            skipped.push((*id, "hard"));
                        }
                    }
                }
                // the check that judged them is gone: move them like any folder
                None => {
                    for (id, _) in &ai_paths {
                        if let Some(p) =
                            with_session(&state, |s| s.plan_item(*id, DeleteMode::Quarantine))?
                        {
                            items.push(p);
                        }
                    }
                }
            }
        }
        let mut plan: CleanPlan = plan::build(items, PlanOptions::default());
        for item in &mut plan.items {
            item.mode = DeleteMode::Quarantine;
            item.selected = true;
        }
        let mut all = Vec::new();
        let mut journal_error = None;
        let mut cancelled = false;
        if plan.selected().next().is_some() {
            let (dto, op) = clean::execute_plan_returning(
                &app,
                &window,
                &state,
                plan,
                "override",
                cancel.clone(),
                Box::new(|_| Ok(())),
            )
            .await?;
            journal_error = dto.journal_error;
            cancelled |= op.cancelled;
            all.extend(op.items);
        }
        if !jobs.is_empty() && !cancelled {
            let (done, problem, stopped) =
                ai_cmd::remove_worktrees(&app, &state, jobs, cancel.clone()).await?;
            journal_error = journal_error.or(problem);
            cancelled |= stopped;
            all.extend(done);
            app.state::<AiState>().clear();
        }
        {
            let mut guard = state.session.write().map_err(|e| err("internal", e))?;
            if let Some(s) = guard.as_mut() {
                let done: Vec<u32> = all
                    .iter()
                    .filter(|i| i.status == exec::ItemStatus::Done)
                    .filter_map(|i| nfd_advice::locate(&s.tree, &s.children, &i.path))
                    .collect();
                for id in done {
                    s.mark_removed(id);
                }
            }
        }
        if all.is_empty() {
            return Err(err("nothing-to-clean", "nothing to delete"));
        }
        Ok(AdviceOutcome {
            operation: Some(clean::op_dto(
                &Operation {
                    id: String::new(),
                    kind: "override".into(),
                    started_at: 0,
                    finished_at: 0,
                    items: all,
                    cancelled,
                },
                journal_error,
            )),
            recycled: Vec::new(),
            skipped: skipped.clone(),
        })
    }
    .await;
    *adv.cancel.lock().map_err(|e| err("internal", e))? = None;
    refresh(&app);
    result
}

/// Stop a running advice clean after the current item.
#[tauri::command]
pub fn advice_cancel(adv: State<'_, AdviceState>) -> CmdResult<()> {
    if let Some(c) = adv.cancel.lock().map_err(|e| err("internal", e))?.as_ref() {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_clean(
    app: &AppHandle,
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    k: &Arc<Knowledge>,
    keys: &[String],
    prepared: nfd_advice::Prepared,
    root: &std::path::Path,
    caution: bool,
    expert: bool,
) -> CmdResult<AdviceOutcome> {
    let cancel = app
        .state::<AdviceState>()
        .cancel
        .lock()
        .map_err(|e| err("internal", e))?
        .clone()
        .unwrap_or_default();
    let mut items = prepared.quarantine.clone();
    // rules: their own walk, inside the picked folders only
    if !prepared.rule_ids.is_empty() {
        let ids = prepared.rule_ids.clone();
        let roots = prepared.rule_roots.clone();
        let k = k.clone();
        let rule_items = tauri::async_runtime::spawn_blocking(move || {
            let running = nfd_platform::locks::running_processes().unwrap_or_default();
            let report = k.matcher().scan(
                &|r: &nfd_rules::Rule, _: &str| ids.contains(&r.id),
                &MatchOptions {
                    now: now(),
                    ..MatchOptions::default()
                },
                &running,
            );
            nfd_advice::rule_items(&report, &roots)
        })
        .await
        .map_err(|e| err("internal", e))?;
        items.extend(rule_items);
    }
    // junk elsewhere: the Clean view's last rule scan
    let elsewhere: Vec<String> = keys
        .iter()
        .filter_map(|k| k.strip_prefix("elsewhere|"))
        .map(str::to_owned)
        .collect();
    if !elsewhere.is_empty() {
        let found = app
            .state::<CleanState>()
            .with_report(|r| nfd_advice::elsewhere_items(r, root, &elsewhere))
            .unwrap_or_default();
        items.extend(found.into_iter().filter(|i| {
            let level = i.level();
            (level != RiskLevel::Caution || caution) && (level != RiskLevel::Advanced || expert)
        }));
    }
    // AI leftovers: folders into the quarantine, worktrees with git
    let mut jobs = Vec::new();
    if !prepared.ai_paths.is_empty() {
        if let Some(report) = app.state::<AiState>().latest() {
            let p = ai_leftovers::prepare(&report, &prepared.ai_paths);
            items.extend(p.plan.items);
            jobs = p.jobs;
        }
    }
    let mut plan: CleanPlan = plan::build(
        items,
        PlanOptions {
            expert,
            include_caution: true,
            permanent: false,
        },
    );
    for item in &mut plan.items {
        item.selected = item.level() != RiskLevel::Advanced || expert;
        if item.mode != DeleteMode::Quarantine {
            item.mode = DeleteMode::Quarantine;
        }
    }
    let mut all = Vec::new();
    let mut journal_error = None;
    let mut cancelled = false;
    if plan.selected().next().is_some() {
        let (dto, op) = clean::execute_plan_returning(
            app,
            window,
            state,
            plan,
            "advice",
            cancel.clone(),
            Box::new(|_| Ok(())),
        )
        .await?;
        journal_error = dto.journal_error;
        cancelled |= op.cancelled;
        all.extend(op.items);
    }
    if !jobs.is_empty() && !cancelled {
        let (done, problem, stopped) =
            ai_cmd::remove_worktrees(app, state, jobs, cancel.clone()).await?;
        journal_error = journal_error.or(problem);
        cancelled |= stopped;
        all.extend(done);
        app.state::<AiState>().clear();
    }
    // what is gone leaves the scan
    {
        let mut guard = state.session.write().map_err(|e| err("internal", e))?;
        if let Some(s) = guard.as_mut() {
            let done: Vec<u32> = all
                .iter()
                .filter(|i| i.status == exec::ItemStatus::Done)
                .filter_map(|i| nfd_advice::locate(&s.tree, &s.children, &i.path))
                .collect();
            for id in done {
                s.mark_removed(id);
            }
        }
    }
    let recycled = if prepared.recycle.is_empty() || cancelled {
        Vec::new()
    } else {
        clean::delete_entries_impl(
            app,
            window,
            state,
            &prepared.recycle,
            DeleteMode::RecycleBin,
            "manual",
        )
        .await?
    };
    let operation = (!all.is_empty()).then(|| {
        clean::op_dto(
            &Operation {
                id: String::new(),
                kind: "advice".into(),
                started_at: 0,
                finished_at: 0,
                items: all,
                cancelled,
            },
            journal_error,
        )
    });
    if operation.is_none() && recycled.is_empty() {
        return Err(err("nothing-to-clean", "nothing to clean"));
    }
    Ok(AdviceOutcome {
        operation,
        recycled,
        skipped: prepared.skipped,
    })
}

/// Bytes the advice of the loaded scan can free, for the Health Score: (root,
/// volume size, reclaimable). `None` without a scan or advice.
pub fn health_facts(state: &AppState) -> Option<(String, u64, u64)> {
    let guard = state.session.read().ok()?;
    let s = guard.as_ref()?;
    let a = s.advice.as_ref()?;
    Some((
        s.tree.root.to_string_lossy().into_owned(),
        s.volume_total,
        a.reclaim(&s.tree, 0),
    ))
}

/// The scanned folder, for findings that must not count what the advice covers.
pub fn scanned_root(state: &AppState) -> Option<PathBuf> {
    let guard = state.session.read().ok()?;
    guard
        .as_ref()
        .filter(|s| s.advice.is_some())
        .map(|s| s.tree.root.clone())
}
