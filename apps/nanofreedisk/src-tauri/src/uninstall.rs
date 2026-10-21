//! `nanofreedisk.exe --uninstall-cleanup`: what every uninstaller (NSIS, MSI, the
//! branded setup) runs before it removes the program files
//! (`docs/PHASE8_INSTALLERS.md` §2.3).
//!
//! * Files NANOFreeDisk moved to its quarantine are still the user's: they are put
//!   back, deleted for good, or (when the data is kept) left for a later install —
//!   always after a clear question, never silently deleted.
//! * `--remove-data` then removes the data folders; a quarantine that still holds
//!   files (an item that could not be put back) is kept.
//! * The Explorer menu is always removed.
//! * "Start without the Windows prompt" (ADR 0003) is removed: the account's
//!   scheduled task and the helper's protected copy in Program Files — through the
//!   task itself when it matches this build (no prompt), else through UAC after a
//!   message (never with `--silent`). An elevated cleanup (per-machine uninstall)
//!   removes every account's.
//!
//! Flags: `--remove-data`, `--silent` (no dialogs: quarantine restored when the data
//! goes, else left alone), `--quarantine restore|purge|keep`, `--lang en|vi`.
//! Exit code 0 unless the database could not be read (1); the uninstall goes on
//! either way.

use std::path::{Path, PathBuf};

use nfd_core::exec::QuarantineRoots;
use nfd_core::units::format_size;
use nfd_history::uninstall;

pub const FLAG: &str = "--uninstall-cleanup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Restore,
    Purge,
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    En,
    Vi,
}

#[derive(Debug)]
struct Options {
    remove_data: bool,
    silent: bool,
    choice: Option<Choice>,
    lang: Option<Lang>,
}

fn parse(args: &[String]) -> Options {
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(|v| v.to_ascii_lowercase())
    };
    Options {
        remove_data: args.iter().any(|a| a == "--remove-data"),
        silent: args.iter().any(|a| a == "--silent"),
        choice: value("--quarantine").and_then(|v| match v.as_str() {
            "restore" => Some(Choice::Restore),
            "purge" => Some(Choice::Purge),
            "keep" => Some(Choice::Keep),
            _ => None,
        }),
        lang: value("--lang").and_then(|v| lang_code(&v)),
    }
}

fn lang_code(v: &str) -> Option<Lang> {
    match v.get(..2)? {
        "vi" => Some(Lang::Vi),
        "en" => Some(Lang::En),
        _ => None,
    }
}

/// The app's language setting (`settings.json` → `locale`), else Windows'.
fn detect_lang(data_dirs: &[PathBuf]) -> Lang {
    for dir in data_dirs {
        let Ok(bytes) = std::fs::read(dir.join("settings.json")) else {
            continue;
        };
        let locale = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|v| v.get("locale")?.as_str().map(str::to_owned));
        if let Some(lang) = locale.as_deref().and_then(lang_code) {
            return lang;
        }
    }
    sys::ui_lang()
}

struct Text {
    title: &'static str,
    close_app: &'static str,
    ask_keep: &'static str,
    ask_remove: &'static str,
    confirm_purge: &'static str,
    failed: &'static str,
    #[cfg_attr(not(windows), allow(dead_code))]
    helper_task: &'static str,
}

const EN: Text = Text {
    title: "Uninstall NANOFreeDisk",
    close_app: "NANOFreeDisk is still running. Close it, then press Retry.",
    ask_keep: "{n} item(s) ({size}) that NANOFreeDisk cleaned are still in its quarantine and can be restored.\n\nYes — put them back where they were\nNo — delete them permanently\nCancel — leave them in the quarantine (a later install can restore them)",
    ask_remove: "{n} item(s) ({size}) that NANOFreeDisk cleaned are still in its quarantine and can be restored.\n\nYes — put them back where they were\nNo — delete them permanently",
    confirm_purge: "Permanently delete {n} item(s) ({size})? This cannot be undone.",
    failed: "{n} item(s) could not be put back and stay in the quarantine:\n\n{list}",
    helper_task: "NANOFreeDisk was set up to start its administrator helper without the Windows prompt. Windows will now ask for administrator rights once to remove that task and the helper's copy in Program Files.",
};

