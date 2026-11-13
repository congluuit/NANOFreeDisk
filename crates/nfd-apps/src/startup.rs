//! Programs that start with Windows (PLAN §13.2 E2, `docs/STARTUP.md`), as Task
//! Manager's Startup apps page lists them, and turning them on or off the way it
//! does.
//!
//! Sources: the `Run` keys of this user and of all users (64-bit and the 32-bit
//! `WOW6432Node` view) and the two Startup folders. Whether an entry is on is
//! Task Manager's `Explorer\StartupApproved\{Run, Run32, StartupFolder}` value of
//! the same name: 12 bytes, the first one even (`02`, `06`) when it may run, odd
//! (`03`, `07`) when it was turned off, then the FILETIME of when that happened.
//! No value means on. Turning an entry off writes that value and nothing else:
//! the `Run` value and the shortcut stay exactly where they are, so it is undone
//! by turning it on again (here or in Task Manager). Nothing is ever deleted.
//!
//! All-users entries (`HKEY_LOCAL_MACHINE`, the common Startup folder) need
//! administrator rights: they are listed with their state, and the UI sends the
//! user to Task Manager for them.
//!
//! Scheduled tasks, services, `RunOnce` and Store apps' startup tasks are out of
//! scope for now.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::Scope;
use crate::command;
use crate::registry::{Hive, Registry, Value};

/// `Run` under `Software\…\CurrentVersion`.
pub const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// The 32-bit view of all users' `Run`.
pub const RUN32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
/// Task Manager's on / off flags.
pub const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

/// Where an entry is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// This user's `Run` key.
    UserRun,
    /// All users' `Run` key (64-bit).
    MachineRun,
    /// All users' `Run` key, 32-bit view (`WOW6432Node`).
    MachineRun32,
    /// This user's Startup folder.
    UserFolder,
    /// All users' Startup folder.
    CommonFolder,
}

impl Origin {
    pub const ALL: [Self; 5] = [
        Self::UserRun,
        Self::MachineRun,
        Self::MachineRun32,
        Self::UserFolder,
        Self::CommonFolder,
    ];

    /// The prefix of entry ids (`user-run|Name`).
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::UserRun => "user-run",
            Self::MachineRun => "machine-run",
            Self::MachineRun32 => "machine-run32",
            Self::UserFolder => "user-folder",
            Self::CommonFolder => "common-folder",
        }
    }

    #[must_use]
    pub const fn scope(self) -> Scope {
        match self {
            Self::UserRun | Self::UserFolder => Scope::User,
            _ => Scope::Machine,
        }
    }

    #[must_use]
    pub const fn hive(self) -> Hive {
        match self.scope() {
            Scope::User => Hive::CurrentUser,
            Scope::Machine => Hive::LocalMachine,
        }
    }

    /// The `StartupApproved` subkey Task Manager keeps this source's flags in.
    #[must_use]
    pub fn approved_key(self) -> String {
        let sub = match self {
            Self::UserRun | Self::MachineRun => "Run",
            Self::MachineRun32 => "Run32",
            Self::UserFolder | Self::CommonFolder => "StartupFolder",
        };
        format!(r"{APPROVED}\{sub}")
    }

    /// The `Run` key of a registry source.
    #[must_use]
    pub const fn run_key(self) -> Option<&'static str> {
        match self {
            Self::UserRun | Self::MachineRun => Some(RUN),
            Self::MachineRun32 => Some(RUN32),
            Self::UserFolder | Self::CommonFolder => None,
        }
    }
}

/// Why an entry deserves care before turning it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Caution {
    /// Part of Windows (its program lives under `%SystemRoot%`).
    Windows,
    /// Security software (Windows Security, an antivirus).
    Security,
}

