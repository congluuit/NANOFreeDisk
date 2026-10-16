//! System tray icon (PLAN §13.2 G5): open the window, pause / resume the disk
//! monitor, quit. Labels follow the UI language ([`tray_texts`]); the pause item
//! stays in step with the Monitor view's own Pause button.

use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, Wry};

use crate::commands::monitor::{MonitorState, set_paused};
use crate::commands::{CmdResult, err};

/// The menu items whose label or check mark changes.
struct Items {
    open: MenuItem<Wry>,
    pause: CheckMenuItem<Wry>,
    quit: MenuItem<Wry>,
}

#[derive(Default)]
pub struct TrayState {
    items: Mutex<Option<Items>>,
}

impl std::fmt::Debug for TrayState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrayState").finish_non_exhaustive()
    }
}

fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Create the tray icon (English labels until the UI sends its own).
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open NANOFreeDisk", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(
        app,
        "pause",
        "Pause disk monitor",
        true,
        false,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("NANOFreeDisk")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_window(app),
            "pause" => {
                let state = app.state::<MonitorState>();
                let paused = !crate::commands::monitor::is_paused(&state);
                let _ = set_paused(app, &state, paused);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    if let Ok(mut slot) = app.state::<TrayState>().items.lock() {
        *slot = Some(Items { open, pause, quit });
    }
    Ok(())
}

/// Keep the check mark in step with the monitor (called when it is paused or
/// resumed from anywhere).
pub fn sync_paused(app: &AppHandle, paused: bool) {
    if let Some(state) = app.try_state::<TrayState>() {
        if let Ok(slot) = state.items.lock() {
            if let Some(items) = slot.as_ref() {
                let _ = items.pause.set_checked(paused);
            }
        }
    }
}

/// Tray labels in the UI language.
#[tauri::command]
pub fn tray_texts(
    state: State<'_, TrayState>,
    open: String,
    pause: String,
    quit: String,
) -> CmdResult<()> {
    let slot = state.items.lock().map_err(|e| err("internal", e))?;
    if let Some(items) = slot.as_ref() {
        items.open.set_text(open).map_err(|e| err("os", e))?;
        items.pause.set_text(pause).map_err(|e| err("os", e))?;
        items.quit.set_text(quit).map_err(|e| err("os", e))?;
    }
    Ok(())
}
