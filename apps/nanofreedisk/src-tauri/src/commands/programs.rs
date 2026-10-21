//! Programs view (PLAN §13.2 E1, `docs/UNINSTALLER.md`): installed programs, the
//! vendors' uninstallers, the leftovers they leave behind and software junk.
//!
//! Nothing is uninstalled without the user starting it: the vendor's uninstaller
//! runs in its own window. Leftovers are only scanned once the program is gone
//! (or its uninstaller is), and removed the Phase 3 way — this user's files
//! through `Planner → SafetyGate → Executor` into the quarantine, registry keys
//! after a `.reg` backup, all-users items in one UAC-elevated run of this
//! executable (`--elevated-leftovers`) that checks everything again. One
//! operation in the Activity log (kind `uninstall`) holds all of it; items only
//! an administrator can put back are restored through the same elevated run.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nfd_apps::inventory::Program;
use nfd_apps::junk::{self, JunkCtx, JunkReport, JunkRoots};
use nfd_apps::leftovers::{self, Ctx, Mode, Others, Report, Roots, Snapshot};
use nfd_apps::{Source, elevated, remove, runner};
use nfd_core::exec::{self, ExecOptions, ItemStatus, Operation, RestoreStatus};
use nfd_core::plan::DeleteMode;
use nfd_core::safety::{LiveProbe, SafetyGate};
use nfd_history::RestoreResult;
use nfd_platform::Platform;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use super::clean::{self, OperationDto};
use super::{AppState, CmdResult, err, owner_handle, with_history};

/// Entries visited per folder when sizing leftovers and junk.
const MAX_ENTRIES: u64 = 2_000_000;
/// Entries visited per install folder when a program registered no size.
const SIZE_ENTRIES: u64 = 200_000;
/// The longest an uninstaller is waited for.
const WAIT_LIMIT: Duration = Duration::from_secs(4 * 3600);

/// The last listing, the snapshots taken before uninstalling, the last leftover
/// and junk scans, and the stop switch of a running wait.
#[derive(Debug, Default)]
pub struct ProgramsState {
    list: Mutex<Option<Vec<Program>>>,
    snapshots: Mutex<HashMap<String, Snapshot>>,
    leftovers: Mutex<HashMap<String, Report>>,
    junk: Mutex<Option<JunkReport>>,
    stop: Mutex<Option<Arc<AtomicBool>>>,
    /// Icons already read (`None`: the program has none).
    icons: Mutex<HashMap<String, Option<IconDto>>>,
}

fn lock<T>(m: &Mutex<T>) -> CmdResult<std::sync::MutexGuard<'_, T>> {
    m.lock().map_err(|e| err("internal", e))
}

fn resolver() -> impl Fn(&str) -> Option<String> {
    move |v| nfd_platform::Native.resolver()(v)
}

fn roots() -> Roots {
    Roots::detect(&resolver(), &|k| std::env::var(k).ok())
}

fn gate() -> SafetyGate {
    SafetyGate::new(&resolver())
}

/// This app's folder (never listed as removable).
fn own_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
}

fn find(programs: &ProgramsState, id: &str) -> CmdResult<Program> {
    lock(&programs.list)?
        .as_ref()
        .and_then(|l| l.iter().find(|p| p.id == id).cloned())
        .ok_or_else(|| err("not-found", id))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| err("internal", e))
}

/// The installed programs (registry entries and, with `store`, Store packages).
#[tauri::command]
pub async fn programs_list(
    programs: State<'_, ProgramsState>,
    store: bool,
) -> CmdResult<Vec<Program>> {
    let list = blocking(move || runner::list_programs(own_dir(), store, SIZE_ENTRIES)).await?;
    *lock(&programs.list)? = Some(list.clone());
    Ok(list)
}

/// A program's icon for the list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconDto {
    /// `data:image/png;base64,…`
    src: String,
    /// Show it on a colored plate (Store logos made for one).
    plate: bool,
}