/// One program that starts with Windows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupEntry {
    /// `<origin tag>|<value or file name>`.
    pub id: String,
    /// The value name, or the file name without its extension.
    pub name: String,
    /// The registered command (or the shortcut's target and arguments).
    pub command: String,
    /// The program that runs (for `rundll32`, the DLL it loads).
    pub exe: Option<PathBuf>,
    /// The program is not on the disk (the entry does nothing).
    pub missing: bool,
    /// `CompanyName` / signer of the program, when cheaply read.
    pub publisher: Option<String>,
    pub origin: Origin,
    pub scope: Scope,
    /// The registry key or folder it is registered in.
    pub location: String,
    /// Allowed to run (Task Manager's "Enabled").
    pub enabled: bool,
    /// When it was turned off (Unix seconds), as Task Manager recorded it.
    pub disabled_at: Option<i64>,
    pub caution: Option<Caution>,
    /// The value or file name its `StartupApproved` flag goes by.
    pub approved_name: String,
}

/// Task Manager's flag: `(enabled, turned off at)`. `None` for bytes it never
/// writes (fewer than 4), which count as enabled.
#[must_use]
pub fn parse_approved(bytes: &[u8]) -> Option<(bool, Option<i64>)> {
    if bytes.len() < 4 {
        return None;
    }
    let enabled = bytes[0] & 1 == 0;
    let at = bytes
        .get(4..12)
        .and_then(|b| <[u8; 8]>::try_from(b).ok())
        .map(u64::from_le_bytes)
        .filter(|&ft| ft != 0 && !enabled)
        .and_then(|ft| i64::try_from(ft / 10_000_000).ok())
        .map(|s| s - 11_644_473_600);
    Some((enabled, at))
}

/// The flag Task Manager writes: on is `02` and zeros, off is `03`, zeros and the
/// FILETIME of `now` (Unix seconds).
#[must_use]
pub fn approved_bytes(enabled: bool, now: i64) -> Vec<u8> {
    let mut out = vec![0u8; 12];
    if enabled {
        out[0] = 2;
    } else {
        out[0] = 3;
        let ft = u64::try_from(now.max(0)).unwrap_or(0) * 10_000_000 + 116_444_736_000_000_000;
        out[4..12].copy_from_slice(&ft.to_le_bytes());
    }
    out
}

/// The program a startup command runs. `rundll32 x.dll,Entry` resolves to the
/// DLL; bare Windows tools to `%SystemRoot%\System32`; `%VAR%` expands.
#[must_use]
pub fn resolve_exe(
    raw: &str,
    env: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
    system_root: &Path,
) -> Option<PathBuf> {
    let cmd = command::parse(raw, env, exists, system_root)?;
    if cmd.file_name() == "rundll32.exe" {
        let target = command::expand_vars(&cmd.args, env);
        let target = target.trim().trim_start_matches('"');
        let dll = target
            .split([',', '"'])
            .next()
            .map(str::trim)
            .filter(|d| !d.is_empty())?;
        let p = PathBuf::from(dll);
        if p.components().count() == 1 {
            return Some(system_root.join("System32").join(p));
        }
        return Some(p);
    }
    Some(cmd.exe)
}

/// Executable names (lower case) of security software Windows itself ships.
const SECURITY_NAMES: [&str; 4] = [
    "securityhealthsystray.exe",
    "msascuil.exe",
    "msmpeng.exe",
    "windowsdefender.exe",
];

/// Folder names (lower case) of security products, wherever they live.
const SECURITY_FOLDERS: [&str; 14] = [
    "windows defender",
    "windows security",
    "microsoft security client",
    "avast software",
    "avg",
    "bitdefender",
    "eset",
    "kaspersky lab",
    "malwarebytes",
    "mcafee",
    "norton",
    "sophos",
    "trend micro",
    "f-secure",
];

