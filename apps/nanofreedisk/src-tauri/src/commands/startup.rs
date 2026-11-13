//! Startup view (PLAN §13.2 E2, `docs/STARTUP.md`): the programs that start with
//! Windows, turned on or off the way Task Manager does (its `StartupApproved`
//! flag; the `Run` value and the shortcut stay). Nothing is deleted. This user's
//! entries change here; all users' entries need administrator rights and are left
//! to Task Manager, which [`startup_open_task_manager`] opens on its Startup page.
//! Windows only; the UI hides the view on macOS.

use std::collections::HashMap;
use std::sync::Mutex;

use nfd_apps::startup::{self, ChangeError, StartupEntry};
use nfd_core::exec;
use serde::Serialize;
use tauri::{AppHandle, State};

use super::{CmdResult, err};

/// The last listing (icons are read for its entries only).
#[derive(Debug, Default)]
pub struct StartupState {
    list: Mutex<Option<Vec<StartupEntry>>>,
    icons: Mutex<HashMap<String, Option<IconDto>>>,
}

/// An entry's icon, as the Programs list hands them (`data:image/png;base64,…`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconDto {
    src: String,
    plate: bool,
}

fn lock<T>(m: &Mutex<T>) -> CmdResult<std::sync::MutexGuard<'_, T>> {
    m.lock().map_err(|e| err("internal", e))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| err("internal", e))
}

/// Every program that starts with Windows (both scopes, registry and folders).
#[tauri::command]
pub async fn startup_list(state: State<'_, StartupState>) -> CmdResult<Vec<StartupEntry>> {
    if cfg!(not(windows)) {
        return Err(err("unsupported", "the Startup view is Windows-only"));
    }
    let list = blocking(startup::list_live).await?;
    *lock(&state.list)? = Some(list.clone());
    Ok(list)
}

/// Turn this user's entry `id` on or off (Task Manager's flag). Returns the entry
/// as it is now. All-users entries: `needs-admin`.
#[tauri::command]
pub async fn startup_set(
    app: AppHandle,
    state: State<'_, StartupState>,
    id: String,
    enabled: bool,
) -> CmdResult<StartupEntry> {
    let now = exec::unix_now();
    let key = id.clone();
    let entry = blocking(move || startup::set_enabled_live(&key, enabled, now))
        .await?
        .map_err(|e| match e {
            ChangeError::NotFound => err("not-found", &id),
            ChangeError::NeedsAdmin => err("needs-admin", &id),
            ChangeError::Io(e) => err("os", e),
        })?;
    if let Some(list) = lock(&state.list)?.as_mut() {
        if let Some(slot) = list.iter_mut().find(|e| e.id == entry.id) {
            *slot = entry.clone();
        }
    }
    if let Some(dir) = super::clean::audit_dir(&app) {
        let _ = nfd_history::audit::note(
            &dir,
            now,
            &format!(
                "startup\t{}\t{}",
                if enabled { "enable" } else { "disable" },
                entry.id
            ),
        );
    }
    Ok(entry)
}

/// The icons of entries `ids` of the last listing (entries without one left out).
#[tauri::command]
pub async fn startup_icons(
    state: State<'_, StartupState>,
    ids: Vec<String>,
) -> CmdResult<HashMap<String, IconDto>> {
    let mut out = HashMap::new();
    let todo: Vec<StartupEntry> = {
        let cache = lock(&state.icons)?;
        let list = lock(&state.list)?;
        let mut todo = Vec::new();
        for id in &ids {
            match cache.get(id) {
                Some(Some(icon)) => {
                    out.insert(id.clone(), icon.clone());
                }
                Some(None) => {}
                None => todo.extend(list.iter().flatten().find(|e| &e.id == id).cloned()),
            }
        }
        todo
    };
    if todo.is_empty() {
        return Ok(out);
    }
    let (asked, found) = blocking(move || {
        let found = startup::icons(&todo);
        (todo, found)
    })
    .await?;
    let mut cache = lock(&state.icons)?;
    for e in &asked {
        cache.insert(e.id.clone(), None);
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

/// Open Task Manager on its Startup apps page (all-users entries are changed
/// there, as administrator).
#[tauri::command]
pub fn startup_open_task_manager() -> CmdResult<()> {
    if cfg!(not(windows)) {
        return Err(err("unsupported", "Task Manager is Windows-only"));
    }
    let sys = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    std::process::Command::new(
        std::path::Path::new(&sys)
            .join("System32")
            .join("Taskmgr.exe"),
    )
    .args(["/0", "/startup"])
    .spawn()
    .map(|_| ())
    .map_err(|e| err("os", e))
}