/// The icons of programs `ids` of the last listing (programs without one are
/// left out). Read once per program and kept for the session.
#[tauri::command]
pub async fn programs_icons(
    programs: State<'_, ProgramsState>,
    ids: Vec<String>,
) -> CmdResult<HashMap<String, IconDto>> {
    let mut out = HashMap::new();
    let todo: Vec<Program> = {
        let cache = lock(&programs.icons)?;
        let list = lock(&programs.list)?;
        let mut todo = Vec::new();
        for id in &ids {
            match cache.get(id) {
                Some(Some(icon)) => {
                    out.insert(id.clone(), icon.clone());
                }
                Some(None) => {}
                None => todo.extend(list.iter().flatten().find(|p| &p.id == id).cloned()),
            }
        }
        todo
    };
    if todo.is_empty() {
        return Ok(out);
    }
    let read = blocking(move || {
        let found = nfd_apps::icons::program_icons(&todo);
        (todo, found)
    })
    .await?;
    let (asked, found) = read;
    let mut cache = lock(&programs.icons)?;
    for p in &asked {
        cache.insert(p.id.clone(), None);
    }
    for (id, icon) in found {
        let dto = IconDto {
            src: format!("data:image/png;base64,{}", super::dupes::base64(&icon.png)),
            plate: icon.plate,
        };
        cache.insert(id.clone(), Some(dto.clone()));
        out.insert(id, dto);
    }
    Ok(out)
}

/// How an uninstall ended.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallOutcome {
    id: String,
    /// `removed`, `still-installed`, `declined` (UAC), `failed`, `broken` (the
    /// uninstaller is gone: only the leftovers can be removed).
    status: &'static str,
    /// `done`, `stopped` (the user said it is finished), `timed-out`.
    waited: Option<&'static str>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UninstallProgress {
    id: String,
    /// `running`, `waiting`.
    phase: &'static str,
    /// Processes of the uninstaller still running.
    alive: usize,
}

/// Is `program` still installed (its entry listed and its uninstaller on disk)?
fn still_installed(program: &Program) -> bool {
    if let Some(pkg) = &program.package {
        return runner::store_rows()
            .iter()
            .any(|r| r.family.eq_ignore_ascii_case(&pkg.family));
    }
    let Some((hive, key)) = program.reg_key() else {
        return false;
    };
    nfd_platform::registry::exists(hive, key)
        && program
            .uninstall
            .as_ref()
            .is_none_or(|c| c.file_name() == "msiexec.exe" || c.exe.exists())
}

/// Run the vendor's uninstaller of program `id` and wait for it (and every
/// process it starts) to finish; `quiet` uses the vendor's silent command when it
/// registered one. A snapshot of the program is taken first for the leftover scan.
/// Progress streams as `uninstall-progress`; [`programs_stop_waiting`] ends the wait.
#[tauri::command]
pub async fn programs_uninstall(
    app: AppHandle,
    window: WebviewWindow,
    programs: State<'_, ProgramsState>,
    id: String,
    quiet: bool,
) -> CmdResult<UninstallOutcome> {
    let program = find(&programs, &id)?;
    if let Some(b) = &program.blocked {
        return Err(err("blocked", b));
    }
    let snap = leftovers::snapshot(&program, &roots(), &runner::system_root(), exec::unix_now());
    lock(&programs.snapshots)?.insert(id.clone(), snap);
    lock(&programs.leftovers)?.remove(&id);
    if program.broken {
        return Ok(UninstallOutcome {
            id,
            status: "broken",
            waited: None,
            message: None,
        });
    }
    let stop = Arc::new(AtomicBool::new(false));
    *lock(&programs.stop)? = Some(stop.clone());
    let owner = owner_handle(&window);
    let emitter = app.clone();
    let p = program.clone();
    let id2 = id.clone();
    let run = blocking(move || -> std::io::Result<runner::Waited> {
        let emit = |phase: &'static str, alive: usize| {
            let _ = emitter.emit(
                "uninstall-progress",
                UninstallProgress {
                    id: id2.clone(),
                    phase,
                    alive,
                },
            );
        };
        emit("running", 1);
        if let Some(pkg) = &p.package {
            runner::remove_store_package(&pkg.full_name)?;
            return Ok(runner::Waited::Done);
        }
        let quiet_cmd = quiet
            .then_some(p.quiet_uninstall.as_deref())
            .flatten()
            .and_then(|q| {
                let env = |k: &str| std::env::var(k).ok();
                nfd_apps::command::parse(q, &env, &|x: &Path| x.exists(), &runner::system_root())
            });
        let cmd = quiet_cmd
            .or_else(|| p.uninstall.clone())
            .ok_or_else(|| std::io::Error::other("no uninstaller"))?;
        let started = runner::start_uninstaller(&cmd, owner)?;
        Ok(runner::wait_tree(
            started.pid,
            started.launched,
            &runner::process_snapshot,
            &stop,
            Duration::from_millis(500),
            WAIT_LIMIT,
            &mut |alive| emit("waiting", alive),
        ))
    })
    .await?;
    *lock(&programs.stop)? = None;
    let (status, waited, message) = match run {
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => ("declined", None, None),
        Err(e) => ("failed", None, Some(e.to_string())),
        Ok(w) => {
            let waited = match w {
                runner::Waited::Done => "done",
                runner::Waited::Stopped => "stopped",
                runner::Waited::TimedOut => "timed-out",
            };
            let p = program.clone();
            let installed = blocking(move || still_installed(&p)).await?;
            (
                if installed {
                    "still-installed"
                } else {
                    "removed"
                },
                Some(waited),
                None,
            )
        }
    };
    if status == "removed" {
        // the listing is stale now
        if let Some(list) = lock(&programs.list)?.as_mut() {
            list.retain(|p| p.id != id);
        }
    }
    Ok(UninstallOutcome {
        id,
        status,
        waited,
        message,
    })
}