/// Why `exe` deserves care: security software first (Windows Security's tray
/// icon lives in System32), then anything under `system_root`.
#[must_use]
pub fn caution(exe: &Path, system_root: &Path) -> Option<Caution> {
    let lower = exe.to_string_lossy().to_lowercase().replace('/', "\\");
    let name = lower.rsplit('\\').next().unwrap_or(&lower);
    if SECURITY_NAMES.contains(&name)
        || lower
            .split('\\')
            .any(|part| SECURITY_FOLDERS.contains(&part))
    {
        return Some(Caution::Security);
    }
    let root = system_root
        .to_string_lossy()
        .to_lowercase()
        .replace('/', "\\");
    let root = root.trim_end_matches('\\');
    if !root.is_empty() && lower.starts_with(&format!("{root}\\")) {
        return Some(Caution::Windows);
    }
    None
}

/// What a `.lnk` shortcut points at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shortcut {
    /// The target path (from the link's location info).
    pub target: Option<String>,
    /// The target written with variables (`%ProgramFiles%\…`), when the link has it.
    pub env_target: Option<String>,
    /// The target relative to the shortcut's folder.
    pub relative: Option<String>,
    pub args: Option<String>,
}

fn u16_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// A NUL-terminated ANSI string at `at` (bytes read as Latin-1).
fn ansi_at(b: &[u8], at: usize) -> Option<String> {
    let rest = b.get(at..)?;
    let end = rest.iter().position(|&c| c == 0)?;
    Some(rest[..end].iter().map(|&c| char::from(c)).collect())
}

/// A NUL-terminated UTF-16 string at `at`.
fn utf16z_at(b: &[u8], at: usize, max_units: usize) -> Option<String> {
    let units: Vec<u16> = b
        .get(at..)?
        .chunks_exact(2)
        .take(max_units)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16(&units).ok()
}

/// Read a Shell Link (`.lnk`, MS-SHLLINK): the target from its LinkInfo, its
/// relative path and arguments, and the variable form of the target from its
/// EnvironmentVariableDataBlock. `None` for anything that is not a shell link.
/// Advertised (Windows Installer) shortcuts carry no path: target stays `None`.
#[must_use]
pub fn parse_lnk(b: &[u8]) -> Option<Shortcut> {
    const HEADER: usize = 0x4C;
    if u32_at(b, 0)? != HEADER {
        return None;
    }
    let flags = u32_at(b, 20)?;
    let mut at = HEADER;
    let mut out = Shortcut::default();
    if flags & 0x1 != 0 {
        at += 2 + u16_at(b, at)?;
    }
    if flags & 0x2 != 0 {
        let li = at;
        let size = u32_at(b, li)?;
        let header = u32_at(b, li + 4)?;
        let li_flags = u32_at(b, li + 8)?;
        if li_flags & 0x1 != 0 {
            let base = if header >= 0x24 {
                utf16z_at(b, li + u32_at(b, li + 28)?, 32_768)
            } else {
                ansi_at(b, li + u32_at(b, li + 16)?)
            };
            let suffix = if header >= 0x24 {
                utf16z_at(b, li + u32_at(b, li + 32)?, 32_768)
            } else {
                ansi_at(b, li + u32_at(b, li + 24)?)
            };
            if let Some(base) = base.filter(|s| !s.is_empty()) {
                out.target = Some(format!("{base}{}", suffix.unwrap_or_default()));
            }
        }
        at = li.checked_add(size.max(4))?;
    }
    let unicode = flags & 0x80 != 0;
    // NAME, RELATIVE_PATH, WORKING_DIR, COMMAND_LINE_ARGUMENTS, ICON_LOCATION
    for bit in [0x4, 0x8, 0x10, 0x20, 0x40] {
        if flags & bit == 0 {
            continue;
        }
        let count = u16_at(b, at)?;
        let len = if unicode { count * 2 } else { count };
        let raw = b.get(at + 2..at + 2 + len)?;
        let text = if unicode {
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            raw.iter().map(|&c| char::from(c)).collect()
        };
        match bit {
            0x8 => out.relative = Some(text),
            0x20 => out.args = Some(text).filter(|a| !a.trim().is_empty()),
            _ => {}
        }
        at += 2 + len;
    }
    // extra data blocks: the environment-variable form of the target
    while let Some(size) = u32_at(b, at) {
        if size < 8 {
            break;
        }
        if u32_at(b, at + 4) == Some(0xA000_0001) && size >= 0x314 {
            let unicode_target = utf16z_at(b, at + 8 + 260, 260);
            let ansi_target = ansi_at(b, at + 8);
            out.env_target = unicode_target
                .filter(|s| !s.is_empty())
                .or(ansi_target)
                .filter(|s| !s.is_empty());
        }
        at = at.checked_add(size)?;
    }
    Some(out)
}

