//! DevCache view (PHASE4_DEVCACHE §2.5): developer caches by vendor → app → folder,
//! toolchains, repositories and their build output, package-store commands and
//! unknown caches.
//!
//! Deleting goes the Phase 3 way — `Planner → SafetyGate → Executor → UndoJournal`
//! ([`super::clean::execute_plan`]): rule matches and repository build output land in
//! the quarantine and can be restored from the Activity log. Package-store commands
//! run only when the UI sends back the exact command line it showed and the user
//! confirmed; guide-only commands are never run.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nfd_core::exec;
use nfd_core::safety::SafetyGate;
use nfd_devtools::devcache::{self, DevCacheReport};
use nfd_devtools::process::{self, RunOutput};
use nfd_devtools::repos::{self, RepoOptions, RepoScan};
use nfd_devtools::stores::{self, StoreInfo};
use nfd_devtools::toolchains::{self, Sources, Toolchain};
use nfd_devtools::unknown::{self, UnknownFolder, UnknownOptions};
use nfd_platform::Platform;
use nfd_rules::matcher::{MatchOptions, MatchReport, Matcher};
use serde::Serialize;
use tauri::{AppHandle, State, WebviewWindow};

use super::clean::{self, CandidateDto, OperationDto};
use super::{AppState, CmdResult, err};

/// Entries visited per folder when sizing toolchains, repositories and stores.
const MAX_ENTRIES: u64 = 2_000_000;
/// Longest a package-store command may run.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(600);
/// Where "propose a rule" links may lead.
const ISSUE_PREFIX: &str = "https://github.com/congluuit/NANOFreeDisk/issues/new?";

/// Results of the last DevCache scans and the running action, if any.
#[derive(Debug, Default)]
pub struct DevState {
    report: Mutex<Option<MatchReport>>,
    repos: Mutex<Option<RepoScan>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// Paths the UI may reveal in Explorer (everything the scans showed).
    shown: Mutex<HashSet<String>>,
}

fn key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

impl DevState {
    pub(super) fn show<'a>(&self, paths: impl IntoIterator<Item = &'a Path>) {
        if let Ok(mut s) = self.shown.lock() {
            s.extend(paths.into_iter().map(key));
        }
    }

    pub(super) fn start(&self) -> CmdResult<Arc<AtomicBool>> {
        let cancel = Arc::new(AtomicBool::new(false));
        *self.cancel.lock().map_err(|e| err("internal", e))? = Some(cancel.clone());
        Ok(cancel)
    }

    pub(super) fn stop(&self) {
        if let Ok(mut c) = self.cancel.lock() {
            *c = None;
        }
    }
}

fn gate() -> SafetyGate {
    SafetyGate::new(&nfd_platform::Native.resolver())
}

/// Vendor thresholds from the UI (GB) merged over the defaults.
fn thresholds(user: Option<BTreeMap<String, f64>>) -> BTreeMap<String, u64> {
    let mut t = devcache::default_thresholds();
    for (vendor, gb) in user.unwrap_or_default() {
        t.retain(|k, _| !k.eq_ignore_ascii_case(&vendor));
        if gb > 0.0 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            t.insert(vendor, (gb * f64::from(1u32 << 30)) as u64);
        }
    }
    t
}

/// Match the DevCache packs (read-only) and group them vendor → app → folder.
/// `thresholds` maps vendor names to GB (0 = no reminder).
#[tauri::command]
pub async fn devcache_scan(
    dev: State<'_, DevState>,
    rules_dir: Option<String>,
    thresholds: Option<BTreeMap<String, f64>>,
) -> CmdResult<DevCacheReport> {
    let packs = clean::load_packs(rules_dir)?;
    let limits = self::thresholds(thresholds);
    let (report, grouped) = tauri::async_runtime::spawn_blocking(move || {
        let dev_packs = devcache::packs(&packs);
        let ids: Vec<String> = dev_packs.iter().map(|p| p.pack.id.clone()).collect();
        let resolve = nfd_platform::Native.resolver();
        // every pack's 🔒 Keep paths protect the DevCache rules too
        let matcher = Matcher::new(&packs, &resolve, std::env::consts::OS);
        let running = nfd_platform::locks::running_processes().unwrap_or_default();
        let report = matcher.scan(
            &|_, pack| ids.iter().any(|p| p == pack),
            &MatchOptions::default(),
            &running,
        );
        let present = devcache::present_rules(&dev_packs, &resolve);
        let grouped = devcache::group(&report, &ids, &limits, &present);
        (report, grouped)
    })
    .await
    .map_err(|e| err("internal", e))?;
    dev.show(report.candidates.iter().map(|c| c.path.as_path()));
    *dev.report.lock().map_err(|e| err("internal", e))? = Some(report);
    Ok(grouped)
}

