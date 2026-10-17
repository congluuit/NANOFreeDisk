//! Install and uninstall steps (Windows). Every file and shortcut the setup creates
//! is listed in `install.manifest` in the install folder, and the uninstaller removes
//! exactly those — never a whole folder the user picked (it could be `C:\Tools`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use winreg::RegKey;
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, KEY_READ, KEY_WOW64_64KEY,
};

use crate::payload::{self, Entry};
use crate::sys;
use crate::text::Lang;
use crate::{APP_EXE, PRODUCT};

/// `…\CurrentVersion\Uninstall\NANOFreeDisk`.
pub const ARP_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\NANOFreeDisk";
const ARP_ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
/// Value that marks an Apps & features entry written by this setup.
const MARKER: &str = "NanoInstaller";
/// Name of the file list in the install folder.
pub const MANIFEST: &str = "install.manifest";

/// What the user chose.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Plan {
    pub dir: PathBuf,
    pub all_users: bool,
    pub start_menu: bool,
    pub desktop: bool,
    pub shell_menu: bool,
    pub lang: Lang,
}

/// Progress steps, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Check,
    Files,
    WebView,
    WebViewDownload,
    Register,
    Shortcuts,
}

/// Why an install or uninstall stopped.
#[derive(Debug, Clone)]
pub enum Problem {
    NeedsAdmin,
    /// The folder cannot be created or written by this user (Program Files for a
    /// per-user install). Nothing is left behind.
    NotWritable(PathBuf),
    NoPayload,
    Running(Vec<String>),
    Foreign(Existing),
    Io(String),
}

impl From<io::Error> for Problem {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Result of a finished install.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// WebView2 could not be installed: the reason.
    pub webview_warning: Option<String>,
}

/// An installation found in Apps & features.
#[derive(Debug, Clone)]
pub struct Existing {
    /// Written by this setup (else by the MSI or the NSIS package).
    pub ours: bool,
    pub all_users: bool,
    pub dir: Option<PathBuf>,
    pub version: String,
    pub uninstall: String,
}

fn root(all_users: bool) -> RegKey {
    RegKey::predef(if all_users {
        HKEY_LOCAL_MACHINE
    } else {
        HKEY_CURRENT_USER
    })
}

/// Every NANOFreeDisk entry in Apps & features (this user and machine-wide),
/// whichever package installed it.
#[must_use]
pub fn existing() -> Vec<Existing> {
    let mut out = Vec::new();
    for all_users in [false, true] {
        let Ok(arp) = root(all_users).open_subkey_with_flags(ARP_ROOT, KEY_READ | KEY_WOW64_64KEY)
        else {
            continue;
        };
        for name in arp.enum_keys().flatten() {
            let Ok(k) = arp.open_subkey(&name) else {
                continue;
            };
            let display: String = k.get_value("DisplayName").unwrap_or_default();
            if display != PRODUCT {
                continue;
            }
            let marker: String = k.get_value(MARKER).unwrap_or_default();
            let dir: String = k.get_value("InstallLocation").unwrap_or_default();
            let uninstall: String = k.get_value("UninstallString").unwrap_or_default();
            if is_stale(&uninstall) {
                continue;
            }
            out.push(Existing {
                ours: marker == "setup",
                all_users,
                dir: (!dir.is_empty()).then(|| PathBuf::from(dir.trim_matches('"'))),
                version: k.get_value("DisplayVersion").unwrap_or_default(),
                uninstall,
            });
        }
    }
    out
}

/// The uninstaller an `UninstallString` runs: the quoted path, or the text up to
/// `.exe`. `None` for `msiexec` (the MSI keeps its own records).
fn uninstaller_path(uninstall: &str) -> Option<PathBuf> {
    let s = uninstall.trim();
    let path = if let Some(rest) = s.strip_prefix('"') {
        &rest[..rest.find('"')?]
    } else {
        let end = s.to_ascii_lowercase().find(".exe")? + 4;
        &s[..end]
    };
    let file = Path::new(path)
        .file_name()?
        .to_string_lossy()
        .to_ascii_lowercase();
    (file != "msiexec.exe").then(|| PathBuf::from(path))
}

/// An entry left by an install that failed half-way: its uninstaller is missing,
/// so there is nothing to uninstall and it must not block a new install.
fn is_stale(uninstall: &str) -> bool {
    uninstaller_path(uninstall).is_some_and(|p| p.is_absolute() && !p.exists())
}