/// Everything the listing reads, so tests run without a machine.
pub struct Env<'a> {
    pub reg: &'a dyn Registry,
    /// `%VAR%` values.
    pub var: &'a dyn Fn(&str) -> Option<String>,
    pub exists: &'a dyn Fn(&Path) -> bool,
    /// Files of a folder.
    pub list_dir: &'a dyn Fn(&Path) -> Vec<PathBuf>,
    /// A file's bytes (shortcuts only).
    pub read: &'a dyn Fn(&Path) -> Option<Vec<u8>>,
    pub system_root: PathBuf,
    /// This user's and all users' Startup folders.
    pub user_folder: Option<PathBuf>,
    pub common_folder: Option<PathBuf>,
}

impl std::fmt::Debug for Env<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Env")
            .field("system_root", &self.system_root)
            .field("user_folder", &self.user_folder)
            .field("common_folder", &self.common_folder)
            .finish_non_exhaustive()
    }
}

/// The flag of `name` in a `StartupApproved` value list.
fn approved_state(values: Option<&[(String, Value)]>, name: &str) -> (bool, Option<i64>) {
    values
        .and_then(|v| v.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)))
        .and_then(|(_, v)| match v {
            Value::Binary(b) | Value::Other(_, b) => parse_approved(b),
            _ => None,
        })
        .unwrap_or((true, None))
}

/// Files of a Startup folder that start something (`desktop.ini` and hidden
/// helpers left out).
fn startup_files(env: &Env<'_>, dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = (env.list_dir)(dir)
        .into_iter()
        .filter(|f| {
            f.file_name().is_some_and(|n| {
                let n = n.to_string_lossy().to_lowercase();
                n != "desktop.ini" && !n.starts_with('.') && !n.starts_with("~$")
            })
        })
        .collect();
    files.sort();
    files
}