/// The largest items one rule would clean (preview).
#[tauri::command]
pub fn devcache_items(
    dev: State<'_, DevState>,
    rule_id: String,
    limit: usize,
) -> CmdResult<Vec<CandidateDto>> {
    let guard = dev.report.lock().map_err(|e| err("internal", e))?;
    let report = guard
        .as_ref()
        .ok_or_else(|| err("no-clean-scan", "no DevCache scan yet"))?;
    Ok(clean::largest(report, &rule_id, limit))
}

/// Clean every candidate of the ticked `rule_ids` (quarantine, restorable from the
/// Activity log). Progress streams as `clean-progress`.
#[tauri::command]
pub async fn devcache_run(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    dev: State<'_, DevState>,
    rule_ids: Vec<String>,
    expert: bool,
    permanent: bool,
) -> CmdResult<OperationDto> {
    let plan = {
        let guard = dev.report.lock().map_err(|e| err("internal", e))?;
        let report = guard
            .as_ref()
            .ok_or_else(|| err("no-clean-scan", "no DevCache scan yet"))?;
        clean::rules_plan(report, &rule_ids, expert, permanent)
    };
    let cancel = dev.start()?;
    let result = clean::execute_plan(&app, &window, &state, plan, "devcache", cancel).await;
    dev.stop();
    *dev.report.lock().map_err(|e| err("internal", e))? = None;
    result
}

/// Stop the running clean, repository scan or command.
#[tauri::command]
pub fn devcache_cancel(dev: State<'_, DevState>) -> CmdResult<()> {
    if let Some(c) = dev.cancel.lock().map_err(|e| err("internal", e))?.as_ref() {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Cache-like folders without a rule (🟡 unverified, never cleaned).
#[tauri::command]
pub async fn devcache_unknown(
    dev: State<'_, DevState>,
    min_mb: Option<u64>,
) -> CmdResult<Vec<UnknownFolder>> {
    let found = tauri::async_runtime::spawn_blocking(move || {
        let packs = nfd_rules::builtin_packs().unwrap_or_default();
        let resolve = nfd_platform::Native.resolver();
        let opts = UnknownOptions {
            min_bytes: min_mb.map_or(UnknownOptions::default().min_bytes, |m| m << 20),
            ..UnknownOptions::default()
        };
        unknown::find(&packs, &resolve, &gate(), opts)
    })
    .await
    .map_err(|e| err("internal", e))?;
    dev.show(found.iter().map(|f| f.path.as_path()));
    Ok(found)
}

/// Installed toolchains with their uninstall command (read-only).
#[tauri::command]
pub async fn devcache_toolchains(dev: State<'_, DevState>) -> CmdResult<Vec<Toolchain>> {
    let list = tauri::async_runtime::spawn_blocking(|| {
        let resolve = nfd_platform::Native.resolver();
        let env = |k: &str| std::env::var(k).ok();
        let sources = Sources {
            resolve: &resolve,
            env: &env,
            vswhere: toolchains::run_vswhere(&resolve),
        };
        toolchains::inventory(&sources, MAX_ENTRIES)
    })
    .await
    .map_err(|e| err("internal", e))?;
    dev.show(list.iter().map(|t| t.path.as_path()));
    Ok(list)
}

/// Find the git repositories and projects under `root` with their build output.
#[tauri::command]
pub async fn devcache_repos(
    dev: State<'_, DevState>,
    root: String,
    keep_recent_days: u32,
    stale_days: u32,
    git_status: bool,
) -> CmdResult<RepoScan> {
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Err(err("not-found", root.display()));
    }
    let cancel = dev.start()?;
    let scan = tauri::async_runtime::spawn_blocking(move || {
        let opts = RepoOptions {
            keep_recent_days,
            stale_days,
            git_status,
            max_entries: MAX_ENTRIES,
            ..RepoOptions::default()
        };
        let git = process::git();
        repos::find(&root, opts, git.as_deref(), &gate(), Some(&cancel))
    })
    .await
    .map_err(|e| err("internal", e))?;
    dev.stop();
    dev.show(scan.repos.iter().flat_map(|r| {
        std::iter::once(r.path.as_path()).chain(r.artifacts.iter().map(|a| a.path.as_path()))
    }));
    *dev.repos.lock().map_err(|e| err("internal", e))? = Some(scan.clone());
    Ok(scan)
}

/// Move the chosen build-output folders of the last repository scan to the
/// quarantine (hand-picked items: Planner → SafetyGate → Executor → journal).
#[tauri::command]
pub async fn devcache_repos_clean(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    dev: State<'_, DevState>,
    paths: Vec<String>,
) -> CmdResult<OperationDto> {
    let wanted: HashSet<String> = paths.iter().map(|p| key(Path::new(p))).collect();
    let plan = {
        let guard = dev.repos.lock().map_err(|e| err("internal", e))?;
        let scan = guard
            .as_ref()
            .ok_or_else(|| err("no-repo-scan", "no repository scan yet"))?;
        repos::plan(
            scan.repos
                .iter()
                .flat_map(|r| &r.artifacts)
                .filter(|a| wanted.contains(&key(&a.path))),
        )
    };
    let cancel = dev.start()?;
    let result = clean::execute_plan(&app, &window, &state, plan, "devcache", cancel).await;
    dev.stop();
    *dev.repos.lock().map_err(|e| err("internal", e))? = None;
    result
}

/// Package-store cleanup commands for the tools on this machine.
#[tauri::command]
pub async fn devcache_stores(dev: State<'_, DevState>) -> CmdResult<Vec<StoreInfo>> {
    let list = tauri::async_runtime::spawn_blocking(|| {
        let resolve = nfd_platform::Native.resolver();
        stores::list(&resolve, &process::which, MAX_ENTRIES)
    })
    .await
    .map_err(|e| err("internal", e))?;
    dev.show(
        list.iter()
            .flat_map(|s| s.stores.iter().map(PathBuf::as_path)),
    );
    Ok(list)
}

/// Result of a package-store command.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreRunDto {
    id: String,
    op_id: String,
    before: u64,
    after: u64,
    freed: u64,
    output: RunOutput,
    journal_error: Option<String>,
}