/// Delete stale NANOFreeDisk entries (see [`is_stale`]) from Apps & features. The
/// machine-wide ones only go when this process is elevated.
fn prune_stale() {
    for all_users in [false, true] {
        let Ok(arp) =
            root(all_users).open_subkey_with_flags(ARP_ROOT, KEY_ALL_ACCESS | KEY_WOW64_64KEY)
        else {
            continue;
        };
        let names: Vec<String> = arp.enum_keys().flatten().collect();
        for name in names {
            let Ok(k) = arp.open_subkey(&name) else {
                continue;
            };
            let display: String = k.get_value("DisplayName").unwrap_or_default();
            let uninstall: String = k.get_value("UninstallString").unwrap_or_default();
            if display == PRODUCT && is_stale(&uninstall) {
                let _ = arp.delete_subkey_all(&name);
            }
        }
    }
}

/// Bytes the payload will take on disk.
#[must_use]
pub fn installed_size(entries: &[Entry]) -> u64 {
    entries.iter().map(|e| e.data.len() as u64).sum()
}

#[derive(Debug, Default)]
struct Manifest {
    all_users: bool,
    files: Vec<String>,
    shortcuts: Vec<PathBuf>,
}

impl Manifest {
    fn read(dir: &Path) -> Option<Self> {
        let text = fs::read_to_string(dir.join(MANIFEST)).ok()?;
        let mut m = Self::default();
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("all_users=") {
                m.all_users = v == "1";
            } else if let Some(v) = line.strip_prefix("file=") {
                if payload::check_name(v).is_ok() {
                    m.files.push(v.to_owned());
                }
            } else if let Some(v) = line.strip_prefix("shortcut=") {
                if v.to_ascii_lowercase().ends_with(".lnk") {
                    m.shortcuts.push(PathBuf::from(v));
                }
            }
        }
        Some(m)
    }

    fn write(&self, dir: &Path) -> io::Result<()> {
        let mut s = format!(
            "# NANOFreeDisk setup: files and shortcuts the uninstaller removes\nversion={}\nall_users={}\n",
            crate::VERSION,
            u8::from(self.all_users)
        );
        for f in &self.files {
            s.push_str(&format!("file={f}\n"));
        }
        for l in &self.shortcuts {
            s.push_str(&format!("shortcut={}\n", l.display()));
        }
        fs::write(dir.join(MANIFEST), s)
    }
}

fn shortcut_paths(all_users: bool, start_menu: bool, desktop: bool) -> Vec<PathBuf> {
    let name = format!("{PRODUCT}.lnk");
    let mut v = Vec::new();
    if start_menu {
        if let Some(d) = sys::programs_dir(all_users) {
            v.push(d.join(&name));
        }
    }
    if desktop {
        if let Some(d) = sys::desktop_dir(all_users) {
            v.push(d.join(&name));
        }
    }
    v
}

fn file_path(dir: &Path, name: &str) -> PathBuf {
    name.split('/').fold(dir.to_path_buf(), |p, s| p.join(s))
}

/// Install `entries` as `plan` says. `progress` is called at the start of each step.
///
/// # Errors
/// See [`Problem`].
pub fn install(
    plan: &Plan,
    entries: &[Entry],
    progress: &mut dyn FnMut(Step),
) -> Result<Outcome, Problem> {
    progress(Step::Check);
    if entries.is_empty() {
        return Err(Problem::NoPayload);
    }
    if plan.all_users && !sys::is_elevated() {
        return Err(Problem::NeedsAdmin);
    }
    prune_stale();
    if let Some(other) = existing().into_iter().find(|e| !e.ours) {
        return Err(Problem::Foreign(other));
    }
    let running = sys::running_in(&plan.dir);
    if !running.is_empty() {
        return Err(Problem::Running(running));
    }

    // what is there before: a failed install removes only what it added
    let new_dirs = missing_dirs(&plan.dir);
    let old = Manifest::read(&plan.dir);
    let registered = ours().is_some();
    let present: Vec<bool> = entries
        .iter()
        .map(|e| file_path(&plan.dir, &e.name).exists())
        .collect();
    let mut shortcuts = Vec::new();

    let result = install_steps(plan, entries, old.as_ref(), &mut shortcuts, progress);
    if result.is_err() {
        if old.is_none() {
            for (e, _) in entries.iter().zip(&present).filter(|(_, was)| !**was) {
                remove_with_empty_parents(&file_path(&plan.dir, &e.name), &plan.dir);
            }
            let _ = fs::remove_file(plan.dir.join(MANIFEST));
        }
        if !registered {
            let _ = root(plan.all_users).delete_subkey_all(ARP_KEY);
            if plan.shell_menu {
                let _ = nfd_platform::shell_menu::unregister();
            }
            for l in &shortcuts {
                let _ = fs::remove_file(l);
            }
        }
        // deepest first; only empty folders go
        for d in &new_dirs {
            let _ = fs::remove_dir(d);
        }
    }
    result
}