/// Every startup entry of the five sources (publishers left empty: see
/// [`fill_publishers`]).
#[must_use]
pub fn list(env: &Env<'_>) -> Vec<StartupEntry> {
    let mut out = Vec::new();
    for origin in Origin::ALL {
        let hive = origin.hive();
        let approved = env.reg.values(hive, &origin.approved_key());
        let flag = |name: &str| approved_state(approved.as_deref(), name);
        if let Some(key) = origin.run_key() {
            for (name, value) in env.reg.values(hive, key).unwrap_or_default() {
                let Some(raw) = value.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
                    continue;
                };
                if name.is_empty() {
                    continue; // the key's default value starts nothing
                }
                let exe = resolve_exe(raw, env.var, env.exists, &env.system_root);
                let (enabled, disabled_at) = flag(&name);
                out.push(entry(
                    env,
                    origin,
                    &name,
                    name.clone(),
                    raw.to_owned(),
                    exe,
                    format!(r"{}\{key}", hive.name()),
                    (enabled, disabled_at),
                ));
            }
            continue;
        }
        let folder = match origin {
            Origin::UserFolder => env.user_folder.as_deref(),
            _ => env.common_folder.as_deref(),
        };
        let Some(folder) = folder else { continue };
        for file in startup_files(env, folder) {
            let Some(file_name) = file.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let stem = file
                .file_stem()
                .map_or_else(|| file_name.clone(), |s| s.to_string_lossy().into_owned());
            let is_lnk = file
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk"));
            let (command, exe) = if is_lnk {
                let link = (env.read)(&file)
                    .and_then(|b| parse_lnk(&b))
                    .unwrap_or_default();
                let target = link
                    .env_target
                    .as_deref()
                    .map(|t| command::expand_vars(t, env.var))
                    .or_else(|| link.target.clone())
                    .or_else(|| {
                        link.relative
                            .as_deref()
                            .map(|r| folder.join(r).to_string_lossy().into_owned())
                    });
                let command = match (&target, &link.args) {
                    (Some(t), Some(a)) => format!("\"{t}\" {a}"),
                    (Some(t), None) => format!("\"{t}\""),
                    (None, _) => file.to_string_lossy().into_owned(),
                };
                let exe = target.as_deref().and_then(|t| {
                    let line = match &link.args {
                        Some(a) => format!("\"{t}\" {a}"),
                        None => format!("\"{t}\""),
                    };
                    resolve_exe(&line, env.var, env.exists, &env.system_root)
                });
                (command, exe)
            } else {
                (format!("\"{}\"", file.display()), Some(file.clone()))
            };
            let (enabled, disabled_at) = flag(&file_name);
            out.push(entry(
                env,
                origin,
                &file_name,
                stem,
                command,
                exe,
                folder.to_string_lossy().into_owned(),
                (enabled, disabled_at),
            ));
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn entry(
    env: &Env<'_>,
    origin: Origin,
    approved_name: &str,
    name: String,
    command: String,
    exe: Option<PathBuf>,
    location: String,
    (enabled, disabled_at): (bool, Option<i64>),
) -> StartupEntry {
    let missing = exe
        .as_deref()
        .is_some_and(|e| e.is_absolute() && !(env.exists)(e));
    StartupEntry {
        id: format!("{}|{approved_name}", origin.tag()),
        name,
        command,
        caution: exe.as_deref().and_then(|e| caution(e, &env.system_root)),
        missing,
        exe,
        publisher: None,
        origin,
        scope: origin.scope(),
        location,
        enabled,
        disabled_at: if enabled { None } else { disabled_at },
        approved_name: approved_name.to_owned(),
    }
}

/// The publisher of each entry's program (signer, else `CompanyName` with ` *`),
/// local drives only. Windows only; elsewhere nothing changes.
pub fn fill_publishers(entries: &mut [StartupEntry]) {
    #[cfg(windows)]
    {
        let drives = crate::icons::local_drives();
        for e in entries.iter_mut() {
            if let Some(exe) = e
                .exe
                .as_deref()
                .filter(|p| !e.missing && crate::icons::is_local(p, &drives))
            {
                e.publisher = nfd_tasks::publisher_label(&exe.to_string_lossy());
            }
        }
    }
    #[cfg(not(windows))]
    let _ = entries;
}

/// The Startup folders of this machine: `%APPDATA%\…\Startup` and
/// `%ProgramData%\…\StartUp`.
#[must_use]
pub fn startup_folders(var: &dyn Fn(&str) -> Option<String>) -> (Option<PathBuf>, Option<PathBuf>) {
    let tail = r"Microsoft\Windows\Start Menu\Programs\Startup";
    (
        var("APPDATA").map(|a| Path::new(&a).join(tail)),
        var("ProgramData").map(|a| Path::new(&a).join(tail)),
    )
}

/// Run `f` with this machine's registry, files and folders.
fn with_live_env<T>(f: impl FnOnce(&Env<'_>) -> T) -> T {
    let var = |k: &str| std::env::var(k).ok();
    let (user_folder, common_folder) = startup_folders(&var);
    let env = Env {
        reg: &crate::registry::LiveRegistry,
        var: &var,
        exists: &|p: &Path| p.exists(),
        list_dir: &|d: &Path| {
            std::fs::read_dir(d)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                        .map(|e| e.path())
                        .take(500)
                        .collect()
                })
                .unwrap_or_default()
        },
        read: &|p: &Path| {
            // a shortcut is a few KB; anything big is not one
            let len = std::fs::metadata(p).ok()?.len();
            (len <= 1 << 20).then(|| std::fs::read(p).ok()).flatten()
        },
        system_root: crate::runner::system_root(),
        user_folder,
        common_folder,
    };
    f(&env)
}

/// The machine's startup entries, with publishers.
#[must_use]
pub fn list_live() -> Vec<StartupEntry> {
    let mut entries = with_live_env(list);
    fill_publishers(&mut entries);
    entries
}

/// Why an entry cannot be changed from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetError {
    /// An all-users entry: Task Manager (as administrator) changes it.
    NeedsAdmin,
}