/// Run the cleanup command `id` — only when `command_line` is exactly the line the
/// user confirmed and the command can be undone by downloading again.
#[tauri::command]
pub async fn devcache_store_run(
    app: AppHandle,
    state: State<'_, AppState>,
    dev: State<'_, DevState>,
    id: String,
    command_line: String,
) -> CmdResult<StoreRunDto> {
    let cancel = dev.start()?;
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let resolve = nfd_platform::Native.resolver();
        let list = stores::list(&resolve, &process::which, MAX_ENTRIES);
        let info = list
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| err("not-found", &id))?;
        let started = exec::unix_now();
        let result = stores::run(
            &info,
            &command_line,
            COMMAND_TIMEOUT,
            Some(&cancel),
            MAX_ENTRIES,
        )
        .map_err(|e| err("command", e))?;
        let op = stores::operation(
            &info,
            &result,
            exec::new_op_id(started),
            started,
            exec::unix_now(),
        );
        Ok::<_, super::CmdError>((result, op))
    })
    .await
    .map_err(|e| err("internal", e));
    dev.stop();
    let (result, op) = outcome??;
    let journal_error = clean::journal(&app, &state, &op);
    Ok(StoreRunDto {
        id: result.id.to_owned(),
        op_id: op.id,
        before: result.before,
        after: result.after,
        freed: result.freed,
        output: result.output,
        journal_error,
    })
}

/// Show a folder the DevCache view listed in Explorer.
#[tauri::command]
pub fn devcache_reveal(app: AppHandle, dev: State<'_, DevState>, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = PathBuf::from(&path);
    let known = dev
        .shown
        .lock()
        .map_err(|e| err("internal", e))?
        .contains(&key(&p));
    if !known {
        return Err(err("not-found", path));
    }
    app.opener()
        .reveal_item_in_dir(&p)
        .map_err(|e| err("os", e))
}

/// Open a pre-filled "Cache rule proposal" issue on GitHub.
#[tauri::command]
pub fn devcache_propose(app: AppHandle, url: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if !url.starts_with(ISSUE_PREFIX) || url.contains(['\n', '\r', ' ']) {
        return Err(err("bad-url", url));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| err("os", e))
}

/// System notification for vendors over their threshold (texts from the UI).
#[tauri::command]
pub fn devcache_notify(app: AppHandle, title: String, body: String) -> CmdResult<()> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| err("os", e))
}
