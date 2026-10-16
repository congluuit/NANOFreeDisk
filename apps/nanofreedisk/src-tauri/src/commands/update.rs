//! Settings → Updates (`docs/PHASE8_INSTALLERS.md` §2.5). Checked only when the user
//! asks. The release's `latest.json` is signed with the minisign key whose public
//! half is in `tauri.conf.json`; the plugin refuses anything else.
//!
//! Only the NSIS and MSI packages update in place. The portable zip and the
//! branded setup (`install.manifest` next to the exe) are told about the new
//! version and get a link to the release page, because the Tauri installers would
//! install a second copy elsewhere.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use super::{CmdResult, err};

/// Release page for installs that cannot update in place.
pub const RELEASES: &str = "https://github.com/congluuit/NANOFreeDisk/releases/latest";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallKind {
    /// NSIS or MSI: the updater installs the new version.
    Installer,
    /// Branded setup: download the new setup.
    Setup,
    Portable,
}

fn install_kind() -> InstallKind {
    if nfd_history::portable_dir().is_some() {
        return InstallKind::Portable;
    }
    let branded = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("install.manifest").is_file()))
        .unwrap_or(false);
    if branded {
        InstallKind::Setup
    } else {
        InstallKind::Installer
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current: String,
    /// Newer version, if any.
    available: Option<String>,
    notes: Option<String>,
    date: Option<String>,
    kind: InstallKind,
    releases: &'static str,
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> CmdResult<UpdateInfo> {
    let updater = app.updater().map_err(|e| err("update", e))?;
    let update = updater.check().await.map_err(|e| err("update", e))?;
    Ok(UpdateInfo {
        current: app.package_info().version.to_string(),
        available: update.as_ref().map(|u| u.version.clone()),
        notes: update.as_ref().and_then(|u| u.body.clone()),
        date: update.as_ref().and_then(|u| u.date.map(|d| d.to_string())),
        kind: install_kind(),
        releases: RELEASES,
    })
}

/// Download, verify and run the new installer, then restart (NSIS/MSI only).
#[tauri::command]
pub async fn update_install(app: AppHandle) -> CmdResult<()> {
    if install_kind() != InstallKind::Installer {
        return Err(err("update", "this copy updates from the release page"));
    }
    let updater = app.updater().map_err(|e| err("update", e))?;
    let Some(update) = updater.check().await.map_err(|e| err("update", e))? else {
        return Ok(());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| err("update", e))?;
    app.restart();
}

/// Open the latest release page (portable and branded-setup installs).
#[tauri::command]
pub fn update_open_releases(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(RELEASES, None::<&str>)
        .map_err(|e| err("os", e))
}