fn install_steps(
    plan: &Plan,
    entries: &[Entry],
    old: Option<&Manifest>,
    shortcuts: &mut Vec<PathBuf>,
    progress: &mut dyn FnMut(Step),
) -> Result<Outcome, Problem> {
    let denied = |e: io::Error| {
        if e.kind() == io::ErrorKind::PermissionDenied {
            Problem::NotWritable(plan.dir.clone())
        } else {
            Problem::from(e)
        }
    };
    fs::create_dir_all(&plan.dir).map_err(denied)?;

    progress(Step::Files);
    payload::extract(entries, &plan.dir).map_err(denied)?;
    let files: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    // an upgrade drops files the new version no longer ships
    if let Some(old) = old {
        for f in old.files.iter().filter(|f| !files.contains(f)) {
            let _ = fs::remove_file(file_path(&plan.dir, f));
        }
        for l in &old.shortcuts {
            let _ = fs::remove_file(l);
        }
    }
    let mut manifest = Manifest {
        all_users: plan.all_users,
        files,
        shortcuts: Vec::new(),
    };
    manifest.write(&plan.dir)?;

    let mut outcome = Outcome::default();
    progress(Step::WebView);
    if sys::webview2_version().is_none() {
        progress(Step::WebViewDownload);
        if let Err(e) = sys::install_webview2() {
            outcome.webview_warning = Some(e.to_string());
        }
    }

    progress(Step::Register);
    register(plan, entries)?;
    let exe = plan.dir.join(APP_EXE);
    if plan.shell_menu {
        let t = plan.lang.text();
        let _ = nfd_platform::shell_menu::register(&exe, t.shell_caption, t.shell_dupes_caption);
    }

    progress(Step::Shortcuts);
    for link in shortcut_paths(plan.all_users, plan.start_menu, plan.desktop) {
        if sys::create_shortcut(&link, &exe, plan.lang.text().tagline).is_ok() {
            shortcuts.push(link.clone());
            manifest.shortcuts.push(link);
        }
    }
    manifest.write(&plan.dir)?;
    Ok(outcome)
}

/// `dir` and its ancestors that do not exist yet, deepest first.
fn missing_dirs(dir: &Path) -> Vec<PathBuf> {
    dir.ancestors()
        .take_while(|d| !d.as_os_str().is_empty() && !d.exists())
        .map(Path::to_path_buf)
        .collect()
}

/// Remove `path`, then its parent folders inside `dir` (not `dir` itself) while
/// they are empty.
fn remove_with_empty_parents(path: &Path, dir: &Path) {
    let _ = fs::remove_file(path);
    let mut parent = path.parent();
    while let Some(p) = parent.filter(|p| p.starts_with(dir) && *p != dir) {
        if fs::remove_dir(p).is_err() {
            break;
        }
        parent = p.parent();
    }
}

fn register(plan: &Plan, entries: &[Entry]) -> io::Result<()> {
    let (key, _) =
        root(plan.all_users).create_subkey_with_flags(ARP_KEY, KEY_ALL_ACCESS | KEY_WOW64_64KEY)?;
    let dir = plan.dir.display().to_string();
    let uninstaller = plan.dir.join(payload::UNINSTALLER);
    let un = format!("\"{}\" --uninstall", uninstaller.display());
    key.set_value("DisplayName", &PRODUCT)?;
    key.set_value("DisplayVersion", &crate::VERSION)?;
    key.set_value("Publisher", &crate::PUBLISHER)?;
    key.set_value("InstallLocation", &dir)?;
    key.set_value(
        "DisplayIcon",
        &format!("\"{}\",0", plan.dir.join(APP_EXE).display()),
    )?;
    key.set_value("UninstallString", &un)?;
    key.set_value("QuietUninstallString", &format!("{un} /S"))?;
    key.set_value("URLInfoAbout", &crate::HOMEPAGE)?;
    key.set_value("HelpLink", &crate::HOMEPAGE)?;
    key.set_value("URLUpdateInfo", &crate::RELEASES)?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    let kb = u32::try_from(installed_size(entries) / 1024).unwrap_or(u32::MAX);
    key.set_value("EstimatedSize", &kb)?;
    key.set_value("Language", &u32::from(plan.lang == Lang::Vi))?;
    key.set_value(MARKER, &"setup")?;
    Ok(())
}

/// Our own installation (from Apps & features), if any.
#[must_use]
pub fn ours() -> Option<Existing> {
    existing().into_iter().find(|e| e.ours)
}

