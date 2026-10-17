//! Clean view → "Web browser caches" → "Advanced: clean one website"
//! (`nfd_browsers`): which websites own how much of each browser profile's caches,
//! and removing only the chosen sites' entries.
//!
//! Cleaning goes the Phase 3 way — whole entry files (plus the advisory index the
//! browser then rebuilds) or whole CacheStorage origin folders through
//! `Planner → SafetyGate → Executor → UndoJournal` into the quarantine (kind
//! `browser-site`, restorable from the Activity log). Never cookies, logins, Local
//! Storage, IndexedDB, History, passwords or extensions; the blockfile HTTP cache is
//! listed only. Nothing is planned while the browser runs, and the browser is
//! checked again while the items move.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nfd_browsers::{Env, Report, ScanOptions, StoreTotal, Target};
use nfd_core::exec;
use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use super::clean::{self, OperationDto};
use super::{AppState, CmdResult, err};

/// The running per-site clean, if any.
#[derive(Debug, Default)]
pub struct BrowserState {
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

/// This machine, or (GUI smoke test only) a fixture laid out as `<dir>/Local`,
/// `<dir>/Roaming`, with `running.txt` standing for the running processes.
fn env(fixture: Option<&Path>) -> Env {
    fixture.map_or_else(Env::detect, Env::fixture)
}

fn fixture_dir(fixture: Option<String>) -> Option<PathBuf> {
    fixture.filter(|f| !f.trim().is_empty()).map(PathBuf::from)
}

/// Sites per browser profile (read-only).
#[tauri::command]
pub async fn browsers_sites(fixture: Option<String>) -> CmdResult<Report> {
    let fixture = fixture_dir(fixture);
    tauri::async_runtime::spawn_blocking(move || {
        nfd_browsers::scan(&env(fixture.as_deref()), &ScanOptions::default())
    })
    .await
    .map_err(|e| err("internal", e))
}

/// What cleaning `targets` removes (shown in the confirmation).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitePlanDto {
    items: usize,
    entries: u64,
    bytes: u64,
    by_store: Vec<StoreTotal>,
    index_files: u64,
    listed_only: u64,
    running: Vec<String>,
    refused: usize,
}

#[tauri::command]
pub async fn browsers_plan(
    targets: Vec<Target>,
    fixture: Option<String>,
) -> CmdResult<SitePlanDto> {
    let fixture = fixture_dir(fixture);
    let p = tauri::async_runtime::spawn_blocking(move || {
        nfd_browsers::plan(
            &env(fixture.as_deref()),
            &targets,
            ScanOptions::default().max_entries,
        )
    })
    .await
    .map_err(|e| err("internal", e))?;
    Ok(SitePlanDto {
        items: p.plan.items.len(),
        entries: p.entries,
        bytes: p.bytes,
        by_store: p.by_store,
        index_files: p.index_files,
        listed_only: p.listed_only,
        running: p.running,
        refused: p.refused.len(),
    })
}

/// Clean the chosen sites' cache (already confirmed). Progress streams as
/// `clean-progress`; [`browsers_cancel`] stops between items.
#[tauri::command]
pub async fn browsers_clean(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    browsers: State<'_, BrowserState>,
    targets: Vec<Target>,
    fixture: Option<String>,
) -> CmdResult<OperationDto> {
    let fixture = fixture_dir(fixture);
    let fx = fixture.clone();
    let p = tauri::async_runtime::spawn_blocking(move || {
        nfd_browsers::plan(
            &env(fx.as_deref()),
            &targets,
            ScanOptions::default().max_entries,
        )
    })
    .await
    .map_err(|e| err("internal", e))?;
    if p.plan.items.is_empty() {
        return Err(if p.running.is_empty() {
            err("nothing-to-clean", "nothing to clean")
        } else {
            err("app-running", p.running.join(", "))
        });
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *browsers.cancel.lock().map_err(|e| err("internal", e))? = Some(cancel.clone());
    // the browser may have been started since the plan: look again every 200 items
    let mut running = env(fixture.as_deref()).running;
    let mut n = 0u32;
    let verify: clean::Verify = Box::new(move |item| {
        n += 1;
        if n % 200 == 0 {
            running = env(fixture.as_deref()).running;
        }
        match item
            .requires_closed
            .iter()
            .find(|exe| nfd_platform::locks::is_running(&running, exe))
        {
            Some(exe) => Err(exec::Refusal {
                code: "app-running",
                message: format!("{exe} is running"),
            }),
            None => Ok(()),
        }
    });
    let result = clean::execute_plan_returning(
        &app,
        &window,
        &state,
        p.plan,
        "browser-site",
        cancel,
        verify,
    )
    .await
    .map(|(dto, _)| dto);
    *browsers.cancel.lock().map_err(|e| err("internal", e))? = None;
    result
}

/// Stop a running per-site clean after the current item.
#[tauri::command]
pub fn browsers_cancel(browsers: State<'_, BrowserState>) -> CmdResult<()> {
    if let Some(c) = browsers
        .cancel
        .lock()
        .map_err(|e| err("internal", e))?
        .as_ref()
    {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}