/// The `StartupApproved` key, value name and bytes that turn `entry` on or off.
/// All-users entries are refused.
///
/// # Errors
/// [`SetError::NeedsAdmin`] for all-users entries.
pub fn flag_write(
    entry: &StartupEntry,
    enabled: bool,
    now: i64,
) -> Result<(Hive, String, String, Value), SetError> {
    if entry.scope != Scope::User {
        return Err(SetError::NeedsAdmin);
    }
    Ok((
        entry.origin.hive(),
        entry.origin.approved_key(),
        entry.approved_name.clone(),
        Value::Binary(approved_bytes(enabled, now)),
    ))
}

/// Why turning an entry on or off failed.
#[derive(Debug)]
pub enum ChangeError {
    /// No such entry any more (the list changed since it was shown).
    NotFound,
    /// An all-users entry: Task Manager (as administrator) changes it.
    NeedsAdmin,
    /// The registry refused the write.
    Io(std::io::Error),
}

impl std::fmt::Display for ChangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("the startup entry is gone"),
            Self::NeedsAdmin => f.write_str("an all-users entry needs administrator rights"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Turn the entry `id` of the machine on or off, the way Task Manager does. The
/// entry is looked up again here, never trusted from the caller. Returns the
/// entry as it is now.
///
/// # Errors
/// Not found, all-users, or the registry write failed.
pub fn set_enabled_live(id: &str, enabled: bool, now: i64) -> Result<StartupEntry, ChangeError> {
    let mut updated = with_live_env(|env| {
        let entry = list(env)
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(ChangeError::NotFound)?;
        let (hive, key, name, value) =
            flag_write(&entry, enabled, now).map_err(|_| ChangeError::NeedsAdmin)?;
        nfd_platform::registry_write::set_value(hive, &key, &name, &value)
            .map_err(ChangeError::Io)?;
        list(env)
            .into_iter()
            .find(|e| e.id == id)
            .ok_or(ChangeError::NotFound)
    })?;
    fill_publishers(std::slice::from_mut(&mut updated));
    Ok(updated)
}

/// Icons of the entries' programs (entries without one are left out), read the
/// way the Programs list reads them.
#[must_use]
pub fn icons(entries: &[StartupEntry]) -> Vec<(String, crate::icons::ProgramIcon)> {
    let programs: Vec<crate::Program> = entries
        .iter()
        .filter(|e| !e.missing)
        .filter_map(|e| {
            Some(crate::Program {
                id: e.id.clone(),
                name: e.name.clone(),
                publisher: None,
                version: None,
                source: crate::Source::Win32,
                scope: e.scope,
                arch: None,
                install_location: None,
                uninstall: None,
                quiet_uninstall: None,
                icon: Some(e.exe.clone()?),
                icon_index: 0,
                logo: None,
                installed_at: None,
                size: 0,
                size_measured: false,
                product_code: None,
                key: None,
                package: None,
                url: None,
                caution: None,
                blocked: None,
                broken: false,
            })
        })
        .collect();
    crate::icons::program_icons(&programs)
}

#[cfg(test)]
mod tests;