/// Uninstall the copy in `dir`. Runs the app's `--uninstall-cleanup` first
/// (quarantine question, `remove_data`), then removes exactly the files and
/// shortcuts in the manifest, the Apps & features entry and the Explorer menu. When
/// this process runs from `dir`, a short-lived hidden `cmd` deletes it afterwards.
///
/// # Errors
/// See [`Problem`].
pub fn uninstall(dir: &Path, remove_data: bool, silent: bool, lang: Lang) -> Result<(), Problem> {
    let manifest = Manifest::read(dir)
        .ok_or_else(|| Problem::Io(format!("{} not found in {}", MANIFEST, dir.display())))?;
    if manifest.all_users && !sys::is_elevated() {
        return Err(Problem::NeedsAdmin);
    }
    let running: Vec<String> = sys::running_in(dir)
        .into_iter()
        .filter(|n| n != payload::UNINSTALLER)
        .collect();
    if !running.is_empty() {
        return Err(Problem::Running(running));
    }

    let exe = dir.join(APP_EXE);
    if exe.is_file() {
        let mut args = vec!["--uninstall-cleanup", "--lang", lang.code()];
        if silent {
            args.push("--silent");
        }
        if remove_data {
            args.push("--remove-data");
        }
        let _ = sys::run_hidden(&exe, &args);
    }
    let _ = nfd_platform::shell_menu::unregister();

    for l in &manifest.shortcuts {
        let _ = fs::remove_file(l);
    }
    let me = std::env::current_exe().ok();
    let mut leftover_self = None;
    for f in &manifest.files {
        let path = file_path(dir, f);
        if me.as_deref().is_some_and(|m| same_file(m, &path)) {
            leftover_self = Some(path);
            continue;
        }
        // and empty parent folders inside the install folder (rules/…)
        remove_with_empty_parents(&path, dir);
    }
    let _ = fs::remove_file(dir.join(MANIFEST));
    let _ = root(manifest.all_users).delete_subkey_all(ARP_KEY);

    match leftover_self {
        Some(path) => delete_after_exit(&path, dir),
        None => {
            let _ = fs::remove_dir(dir);
        }
    }
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// A running exe cannot delete itself: a hidden `cmd` waits two seconds, deletes
/// it and removes the install folder if it is empty (`rmdir` without `/s`).
fn delete_after_exit(exe: &Path, dir: &Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let script = format!(
        "ping -n 3 127.0.0.1 >nul & del /f /q \"{}\" & rmdir \"{}\"",
        exe.display(),
        dir.display()
    );
    let _ = std::process::Command::new("cmd.exe")
        .arg("/d")
        .arg("/c")
        .raw_arg(&script)
        .current_dir(std::env::temp_dir())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninstaller_path_reads_quoted_and_bare_commands() {
        assert_eq!(
            uninstaller_path(r#""C:\Program Files\NANOFreeDisk\uninstall.exe""#),
            Some(PathBuf::from(
                r"C:\Program Files\NANOFreeDisk\uninstall.exe"
            ))
        );
        assert_eq!(
            uninstaller_path(r#""D:\Apps\NANOFreeDisk\uninstall.exe" --uninstall"#),
            Some(PathBuf::from(r"D:\Apps\NANOFreeDisk\uninstall.exe"))
        );
        assert_eq!(
            uninstaller_path(r"C:\Apps\NFD\Uninstall.EXE /S"),
            Some(PathBuf::from(r"C:\Apps\NFD\Uninstall.EXE"))
        );
        assert_eq!(uninstaller_path("MsiExec.exe /X{1234}"), None);
        assert_eq!(uninstaller_path(""), None);
        assert_eq!(uninstaller_path(r#""unterminated"#), None);
    }

    #[test]
    fn stale_means_the_uninstaller_is_gone() {
        let dir = tempfile::tempdir().expect("tmp");
        let un = dir.path().join("uninstall.exe");
        fs::write(&un, b"x").expect("write");
        assert!(!is_stale(&format!("\"{}\"", un.display())));
        fs::remove_file(&un).expect("remove");
        assert!(is_stale(&format!("\"{}\"", un.display())));
        assert!(!is_stale("MsiExec.exe /X{1234}"));
        assert!(!is_stale(""));
    }

    #[test]
    fn missing_dirs_lists_only_what_does_not_exist_deepest_first() {
        let base = tempfile::tempdir().expect("tmp");
        let base = base.path();
        let target = base.join("a").join("b");
        assert_eq!(missing_dirs(&target), vec![target.clone(), base.join("a")]);
        assert!(missing_dirs(base).is_empty());
    }

    #[test]
    fn remove_with_empty_parents_stops_at_the_install_folder() {
        let base = tempfile::tempdir().expect("tmp");
        let base = base.path();
        let file = base.join("rules").join("x").join("a.toml");
        fs::create_dir_all(base.join("rules").join("x")).expect("mkdir");
        fs::write(&file, b"").expect("write");
        fs::write(base.join("keep.txt"), b"").expect("write");
        remove_with_empty_parents(&file, base);
        assert!(!base.join("rules").exists());
        assert!(base.join("keep.txt").exists());
    }
}