/// End the wait for an uninstaller ("It is finished — look for leftovers").
#[tauri::command]
pub fn programs_stop_waiting(programs: State<'_, ProgramsState>) -> CmdResult<()> {
    if let Some(s) = lock(&programs.stop)?.as_ref() {
        s.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Look for what program `id` left behind. Normally after
/// [`programs_uninstall`]; for a broken entry (uninstaller gone) at any time;
/// `forced` (Expert) scans a program that still looks installed — nothing is then
/// pre-selected and its entry is never offered.
#[tauri::command]
pub async fn programs_leftovers(
    programs: State<'_, ProgramsState>,
    id: String,
    forced: bool,
) -> CmdResult<Report> {
    let snap = match lock(&programs.snapshots)?.get(&id).cloned() {
        Some(s) => s,
        None => {
            let p = find(&programs, &id)?;
            leftovers::snapshot(&p, &roots(), &runner::system_root(), exec::unix_now())
        }
    };
    let report = blocking(move || -> CmdResult<Report> {
        let list = runner::registry_list(own_dir());
        let p = &snap.program;
        let mode = if forced {
            Mode::Forced
        } else if p.broken {
            Mode::Broken
        } else if p.source != Source::Store && still_installed(p) {
            return Err(err("still-installed", &p.name));
        } else {
            Mode::AfterUninstall
        };
        let roots = roots();
        let system_root = runner::system_root();
        let others = Others::new(&list, &p.id, &roots, &system_root);
        let running = runner::running_images();
        let resolve = resolver();
        let env = move |k: &str| std::env::var(k).ok().or_else(|| resolve(&k.to_uppercase()));
        let gate = gate();
        let ctx = Ctx {
            reg: &nfd_apps::registry::LiveRegistry,
            roots: &roots,
            gate: &gate,
            others: &others,
            running: &running,
            max_entries: MAX_ENTRIES,
            large: leftovers::LARGE_LEFTOVER,
            exists: &|x: &Path| x.exists(),
            env: &env,
            system_root,
        };
        Ok(leftovers::scan(&snap, &ctx, mode))
    })
    .await??;
    lock(&programs.leftovers)?.insert(id, report.clone());
    Ok(report)
}

/// Remove the `picked` leftovers (ids from the last [`programs_leftovers`] of
/// program `id`): this user's through the quarantine and `.reg` backups, the
/// all-users ones in one elevated run. One Activity operation (kind `uninstall`).
#[tauri::command]
pub async fn programs_leftovers_remove(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    programs: State<'_, ProgramsState>,
    id: String,
    picked: Vec<String>,
) -> CmdResult<OperationDto> {
    let report = lock(&programs.leftovers)?
        .get(&id)
        .cloned()
        .ok_or_else(|| err("no-leftover-scan", &id))?;
    let local = clean::app_local(&app).ok_or_else(|| err("os", "no data folder"))?;
    let now = exec::unix_now();
    let op_id = exec::new_op_id(now);
    let prepared = remove::prepare(
        &report,
        &picked,
        &local.join("registry-backup").join(&op_id),
        &op_id,
    );
    if prepared.is_empty() {
        return Err(err("nothing-to-clean", "nothing to remove"));
    }
    let opts = ExecOptions {
        op_id: op_id.clone(),
        kind: "uninstall".into(),
        owner: owner_handle(&window),
        roots: clean::roots(&app),
        now,
    };
    let emitter = app.clone();
    let elevated_dir = local.join("elevated");
    let op = blocking(move || {
        let gate = gate();
        let total = prepared.plan.items.len()
            + prepared.registry.len()
            + prepared.machine.files.len()
            + prepared.machine.registry.len();
        let mut freed = 0;
        let mut op = exec::execute(
            &prepared.plan,
            &gate,
            &LiveProbe,
            &opts,
            &mut |done, _, r| {
                freed += r.freed;
                let _ = emitter.emit(
                "clean-progress",
                serde_json::json!({ "done": done, "total": total, "freed": freed, "path": r.path }),
            );
                true
            },
        );
        let first = op.items.len();
        op.items
            .extend(remove::run_registry(&prepared.registry, first, false));
        let m = &prepared.machine;
        if !m.is_empty() {
            let base = op.items.len();
            match std::env::current_exe()
                .and_then(|exe| runner::run_elevated_step(&exe, m, &elevated_dir, opts.owner))
            {
                Ok(resp) if resp.error.is_none() => {
                    for mut item in resp.items {
                        item.n += base;
                        op.items.push(item);
                    }
                }
                outcome => {
                    let (reason, message) = match outcome {
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                            ("uac-declined", None)
                        }
                        Err(e) => ("elevation-failed", Some(e.to_string())),
                        Ok(resp) => ("elevation-failed", resp.error),
                    };
                    skipped_machine(&mut op, m, base, reason, message.as_deref());
                }
            }
        }
        op.finished_at = exec::unix_now();
        op
    })
    .await?;
    let journal_error = clean::journal(&app, &state, &op);
    lock(&programs.leftovers)?.remove(&id);
    Ok(clean::op_dto(&op, journal_error))
}

/// The all-users items as skipped (the administrator prompt was declined or the
/// elevated run failed).
fn skipped_machine(
    op: &mut Operation,
    m: &elevated::Request,
    base: usize,
    reason: &str,
    message: Option<&str>,
) {
    let mut n = base;
    for f in &m.files {
        op.items.push(exec::ItemResult {
            n,
            path: f.path.clone(),
            rule_id: Some(format!("leftover.{}", f.reason)),
            mode: DeleteMode::RecycleBin,
            is_dir: f.is_dir,
            bytes: f.bytes,
            files: f.files,
            freed: 0,
            status: ItemStatus::Skipped,
            reason: Some(reason.to_owned()),
            message: message.map(str::to_owned),
            stored: None,
            warnings: Vec::new(),
        });
        n += 1;
    }
    for j in &m.registry {
        let mut r = remove::registry_result(j, n);
        r.reason = Some(reason.to_owned());
        r.message = message.map(str::to_owned);
        op.items.push(r);
        n += 1;
    }
}

/// Find the software junk of the installed programs (read-only).
#[tauri::command]
pub async fn programs_junk_scan(
    app: AppHandle,
    programs: State<'_, ProgramsState>,
) -> CmdResult<JunkReport> {
    let list = lock(&programs.list)?.clone();
    let own: Vec<PathBuf> = [clean::app_local(&app), crate::paths::roaming_data(&app)]
        .into_iter()
        .flatten()
        .collect();
    let report = blocking(move || -> CmdResult<JunkReport> {
        let list = list.unwrap_or_else(|| runner::registry_list(own_dir()));
        let packs = nfd_rules::builtin_packs().map_err(|e| err("rules", e))?;
        let resolve = resolver();
        let matcher = nfd_rules::matcher::Matcher::new(&packs, &resolve, std::env::consts::OS);
        let covered = junk::covered_by(&matcher);
        let running = runner::running_images();
        let gate = gate();
        let ctx = JunkCtx {
            gate: &gate,
            covered: &covered,
            running: &running,
            max_entries: MAX_ENTRIES,
            depth: 5,
            cancel: None,
        };
        Ok(junk::scan(&list, &JunkRoots::detect(&resolve, &own), &ctx))
    })
    .await??;
    *lock(&programs.junk)? = Some(report.clone());
    Ok(report)
}

/// Clean the chosen software junk of the last scan into the quarantine (kind
/// `software-junk`). Progress streams as `clean-progress`.
#[tauri::command]
pub async fn programs_junk_clean(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    programs: State<'_, ProgramsState>,
    paths: Vec<String>,
) -> CmdResult<OperationDto> {
    let plan = {
        let guard = lock(&programs.junk)?;
        let report = guard
            .as_ref()
            .ok_or_else(|| err("no-junk-scan", "no software junk scan yet"))?;
        let picked: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        junk::plan(report, &picked)
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let result = clean::execute_plan(&app, &window, &state, plan, "software-junk", cancel).await;
    *lock(&programs.junk)? = None;
    result
}

/// Show a program's folder, a leftover or a junk item in Explorer (only paths the
/// view listed).
#[tauri::command]
pub fn programs_reveal(
    app: AppHandle,
    programs: State<'_, ProgramsState>,
    path: String,
) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = PathBuf::from(&path);
    let same = |a: &Path| nfd_apps::inventory::same(a, &p);
    let known = lock(&programs.list)?.as_ref().is_some_and(|l| {
        l.iter()
            .any(|x| x.install_location.as_deref().is_some_and(same))
    }) || lock(&programs.leftovers)?
        .values()
        .any(|r| r.items.iter().any(|i| same(Path::new(&i.path))))
        || lock(&programs.junk)?.as_ref().is_some_and(|r| {
            r.groups
                .iter()
                .any(|g| same(&g.folder) || g.items.iter().any(|i| same(&i.path)))
        });
    if !known {
        return Err(err("not-found", path));
    }
    let target = if p.exists() {
        p
    } else {
        // a removed item: its folder
        p.ancestors()
            .find(|a| a.exists())
            .map(Path::to_path_buf)
            .ok_or_else(|| err("not-found", &path))?
    };
    app.opener()
        .reveal_item_in_dir(&target)
        .map_err(|e| err("os", e))
}

/// Put back items of operation `id` that need administrator rights (all-users
/// leftovers: Recycle Bin items in Program Files / ProgramData, `HKEY_LOCAL_MACHINE`
/// backups) through one elevated run. `items` = item numbers (all when `None`).
#[tauri::command]
pub async fn activity_restore_admin(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    items: Option<Vec<u64>>,
) -> CmdResult<Vec<RestoreResult>> {
    let (row, rows) = with_history(&app, &state, |h| h.operation(&id))?;
    let mut req = elevated::Request::new(&id);
    for r in rows
        .iter()
        .filter(|r| r.restorable(&row.state))
        .filter(|r| items.as_ref().is_none_or(|o| o.contains(&r.n)))
    {
        let mode = match r.mode.as_str() {
            "registry" => DeleteMode::Registry,
            "recycle-bin" => DeleteMode::RecycleBin,
            _ => continue,
        };
        req.restore.push(elevated::RestoreJob {
            n: r.n,
            path: r.path.clone(),
            mode,
            stored: r.stored.as_ref().map(PathBuf::from),
            is_key: r.is_dir,
            deleted_after: row.started_at,
        });
    }
    if req.restore.is_empty() {
        return Ok(Vec::new());
    }
    let local = clean::app_local(&app).ok_or_else(|| err("os", "no data folder"))?;
    let owner = owner_handle(&window);
    let req2 = req.clone();
    let resp = blocking(move || {
        std::env::current_exe()
            .and_then(|exe| runner::run_elevated_step(&exe, &req2, &local.join("elevated"), owner))
    })
    .await?
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::Interrupted {
            err("uac-declined", e)
        } else {
            err("os", e)
        }
    })?;
    if let Some(e) = resp.error {
        return Err(err("elevation-failed", e));
    }
    let back: Vec<u64> = resp
        .restored
        .iter()
        .filter(|r| matches!(r.status, RestoreStatus::Restored | RestoreStatus::Missing))
        .map(|r| r.n)
        .collect();
    if !back.is_empty() {
        with_history(&app, &state, |h| h.mark_restored(&id, &back))?;
    }
    if let Some(dir) = clean::audit_dir(&app) {
        let _ = nfd_history::audit::note(
            &dir,
            exec::unix_now(),
            &format!("{id}\trestore (administrator)\t{} item(s)", back.len()),
        );
    }
    Ok(resp
        .restored
        .into_iter()
        .map(|r| RestoreResult {
            path: req
                .restore
                .iter()
                .find(|j| j.n == r.n)
                .map(|j| j.path.clone())
                .unwrap_or_default(),
            n: r.n,
            status: r.status,
            message: r.message,
        })
        .collect())
}
