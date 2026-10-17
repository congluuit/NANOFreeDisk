//! Tauri commands: thin wrappers around [`Session`] plus scanning, deleting (see
//! [`clean`]) and settings. Errors cross the bridge as `{ code, message }` so the UI can localize them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

pub mod ai_leftovers;
pub mod clean;
pub mod devcache;
pub mod diskhealth;
pub mod dupes;

use nfd_core::health::{self, HealthScore, JunkFacts, ScanFacts, VolumeFacts};
use nfd_core::insights::OldLargeOptions;
use nfd_core::query::Filter;
use nfd_core::scanner::{self, ScanEvent, ScanOptions, ScanStats};
use nfd_history::{Diff, DiffOptions, History, RecordOptions, Snapshot, VolumeState};
use nfd_platform::Platform;
use nfd_platform::volumes::{self, VolumeKind};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

pub mod hwmon;
pub mod monitor;
pub mod update;

use crate::session::{
    self, AgeDto, DeletePlan, Detail, EmptyRows, ExtRow, Located, Node, OldLargeRows, SearchRows,
    Session, ShortcutRows, Slice,
};

/// Shared application state.
#[derive(Debug, Default)]
pub struct AppState {
    session: RwLock<Option<Session>>,
    scan: Mutex<Option<Arc<AtomicBool>>>,
    scan_seq: AtomicU32,
    /// Scan history, opened on first use (`None` inside = not opened yet).
    history: Mutex<Option<History>>,
    /// Folder Encyclopedia with this machine's paths expanded (built on first use).
    encyclopedia: std::sync::OnceLock<nfd_rules::encyclopedia::Matcher>,
    /// Folder given on the command line, handed to the UI once.
    startup: Mutex<Option<String>>,
    /// Folder given with `--dupes` ("Find duplicates here"), handed to the UI once.
    startup_dupes: Mutex<Option<String>>,
}

impl AppState {
    pub fn set_startup_path(&self, path: Option<String>) {
        if let Ok(mut slot) = self.startup.lock() {
            *slot = path;
        }
    }

    pub fn set_startup_dupes(&self, path: Option<String>) {
        if let Ok(mut slot) = self.startup_dupes.lock() {
            *slot = path;
        }
    }
}

/// The folder the app was started with (A7), once; `None` afterwards.
#[tauri::command]
pub fn take_startup_path(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    Ok(state.startup.lock().map_err(|e| err("internal", e))?.take())
}

/// The folder the app was started with through "Find duplicates here", once.
#[tauri::command]
pub fn take_startup_dupes(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    Ok(state
        .startup_dupes
        .lock()
        .map_err(|e| err("internal", e))?
        .take())
}

/// Whether the Explorer menu points at this executable.
#[tauri::command]
pub fn shell_menu_status() -> CmdResult<bool> {
    let exe = std::env::current_exe().map_err(|e| err("os", e))?;
    Ok(nfd_platform::shell_menu::registered_command()
        .is_some_and(|c| c.eq_ignore_ascii_case(&nfd_platform::shell_menu::command_line(&exe))))
}

/// Add or remove "Analyze with NANOFreeDisk" and "Find duplicates here" in
/// Explorer's right-click menu (per user, `HKCU` only). The captions are the menu
/// texts in the UI language.
#[tauri::command]
pub fn shell_menu_set(
    enabled: bool,
    caption: String,
    dupes_caption: Option<String>,
) -> CmdResult<bool> {
    if enabled {
        let exe = std::env::current_exe().map_err(|e| err("os", e))?;
        let pick = |text: &str, default: &'static str| -> String {
            let text = text.trim();
            if text.is_empty() { default } else { text }.to_owned()
        };
        let caption = pick(&caption, "Analyze with NANOFreeDisk");
        let dupes = pick(
            dupes_caption.as_deref().unwrap_or_default(),
            "Find duplicates here",
        );
        nfd_platform::shell_menu::register(&exe, &caption, &dupes).map_err(|e| err("os", e))?;
    } else {
        nfd_platform::shell_menu::unregister().map_err(|e| err("os", e))?;
    }
    shell_menu_status()
}