const VI: Text = Text {
    title: "Gỡ cài đặt NANOFreeDisk",
    close_app: "NANOFreeDisk vẫn đang chạy. Hãy đóng ứng dụng rồi bấm Retry (Thử lại).",
    ask_keep: "{n} mục ({size}) NANOFreeDisk đã dọn vẫn nằm trong khu cách ly và còn khôi phục được.\n\nYes (Có) — trả chúng về chỗ cũ\nNo (Không) — xóa vĩnh viễn\nCancel (Hủy) — để lại trong khu cách ly (lần cài sau vẫn khôi phục được)",
    ask_remove: "{n} mục ({size}) NANOFreeDisk đã dọn vẫn nằm trong khu cách ly và còn khôi phục được.\n\nYes (Có) — trả chúng về chỗ cũ\nNo (Không) — xóa vĩnh viễn",
    confirm_purge: "Xóa vĩnh viễn {n} mục ({size})? Không thể hoàn tác.",
    failed: "{n} mục không trả về được và vẫn nằm trong khu cách ly:\n\n{list}",
    helper_task: "NANOFreeDisk đã được thiết lập để khởi động trình trợ giúp quản trị mà không cần hỏi Windows. Windows sẽ hỏi quyền quản trị một lần để gỡ tác vụ đó và bản sao của trình trợ giúp trong Program Files.",
};

fn fill(template: &str, n: u64, size: u64) -> String {
    template
        .replace("{n}", &n.to_string())
        .replace("{size}", &format_size(size))
}

/// Another `nanofreedisk.exe` (not this process) is running.
fn app_running() -> bool {
    nfd_platform::locks::running_processes().is_ok_and(|all| {
        all.iter()
            .filter(|p| p.as_str() == "nanofreedisk.exe")
            .count()
            > 1
    })
}

/// Run the cleanup; returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let opts = parse(args);
    let data_dirs = uninstall::data_dirs();
    let text = match opts.lang.unwrap_or_else(|| detect_lang(&data_dirs)) {
        Lang::Vi => &VI,
        Lang::En => &EN,
    };
    let _ = nfd_platform::shell_menu::unregister();

    if !opts.silent {
        while app_running() {
            if !sys::retry(text.title, text.close_app) {
                break;
            }
        }
    }
    #[cfg(windows)]
    remove_helper_task(opts.silent, text);

    let db = nfd_history::default_path();
    let local = db.as_deref().and_then(Path::parent).map(Path::to_path_buf);
    let roots = QuarantineRoots::new(local.as_deref());
    let mut code = 0;
    let mut failed = Vec::new();
    if let Some(path) = db.filter(|p| p.is_file()) {
        match nfd_history::History::open(&path) {
            Ok(mut h) => {
                let pending = uninstall::pending(&h).unwrap_or_default();
                if !pending.is_empty() {
                    let choice = decide(&opts, text, pending.items, pending.bytes);
                    match choice {
                        Choice::Restore => {
                            if let Ok(r) = uninstall::restore_all(&mut h) {
                                failed = r.failed;
                            }
                        }
                        Choice::Purge => {
                            let _ = uninstall::purge_all(&mut h, &roots);
                        }
                        Choice::Keep => {}
                    }
                }
            }
            Err(_) => code = 1,
        }
    }
    if !failed.is_empty() && !opts.silent {
        let list: Vec<&str> = failed.iter().take(15).map(String::as_str).collect();
        let msg = fill(text.failed, failed.len() as u64, 0).replace("{list}", &list.join("\n"));
        sys::info(text.title, &msg);
    }

    if opts.remove_data {
        // keep a system-drive quarantine that still holds something
        let keep = roots
            .system
            .clone()
            .filter(|q| std::fs::read_dir(q).is_ok_and(|mut rd| rd.next().is_some()));
        for dir in &data_dirs {
            // WebView2 helpers may hold the profile for a moment after the app quit
            for _ in 0..10 {
                if uninstall::remove_data_dir(dir, keep.as_deref()).is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            if dir.ends_with("pawnio") {
                uninstall::remove_empty_parent(dir);
            }
        }
    }
    uninstall::remove_empty_volume_quarantines(&roots);
    code
}

/// Remove "start without the Windows prompt" (ADR 0003) before the program files go.
#[cfg(windows)]
fn remove_helper_task(silent: bool, text: &Text) {
    use nfd_helper::TaskState;
    use nfd_helper::win_task;
    if win_task::elevated() {
        let _ = win_task::remove_all();
        return;
    }
    let state = win_task::state();
    if state == TaskState::Off {
        return;
    }
    let through_task = if state == TaskState::On {
        nfd_helper::start_task().ok()
    } else {
        None
    };
    let conn = through_task.or_else(|| {
        if silent {
            return None; // stale and silent: left in place (ADR 0003, residual risks)
        }
        sys::info(text.title, text.helper_task);
        nfd_helper::start().ok()
    });
    if let Some(c) = conn {
        let _ = c.task_remove();
    }
}

