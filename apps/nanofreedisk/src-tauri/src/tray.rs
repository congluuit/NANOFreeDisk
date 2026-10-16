//! Notification-area icon (PLAN §13 G5): the drives' verdict and the free space of
//! each fixed drive at a glance, "Open" and "Quit". Display only — no action in the
//! menu deletes or changes anything. Texts come from the UI (EN / VI); English is
//! used until it sends them.

use std::collections::HashMap;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const ID: &str = "nanofreedisk";

/// Text for `key` from the UI, else the English default.
pub fn text(texts: &HashMap<String, String>, key: &str) -> String {
    if let Some(t) = texts.get(key).filter(|t| !t.is_empty()) {
        return t.clone();
    }
    match key {
        "tray-open" => "Open NANOFreeDisk",
        "tray-quit" => "Quit",
        "tray-status" => "Drive health: {status}",
        "tray-free" => "{drive}  {size} free ({percent})",
        "status-good" => "Good",
        "status-caution" => "Caution",
        "status-bad" => "Bad",
        "status-unknown" => "Unknown",
        _ => key,
    }
    .to_owned()
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// One line per drive plus the verdict, from the last health report.
pub struct TrayState {
    pub status: String,
    /// (drive root, free, total).
    pub drives: Vec<(String, u64, u64)>,
}

fn lines(state: &TrayState, texts: &HashMap<String, String>) -> (String, Vec<String>) {
    let status = text(texts, &format!("status-{}", state.status));
    let head = text(texts, "tray-status").replace("{status}", &status);
    let drives = state
        .drives
        .iter()
        .map(|(root, free, total)| {
            let pct = if *total > 0 {
                *free as f64 * 100.0 / *total as f64
            } else {
                0.0
            };
            text(texts, "tray-free")
                .replace("{drive}", root.trim_end_matches('\\'))
                .replace("{size}", &nfd_core::units::format_size(*free))
                .replace("{percent}", &format!("{pct:.0} %"))
        })
        .collect();
    (head, drives)
}

fn menu(
    app: &AppHandle,
    state: Option<&TrayState>,
    texts: &HashMap<String, String>,
) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "open",
        text(texts, "tray-open"),
        true,
        None::<&str>,
    )?)?;
    if let Some(s) = state {
        let (head, drives) = lines(s, texts);
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&MenuItem::with_id(
            app,
            "status",
            head,
            false,
            None::<&str>,
        )?)?;
        for (i, d) in drives.into_iter().enumerate() {
            menu.append(&MenuItem::with_id(
                app,
                format!("drive-{i}"),
                d,
                false,
                None::<&str>,
            )?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        text(texts, "tray-quit"),
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

/// Create the icon (call once at startup).
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let texts = HashMap::new();
    let mut builder = TrayIconBuilder::with_id(ID)
        .tooltip("NANOFreeDisk")
        .menu(&menu(app, None, &texts)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
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
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Show the latest verdict and free space.
pub fn update(app: &AppHandle, state: &TrayState, texts: &HashMap<String, String>) {
    let Some(tray) = app.tray_by_id(ID) else {
        return;
    };
    let (head, drives) = lines(state, texts);
    let mut tip = format!("NANOFreeDisk\n{head}");
    for d in drives.iter().take(4) {
        tip.push('\n');
        tip.push_str(d);
    }
    let _ = tray.set_tooltip(Some(tip));
    if let Ok(m) = menu(app, Some(state), texts) {
        let _ = tray.set_menu(Some(m));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_use_the_ui_texts() {
        let state = TrayState {
            status: "caution".into(),
            drives: vec![(r"C:\".into(), 50 << 30, 500 << 30)],
        };
        let (head, drives) = lines(&state, &HashMap::new());
        assert_eq!(head, "Drive health: Caution");
        assert_eq!(drives, vec!["C:  50.0 GiB free (10 %)".to_owned()]);
        let vi: HashMap<String, String> = [
            ("tray-status", "Sức khỏe ổ: {status}"),
            ("status-caution", "Cần chú ý"),
            ("tray-free", "{drive} còn {size} ({percent})"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        let (head, drives) = lines(&state, &vi);
        assert_eq!(head, "Sức khỏe ổ: Cần chú ý");
        assert_eq!(drives[0], "C: còn 50.0 GiB (10 %)");
    }
}