/// Error sent to the UI.
#[derive(Debug, Serialize)]
pub struct CmdError {
    pub code: &'static str,
    pub message: String,
}

pub(crate) type CmdResult<T> = Result<T, CmdError>;

pub(crate) fn err(code: &'static str, message: impl ToString) -> CmdError {
    CmdError {
        code,
        message: message.to_string(),
    }
}

fn no_session() -> CmdError {
    err("no-session", "no scan loaded")
}

fn with_session<T>(state: &State<'_, AppState>, f: impl FnOnce(&Session) -> T) -> CmdResult<T> {
    let guard = state.session.read().map_err(|e| err("internal", e))?;
    guard.as_ref().map(f).ok_or_else(no_session)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    os: &'static str,
    accent: Option<String>,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        accent: nfd_platform::appearance::accent_color(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeDto {
    root: String,
    label: String,
    fs: String,
    kind: &'static str,
    total: u64,
    available: u64,
    free: u64,
    ready: bool,
    system: bool,
}

#[tauri::command]
pub async fn list_volumes() -> Vec<VolumeDto> {
    volumes::list()
        .into_iter()
        .map(|v| VolumeDto {
            root: v.root.to_string_lossy().into_owned(),
            label: v.label,
            fs: v.fs,
            kind: match v.kind {
                VolumeKind::Fixed => "fixed",
                VolumeKind::Removable => "removable",
                VolumeKind::Network => "network",
                VolumeKind::Optical => "optical",
                VolumeKind::RamDisk => "ramdisk",
                VolumeKind::Unknown => "unknown",
            },
            total: v.total,
            available: v.available,
            free: v.free,
            ready: v.ready,
            system: v.system,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    scan: u32,
    files: u64,
    dirs: u64,
    on_disk: u64,
    errors: u64,
    elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Finished {
    scan: u32,
    ok: bool,
    cancelled: bool,
    error: Option<String>,
    /// Id of the history snapshot written for this scan.
    snapshot: Option<i64>,
    /// Why the snapshot could not be written (the scan itself is fine).
    history_error: Option<String>,
}

/// Start scanning `path` in the background. Progress arrives as `scan-progress`
/// events, the end as `scan-done`; a running scan is cancelled first. With
/// `record` (default) a snapshot goes into the scan history before `scan-done`.
#[tauri::command]
pub fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    record: Option<bool>,
) -> CmdResult<u32> {
    let record = record.unwrap_or(true);
    let root = PathBuf::from(path.trim());
    if !root.is_absolute() {
        return Err(err("bad-path", "path must be absolute"));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = state.scan.lock().map_err(|e| err("internal", e))?;
        if let Some(previous) = running.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }
    let id = state.scan_seq.fetch_add(1, Ordering::Relaxed) + 1;
    let options = ScanOptions {
        cancel: Some(cancel.clone()),
        progress_interval: Duration::from_millis(120),
        ..ScanOptions::default()
    };

    std::thread::Builder::new()
        .name(format!("scan-{id}"))
        .spawn(move || {
            let (tx, rx) = mpsc::channel::<ScanEvent>();
            let progress_app = app.clone();
            let forward = std::thread::spawn(move || {
                let started = std::time::Instant::now();
                for event in rx {
                    if let ScanEvent::Progress(stats) = event {
                        emit_progress(&progress_app, id, &stats, started.elapsed());
                    }
                }
            });
            let result = scanner::scan(&root, &options, Some(&tx));
            drop(tx);
            let _ = forward.join();

            let state = app.state::<AppState>();
            let done = match result {
                Ok(tree) => {
                    let vol = volume_for(&tree.root);
                    let (snapshot, history_error) = if record {
                        match record_snapshot(&app, &state, &tree, &vol) {
                            Ok(id) => (Some(id), None),
                            Err(e) => (None, Some(e.message)),
                        }
                    } else {
                        (None, None)
                    };
                    let session = Session::new(tree, vol.1, vol.2);
                    if let Ok(mut slot) = state.session.write() {
                        *slot = Some(session);
                    }
                    Finished {
                        scan: id,
                        ok: true,
                        cancelled: false,
                        error: None,
                        snapshot,
                        history_error,
                    }
                }
                Err(e) => Finished {
                    scan: id,
                    ok: false,
                    cancelled: matches!(e, scanner::ScanError::Cancelled),
                    error: Some(format!("{e}: {}", source_text(&e))),
                    snapshot: None,
                    history_error: None,
                },
            };
            if let Ok(mut running) = state.scan.lock() {
                if running.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cancel)) {
                    *running = None;
                }
            }
            let _ = app.emit("scan-done", done);
        })
        .map_err(|e| err("internal", e))?;
    Ok(id)
}

fn source_text(e: &scanner::ScanError) -> String {
    std::error::Error::source(e).map_or_else(String::new, ToString::to_string)
}

fn emit_progress(app: &AppHandle, id: u32, stats: &ScanStats, elapsed: Duration) {
    let _ = app.emit(
        "scan-progress",
        Progress {
            scan: id,
            files: stats.files,
            dirs: stats.dirs,
            on_disk: stats.on_disk,
            errors: stats.errors,
            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        },
    );
}

/// (root, total, free) of the volume holding `path`; empty root and zeros when
/// unknown.
fn volume_for(path: &Path) -> (String, u64, u64) {
    let p = path.to_string_lossy().to_lowercase();
    volumes::list()
        .into_iter()
        .filter(|v| p.starts_with(&v.root.to_string_lossy().to_lowercase()))
        .max_by_key(|v| v.root.as_os_str().len())
        .map_or((String::new(), 0, 0), |v| {
            (v.root.to_string_lossy().into_owned(), v.total, v.free)
        })
}

fn history_path(app: &AppHandle) -> CmdResult<PathBuf> {
    crate::paths::local_data(app)
        .map(|d| d.join("history.db"))
        .ok_or_else(|| err("os", "no data folder"))
}

/// Run `f` on the history database, opening it on first use.
fn with_history<T>(
    app: &AppHandle,
    state: &AppState,
    f: impl FnOnce(&mut History) -> nfd_history::Result<T>,
) -> CmdResult<T> {
    let mut slot = state.history.lock().map_err(|e| err("internal", e))?;
    if slot.is_none() {
        *slot = Some(History::open(&history_path(app)?).map_err(|e| err("history", e))?);
    }
    let history = slot.as_mut().ok_or_else(|| err("internal", "history"))?;
    f(history).map_err(|e| match e {
        nfd_history::HistoryError::NotFound(id) => err("not-found", id),
        other => err("history", other),
    })
}

fn record_snapshot(
    app: &AppHandle,
    state: &AppState,
    tree: &scanner::ScanTree,
    vol: &(String, u64, u64),
) -> CmdResult<i64> {
    let volume = VolumeState {
        total: vol.1,
        free: vol.2,
    };
    with_history(app, state, |h| {
        h.record(tree, &vol.0, volume, &RecordOptions::default())
    })
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(c) = state.scan.lock().map_err(|e| err("internal", e))?.as_ref() {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    root: String,
    root_id: u32,
    stats: ScanStats,
    elapsed_ms: u64,
    scanned_at: i64,
    issues: usize,
    mode: scanner::ScanMode,
    volume_total: u64,
    volume_free: u64,
}

#[tauri::command]
pub fn summary(state: State<'_, AppState>) -> CmdResult<Summary> {
    with_session(&state, |s| Summary {
        root: s.tree.root.to_string_lossy().into_owned(),
        root_id: 0,
        stats: s.tree.stats,
        elapsed_ms: u64::try_from(s.tree.elapsed.as_millis()).unwrap_or(u64::MAX),
        scanned_at: s.tree.started_at,
        issues: s.tree.issues.len(),
        mode: s.tree.mode,
        volume_total: s.volume_total,
        volume_free: s.volume_free,
    })
}

#[tauri::command]
pub fn issues(state: State<'_, AppState>, limit: usize) -> CmdResult<Vec<scanner::ScanIssue>> {
    with_session(&state, |s| {
        s.tree.issues.iter().take(limit).cloned().collect()
    })
}

#[tauri::command]
pub fn children(state: State<'_, AppState>, id: u32) -> CmdResult<Vec<Node>> {
    with_session(&state, |s| s.children_of(id))
}

#[tauri::command]
pub fn detail(state: State<'_, AppState>, id: u32) -> CmdResult<Detail> {
    with_session(&state, |s| s.detail(id))?.ok_or_else(|| err("not-found", id))
}

#[tauri::command]
pub fn slice(
    state: State<'_, AppState>,
    id: u32,
    max_depth: u8,
    min_bytes: u64,
    max_nodes: usize,
) -> CmdResult<Slice> {
    with_session(&state, |s| {
        s.slice(id, max_depth, min_bytes, max_nodes.min(200_000))
    })
}

#[tauri::command]
pub fn top_files(state: State<'_, AppState>, id: u32, limit: usize) -> CmdResult<Vec<Located>> {
    with_session(&state, |s| s.top_files(id, limit.min(10_000)))
}

#[tauri::command]
pub fn extensions(state: State<'_, AppState>, id: u32, limit: usize) -> CmdResult<Vec<ExtRow>> {
    with_session(&state, |s| s.extensions(id, limit))
}

#[tauri::command]
pub fn search(
    state: State<'_, AppState>,
    id: u32,
    filter: Filter,
    limit: usize,
) -> CmdResult<SearchRows> {
    with_session(&state, |s| s.search(id, &filter, limit.min(10_000)))
}

/// Old & Large lists (A2) below `id`.
#[tauri::command]
pub fn old_and_large(
    state: State<'_, AppState>,
    id: u32,
    options: OldLargeOptions,
) -> CmdResult<OldLargeRows> {
    let options = OldLargeOptions {
        limit: options.limit.min(2_000),
        ..options
    };
    with_session(&state, |s| s.old_and_large(id, &options))?.ok_or_else(|| err("not-found", id))
}

/// Outermost empty folders (A3) below `id`; read-only.
#[tauri::command]
pub fn empty_folders(state: State<'_, AppState>, id: u32, limit: usize) -> CmdResult<EmptyRows> {
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    with_session(&state, |s| s.empty_folders(id, limit.min(10_000), &resolve))?
        .ok_or_else(|| err("not-found", id))
}

/// Shortcuts below `id` whose target is gone (A3); read-only. Reads each `.lnk`
/// file (never through the shell, never touching network targets).
#[tauri::command]
pub async fn broken_shortcuts(
    state: State<'_, AppState>,
    id: u32,
    limit: usize,
) -> CmdResult<ShortcutRows> {
    let files = with_session(&state, |s| s.shortcut_files(id))?;
    let read = |p: &Path| -> Option<Vec<u8>> {
        let meta = std::fs::metadata(p).ok()?;
        if meta.len() > nfd_core::shortcut::MAX_LINK_SIZE {
            return None;
        }
        std::fs::read(p).ok()
    };
    // Drive letters that must not be probed: a mapped share can hang for minutes
    // when its server is gone, an empty optical drive spins up.
    let remote: Vec<String> = volumes::list()
        .into_iter()
        .filter(|v| matches!(v.kind, VolumeKind::Network | VolumeKind::Optical))
        .map(|v| v.root.to_string_lossy().to_lowercase())
        .collect();
    let probe = |p: &Path| {
        let s = p.to_string_lossy().to_lowercase();
        if remote.iter().any(|r| s.starts_with(r.as_str())) {
            return None;
        }
        // an error (access denied…) is not proof the target is gone
        Some(p.try_exists().unwrap_or(true))
    };
    let env = |k: &str| std::env::var(k).ok();
    let checks = session::check_shortcuts(&files, &read, &probe, &env);
    with_session(&state, |s| s.shortcut_rows(&checks, limit.min(10_000)))
}

/// Snapshots of `root` in the scan history, newest first.
#[tauri::command]
pub async fn history_snapshots(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
) -> CmdResult<Vec<Snapshot>> {
    with_history(&app, &state, |h| h.snapshots(&root))
}

/// "What changed?" between two snapshots (A1).
#[tauri::command]
pub async fn history_diff(
    app: AppHandle,
    state: State<'_, AppState>,
    older: i64,
    newer: i64,
    min_delta: u64,
    limit: usize,
) -> CmdResult<Diff> {
    let options = DiffOptions {
        min_delta,
        limit: limit.min(1_000),
    };
    with_history(&app, &state, |h| h.diff(older, newer, &options))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryInfo {
    snapshots: u64,
    path: String,
}

#[tauri::command]
pub async fn history_info(app: AppHandle, state: State<'_, AppState>) -> CmdResult<HistoryInfo> {
    let path = history_path(&app)?.to_string_lossy().into_owned();
    let snapshots = with_history(&app, &state, |h| h.count())?;
    Ok(HistoryInfo { snapshots, path })
}

/// Delete every stored snapshot (privacy; the current scan stays loaded).
#[tauri::command]
pub async fn history_clear(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    with_history(&app, &state, History::clear)
}

/// Forget every snapshot of one folder.
#[tauri::command]
pub async fn history_forget(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
) -> CmdResult<usize> {
    with_history(&app, &state, |h| h.forget(&root))
}

/// A Folder Encyclopedia article in one language (A5).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleDto {
    id: String,
    risk: nfd_core::RiskLevel,
    title: String,
    body: String,
    /// The entry is inside the folder the article is about.
    inside: bool,
    /// The folder the article is about.
    path: String,
    rules: Vec<String>,
}

/// "What is this?" for an entry of the loaded scan: the article for it or its
/// nearest described ancestor, in `lang` (falls back to English). Read-only.
#[tauri::command]
pub fn explain(state: State<'_, AppState>, id: u32, lang: String) -> CmdResult<Option<ArticleDto>> {
    let path = with_session(&state, |s| s.path(id))?.ok_or_else(|| err("not-found", id))?;
    let matcher = state.encyclopedia.get_or_init(|| {
        let platform = nfd_platform::Native;
        nfd_rules::encyclopedia::Matcher::new(
            nfd_rules::encyclopedia::builtin(),
            &platform.resolver(),
        )
    });
    let Some(hit) = matcher.lookup(&path, &|p| p.exists()) else {
        return Ok(None);
    };
    let a = hit.article;
    let text = a.text.get(&lang).or_else(|| a.text.get("en"));
    let matched = path
        .ancestors()
        .nth(hit.depth_up)
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    Ok(text.map(|t| ArticleDto {
        id: a.id.clone(),
        risk: a.risk,
        title: t.title.clone(),
        body: t.body.clone(),
        inside: hit.inside,
        path: matched,
        rules: a.rules.clone(),
    }))
}

/// Bytes by last-modified age below `id`, per child (A9 heatmap).
#[tauri::command]
pub fn age_matrix(state: State<'_, AppState>, id: u32, limit: usize) -> CmdResult<AgeDto> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    with_session(&state, |s| s.age(id, now, limit.min(100)))?.ok_or_else(|| err("not-found", id))
}

/// Owner account of an entry (A9), read from its security descriptor.
#[tauri::command]
pub async fn owner(state: State<'_, AppState>, id: u32) -> CmdResult<Option<String>> {
    let path = with_session(&state, |s| s.path(id))?.ok_or_else(|| err("not-found", id))?;
    Ok(nfd_platform::owner::owner_name(&path))
}

/// Junctions and symbolic links below `id` whose target is gone (A3); read-only.
#[tauri::command]
pub async fn broken_links(
    state: State<'_, AppState>,
    id: u32,
    limit: usize,
) -> CmdResult<ShortcutRows> {
    let links = with_session(&state, |s| s.link_entries(id))?;
    let remote: Vec<String> = volumes::list()
        .into_iter()
        .filter(|v| matches!(v.kind, VolumeKind::Network | VolumeKind::Optical))
        .map(|v| v.root.to_string_lossy().to_lowercase())
        .collect();
    let probe = |p: &Path| {
        let s = p.to_string_lossy().to_lowercase();
        if remote.iter().any(|r| s.starts_with(r.as_str())) {
            return None;
        }
        Some(p.try_exists().unwrap_or(true))
    };
    let checks = session::check_links(&links, &|p| std::fs::read_link(p).ok(), &probe);
    with_session(&state, |s| s.shortcut_rows(&checks, limit.min(10_000)))
}

/// Id of the entry at `path` (relative to the scanned folder) in the loaded scan.
#[tauri::command]
pub fn find_path(state: State<'_, AppState>, path: String) -> CmdResult<Option<u32>> {
    with_session(&state, |s| s.find(&path))
}

/// Health Score (F1) from the fixed drives, their free-space trend in the scan
/// history and the Old & Large total of the loaded scan.
#[tauri::command]
pub async fn health(app: AppHandle, state: State<'_, AppState>) -> CmdResult<HealthScore> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let mut facts = Vec::new();
    for v in volumes::list()
        .into_iter()
        .filter(|v| v.ready && v.kind == VolumeKind::Fixed && v.total > 0)
    {
        let root = v.root.to_string_lossy().into_owned();
        // history problems only cost the trend, never the score
        let trend = with_history(&app, &state, |h| {
            h.forecast(&root, now, v.free, v.total, 90)
        })
        .ok()
        .flatten();
        facts.push(VolumeFacts {
            root,
            total: v.total,
            free: v.free,
            system: v.system,
            days_to_full: trend.as_ref().and_then(|t| t.days_to_full),
            has_trend: trend.is_some(),
        });
    }
    let scan = {
        let guard = state.session.read().map_err(|e| err("internal", e))?;
        guard.as_ref().map(|s| {
            let reclaimable = s
                .old_and_large(0, &OldLargeOptions::at(now))
                .map_or(0, |r| r.distinct_on_disk);
            ScanFacts {
                root: s.tree.root.to_string_lossy().into_owned(),
                volume_total: s.volume_total,
                reclaimable,
            }
        })
    };
    let junk = app
        .state::<clean::CleanState>()
        .safe_bytes()
        .map(|bytes| JunkFacts {
            bytes,
            volume_total: facts.iter().find(|v| v.system).map_or(0, |v| v.total),
        });
    let mut score = health::score_with(&facts, scan.as_ref(), junk.as_ref());
    hwmon::add_heat_findings(&app, &mut score);
    diskhealth::add_findings(&app, &mut score);
    // AI leftovers of finished sessions (the last scan of the DevCache tab)
    if let Some(bytes) = app.state::<ai_leftovers::AiState>().selected_bytes() {
        health::add_ai_leftovers(&mut score, bytes);
    }
    // information only: what the last duplicate search found
    if let Some((bytes, groups, root)) = app.state::<dupes::DupeState>().summary() {
        if bytes > 0 {
            score.findings.push(health::Finding {
                kind: "duplicates",
                severity: health::Severity::Info,
                penalty: 0,
                subject: root,
                value: groups as f64,
                bytes,
                fix: "duplicates",
            });
        }
    }
    Ok(score)
}

#[tauri::command]
pub fn reveal(app: AppHandle, state: State<'_, AppState>, id: u32) -> CmdResult<()> {
    let path = with_session(&state, |s| s.path(id))?.ok_or_else(|| err("not-found", id))?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| err("os", e))
}

#[tauri::command]
pub fn open_item(app: AppHandle, state: State<'_, AppState>, id: u32) -> CmdResult<()> {
    let path = with_session(&state, |s| s.path(id))?.ok_or_else(|| err("not-found", id))?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| err("os", e))
}

#[tauri::command]
pub fn plan_delete(state: State<'_, AppState>, ids: Vec<u32>) -> CmdResult<DeletePlan> {
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    with_session(&state, |s| s.plan_delete(&ids, &resolve))
}

/// Move the given entries to the Recycle Bin (Planner → SafetyGate → Executor, with
/// an entry in the Activity log). Kept for the Phase 2 UI and scripts; same as
/// `delete_entries` with the default mode.
#[tauri::command]
pub async fn recycle(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    ids: Vec<u32>,
) -> CmdResult<Vec<clean::DeleteOutcome>> {
    clean::delete_entries_impl(
        &app,
        &window,
        &state,
        &ids,
        nfd_core::plan::DeleteMode::RecycleBin,
        "manual",
    )
    .await
}

#[cfg(windows)]
fn owner_handle(window: &WebviewWindow) -> isize {
    window.hwnd().map_or(0, |h| h.0 as isize)
}

#[cfg(not(windows))]
fn owner_handle(_window: &WebviewWindow) -> isize {
    0
}

/// Write the current scan as `nanofreedisk.scan.v1` JSON or CSV.
#[tauri::command]
pub async fn export_report(
    state: State<'_, AppState>,
    path: String,
    format: String,
) -> CmdResult<()> {
    let path = PathBuf::from(path);
    let bytes = with_session(&state, |s| -> Result<Vec<u8>, String> {
        match format.as_str() {
            "json" => serde_json::to_vec_pretty(&scanner::Report::new(&s.tree, 100, 0))
                .map_err(|e| e.to_string()),
            "csv" => {
                let mut buf = Vec::new();
                scanner::report::write_csv(&s.tree, 0, &mut buf).map_err(|e| e.to_string())?;
                Ok(buf)
            }
            other => Err(format!("unknown format {other}")),
        }
    })?
    .map_err(|e| err("export", e))?;
    std::fs::write(&path, bytes).map_err(|e| err("os", e))
}

/// Write an HTML report built by the UI to a path the user picked in a save dialog.
#[tauri::command]
pub async fn save_html_report(path: String, html: String) -> CmdResult<()> {
    let p = Path::new(&path);
    let ok = p.is_absolute()
        && p.extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("html") || x.eq_ignore_ascii_case("htm"));
    if !ok {
        return Err(err("bad-path", "only absolute *.html report files"));
    }
    std::fs::write(p, html).map_err(|e| err("os", e))
}