fn decide(opts: &Options, text: &Text, items: u64, bytes: u64) -> Choice {
    if let Some(c) = opts.choice {
        // keeping the quarantine while removing the data would delete it anyway
        return if c == Choice::Keep && opts.remove_data {
            Choice::Restore
        } else {
            c
        };
    }
    if opts.silent {
        return if opts.remove_data {
            Choice::Restore
        } else {
            Choice::Keep
        };
    }
    let question = if opts.remove_data {
        text.ask_remove
    } else {
        text.ask_keep
    };
    match sys::ask(text.title, &fill(question, items, bytes), !opts.remove_data) {
        sys::Answer::Yes => Choice::Restore,
        sys::Answer::No => {
            if sys::confirm(text.title, &fill(text.confirm_purge, items, bytes)) {
                Choice::Purge
            } else if opts.remove_data {
                Choice::Restore
            } else {
                Choice::Keep
            }
        }
        sys::Answer::Cancel => {
            if opts.remove_data {
                Choice::Restore
            } else {
                Choice::Keep
            }
        }
    }
}

#[cfg(windows)]
mod sys {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDNO, IDOK, IDRETRY, IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_ICONWARNING,
        MB_OKCANCEL, MB_RETRYCANCEL, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, MB_YESNOCANCEL,
        MESSAGEBOX_STYLE, MessageBoxW,
    };

    use super::Lang;

    pub enum Answer {
        Yes,
        No,
        Cancel,
    }

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn message(title: &str, text: &str, style: MESSAGEBOX_STYLE) -> i32 {
        let (t, m) = (wide(title), wide(text));
        // SAFETY: both buffers are NUL-terminated and outlive the call; no owner window.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                m.as_ptr(),
                t.as_ptr(),
                style | MB_TOPMOST | MB_SETFOREGROUND,
            )
        }
    }

    pub fn ask(title: &str, text: &str, with_cancel: bool) -> Answer {
        let buttons = if with_cancel {
            MB_YESNOCANCEL
        } else {
            MB_YESNO
        };
        match message(title, text, buttons | MB_ICONQUESTION) {
            IDYES => Answer::Yes,
            IDNO => Answer::No,
            _ => Answer::Cancel,
        }
    }

    pub fn confirm(title: &str, text: &str) -> bool {
        message(title, text, MB_OKCANCEL | MB_ICONWARNING) == IDOK
    }

    pub fn retry(title: &str, text: &str) -> bool {
        message(title, text, MB_RETRYCANCEL | MB_ICONWARNING) == IDRETRY
    }

    pub fn info(title: &str, text: &str) {
        message(title, text, MB_ICONINFORMATION);
    }

    pub fn ui_lang() -> Lang {
        // SAFETY: no arguments, no side effects.
        let lang = unsafe { GetUserDefaultUILanguage() };
        // primary language id 0x2A = Vietnamese
        if lang & 0x3ff == 0x2a {
            Lang::Vi
        } else {
            Lang::En
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::Lang;

    #[allow(dead_code)]
    pub enum Answer {
        Yes,
        No,
        Cancel,
    }

    pub fn ask(_: &str, _: &str, _: bool) -> Answer {
        Answer::Cancel
    }
    pub fn confirm(_: &str, _: &str) -> bool {
        false
    }
    pub fn retry(_: &str, _: &str) -> bool {
        false
    }
    pub fn info(_: &str, _: &str) {}
    pub fn ui_lang() -> Lang {
        Lang::En
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn silent_choices_never_lose_files() {
        let t = &EN;
        let o = parse(&args(&["x", FLAG, "--silent", "--remove-data"]));
        assert_eq!(decide(&o, t, 3, 10), Choice::Restore);
        let o = parse(&args(&["x", FLAG, "--silent"]));
        assert_eq!(decide(&o, t, 3, 10), Choice::Keep);
        // "keep" cannot survive removing the data folder
        let o = parse(&args(&["x", FLAG, "--quarantine", "keep", "--remove-data"]));
        assert_eq!(decide(&o, t, 3, 10), Choice::Restore);
        let o = parse(&args(&["x", FLAG, "--quarantine", "PURGE"]));
        assert_eq!(decide(&o, t, 3, 10), Choice::Purge);
    }

    #[test]
    fn language_flag() {
        assert_eq!(parse(&args(&["x", "--lang", "vi-VN"])).lang, Some(Lang::Vi));
        assert_eq!(parse(&args(&["x", "--lang", "en"])).lang, Some(Lang::En));
        assert_eq!(parse(&args(&["x", "--lang", "fr"])).lang, None);
        assert_eq!(
            fill(VI.confirm_purge, 2, 2048),
            "Xóa vĩnh viễn 2 mục (2.0 KiB)? Không thể hoàn tác."
        );
    }
}