fn settings_path(app: &AppHandle) -> CmdResult<PathBuf> {
    crate::paths::config(app)
        .map(|d| d.join("settings.json"))
        .ok_or_else(|| err("os", "no settings folder"))
}

/// The UI's settings document (opaque JSON owned by the frontend), `null` if none.
#[tauri::command]
pub fn load_settings(app: AppHandle) -> CmdResult<Option<serde_json::Value>> {
    let path = settings_path(&app)?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(err("os", e)),
    }
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: serde_json::Value) -> CmdResult<()> {
    let path = settings_path(&app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| err("os", e))?;
    }
    let bytes = serde_json::to_vec_pretty(&settings).map_err(|e| err("internal", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| err("os", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| err("os", e))
}

/// Theme files are the only files the UI reads or writes directly.
fn theme_file(path: &str) -> CmdResult<&Path> {
    let p = Path::new(path);
    let ok = p.is_absolute()
        && p.extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("toml"));
    if ok {
        Ok(p)
    } else {
        Err(err("bad-path", "only absolute *.toml theme files"))
    }
}

/// Read a theme file the user picked (import).
#[tauri::command]
pub async fn read_text_file(path: String) -> CmdResult<String> {
    let path = theme_file(&path)?;
    let meta = std::fs::metadata(path).map_err(|e| err("os", e))?;
    if meta.len() > 256 * 1024 {
        return Err(err("too-large", "file larger than 256 KiB"));
    }
    std::fs::read_to_string(path).map_err(|e| err("os", e))
}

/// Write a theme file to a path the user picked in a save dialog (export).
#[tauri::command]
pub async fn write_text_file(path: String, contents: String) -> CmdResult<()> {
    std::fs::write(theme_file(&path)?, contents).map_err(|e| err("os", e))
}
