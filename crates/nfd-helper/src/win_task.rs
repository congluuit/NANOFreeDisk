//! Windows side of "start without the Windows prompt" (ADR 0003): the account's
//! SID, the administrator-only copy of the helper and its ACLs, and the task
//! through `schtasks.exe`.
//!
//! The app (unelevated) only reads here: [`state`] and [`run_task`]. Everything
//! that writes ([`register`], [`remove`], [`remove_all`]) needs administrator rights
//! and runs in the elevated helper or an elevated uninstaller.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
    GetTokenInformation, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_READ, MOVEFILE_DELAY_UNTIL_REBOOT,
    MoveFileExW,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath};
use windows_sys::core::PWSTR;

use crate::task::{
    COPY_EXE, COPY_ROOT, COPY_ROOT_SDDL, TaskState, copy_dir_sddl, task_name, task_xml, valid_sid,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// A NUL-terminated wide string from Windows.
///
/// # Safety
/// `p` must be null or point to a NUL-terminated UTF-16 string.
unsafe fn from_wide_ptr(p: *const u16) -> OsString {
    if p.is_null() {
        return OsString::new();
    }
    let mut n = 0;
    // SAFETY: NUL-terminated per the contract.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: `n` elements were just read.
    OsString::from_wide(unsafe { std::slice::from_raw_parts(p, n) })
}

/// String SID of the account a process runs as (`None`: this process).
pub fn process_sid(pid: Option<u32>) -> Option<String> {
    // SAFETY: query-only handles closed below (the pseudo handle needs no closing);
    // the token buffer is sized by the first call and 8-byte aligned.
    unsafe {
        let process = match pid {
            None => GetCurrentProcess(),
            Some(p) => {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, p);
                if h.is_null() {
                    return None;
                }
                h
            }
        };
        let mut token: HANDLE = std::ptr::null_mut();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token) != 0;
        if pid.is_some() {
            CloseHandle(process);
        }
        if !opened {
            return None;
        }
        let mut len = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len);
        let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
        let got =
            GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) != 0;
        CloseHandle(token);
        if !got {
            return None;
        }
        let user = &*buf.as_ptr().cast::<TOKEN_USER>();
        let mut s: PWSTR = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut s) == 0 {
            return None;
        }
        let text = from_wide_ptr(s).to_string_lossy().into_owned();
        LocalFree(s.cast());
        Some(text).filter(|t| valid_sid(t))
    }
}

/// This process's account (read once).
pub fn own_sid() -> Option<String> {
    static SID: OnceLock<Option<String>> = OnceLock::new();
    SID.get_or_init(|| process_sid(None)).clone()
}

/// The machine's Program Files (the known folder: never an environment variable,
/// which the user could point anywhere).
fn program_files() -> Option<PathBuf> {
    let mut p: PWSTR = std::ptr::null_mut();
    // SAFETY: the out string is freed with CoTaskMemFree, also on failure (documented).
    unsafe {
        let hr = SHGetKnownFolderPath(&FOLDERID_ProgramFiles, 0, std::ptr::null_mut(), &mut p);
        let out = (hr >= 0 && !p.is_null()).then(|| PathBuf::from(from_wide_ptr(p)));
        CoTaskMemFree(p.cast());
        out
    }
}

/// Where an account's copy lives.
#[derive(Debug, Clone)]
pub struct CopyPaths {
    /// `<Program Files>\NANOFreeDisk Helper`
    pub root: PathBuf,
    /// `…\<SID>`
    pub dir: PathBuf,
    /// `…\<SID>\nanofreedisk.exe`
    pub exe: PathBuf,
}

pub fn copy_paths(sid: &str) -> Option<CopyPaths> {
    if !valid_sid(sid) {
        return None;
    }
    let root = program_files()?.join(COPY_ROOT);
    let dir = root.join(sid);
    let exe = dir.join(COPY_EXE);
    Some(CopyPaths { root, dir, exe })
}

/// Windows paths compare without case.
pub fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
}

pub fn file_sha256(path: &Path) -> Option<[u8; 32]> {
    nfd_sensors::win::pawnio::sha256(&std::fs::read(path).ok()?)
}

/// SHA-256 of this executable (read once).
pub fn own_sha256() -> Option<[u8; 32]> {
    static HASH: OnceLock<Option<[u8; 32]>> = OnceLock::new();
    *HASH.get_or_init(|| file_sha256(&std::env::current_exe().ok()?))
}

/// Whether the task is set up for this account, and for this build. Reads only.
pub fn state() -> TaskState {
    let Some(p) = own_sid().and_then(|s| copy_paths(&s)) else {
        return TaskState::Off;
    };
    if !p.exe.is_file() {
        return TaskState::Off;
    }
    match (file_sha256(&p.exe), own_sha256()) {
        (Some(a), Some(b)) if a == b => TaskState::On,
        _ => TaskState::Stale,
    }
}

/// Whether this process has administrator rights.
pub fn elevated() -> bool {
    nfd_monitor::win::is_elevated()
}

/// Lock this process's own image: open the running executable with read-only
/// sharing, so it can no longer be renamed, replaced or written. The file is found
/// by the image's current name (Windows follows renames) and that name is checked
/// again once the handle is open: a file swapped in under the old name in between
/// is refused. `None` when it cannot be proven.
pub fn lock_own_image() -> Option<File> {
    // SAFETY: no arguments.
    let me = unsafe { GetCurrentProcessId() };
    let before = crate::win::image_of(me)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&before)
        .ok()?;
    let after = crate::win::image_of(me)?;
    before.eq_ignore_ascii_case(&after).then_some(file)
}

fn system32() -> PathBuf {
    let mut buf = [0u16; 260];
    // SAFETY: buffer with its length.
    let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return PathBuf::from(r"C:\Windows\System32");
    }
    PathBuf::from(OsString::from_wide(&buf[..n]))
}

/// `<System32>\schtasks.exe` (the real System32, not `%SystemRoot%`), no window.
fn schtasks(args: &[&OsStr]) -> Result<(), String> {
    let out = std::process::Command::new(system32().join("schtasks.exe"))
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("schtasks: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let mut why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    if why.is_empty() {
        why = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    }
    Err(format!(
        "schtasks exit {}: {why}",
        out.status.code().unwrap_or(-1)
    ))
}

fn task_exists(name: &str) -> bool {
    schtasks(&["/query".as_ref(), "/tn".as_ref(), name.as_ref()]).is_ok()
}

/// Start this account's task (the app's side; needs no rights beyond the task's
/// run permission).
pub fn run_task(sid: &str) -> Result<(), String> {
    schtasks(&["/run".as_ref(), "/tn".as_ref(), task_name(sid).as_ref()])
}

struct Sd(PSECURITY_DESCRIPTOR);

impl Drop for Sd {
    fn drop(&mut self) {
        // SAFETY: allocated by the SDDL conversion.
        unsafe { LocalFree(self.0) };
    }
}

fn sd(sddl: &str) -> Result<Sd, String> {
    let s = wide(sddl.as_ref());
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: NUL-terminated SDDL; freed by `Sd`.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            s.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(format!("SDDL {sddl}: {}", std::io::Error::last_os_error()));
    }
    Ok(Sd(sd))
}

/// Set the owner and a protected (not inherited) DACL from `sddl`; children get
/// the inheritable entries.
fn protect(path: &Path, sddl: &str) -> Result<(), String> {
    let sd = sd(sddl)?;
    let (mut present, mut defaulted) = (0, 0);
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut owner: PSID = std::ptr::null_mut();
    let w = wide(path.as_os_str());
    // SAFETY: the DACL / owner point into `sd`, alive for the call.
    let rc = unsafe {
        GetSecurityDescriptorDacl(sd.0, &mut present, &mut dacl, &mut defaulted);
        GetSecurityDescriptorOwner(sd.0, &mut owner, &mut defaulted);
        let all = DACL_SECURITY_INFORMATION
            | PROTECTED_DACL_SECURITY_INFORMATION
            | OWNER_SECURITY_INFORMATION;
        let rc = SetNamedSecurityInfoW(
            w.as_ptr(),
            SE_FILE_OBJECT,
            all,
            owner,
            std::ptr::null_mut(),
            dacl,
            std::ptr::null(),
        );
        if rc == 0 {
            rc
        } else {
            // the owner is Administrators already when an administrator created it
            SetNamedSecurityInfoW(
                w.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null(),
            )
        }
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "ACL of {}: {}",
            path.display(),
            std::io::Error::from_raw_os_error(rc as i32)
        ))
    }
}

/// A real folder: not a file, not a junction or symbolic link.
fn plain_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|m| m.is_dir() && m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0)
}

/// Create `path` with `sddl` (or re-apply it to an existing plain folder).
fn ensure_dir(path: &Path, sddl: &str) -> Result<(), String> {
    if std::fs::symlink_metadata(path).is_err() {
        let sd = sd(sddl)?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: 0,
        };
        let w = wide(path.as_os_str());
        // SAFETY: NUL-terminated path, security attributes alive for the call.
        if unsafe { CreateDirectoryW(w.as_ptr(), &sa) } == 0 {
            return Err(format!(
                "{}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
    }
    if !plain_dir(path) {
        return Err(format!("{} is not a plain folder", path.display()));
    }
    protect(path, sddl)
}

/// Delete `path`, or — while it runs — rename it and delete it at the next restart.
fn set_aside(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => {}
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let old = path.with_extension(format!("old-{nanos:x}"));
    std::fs::rename(path, &old).map_err(|e| format!("{}: {e}", path.display()))?;
    let w = wide(old.as_os_str());
    // SAFETY: NUL-terminated path; no new name = delete at restart.
    unsafe { MoveFileExW(w.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
    Ok(())
}

/// Leftovers of earlier copies (`*.old-*`, `*.new`, `task.xml`).
fn sweep(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if name.contains(".old-") || name.ends_with(".new") || name == "task.xml" {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn put_file(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let new = target.with_extension("new");
    let _ = std::fs::remove_file(&new);
    std::fs::write(&new, bytes).map_err(|e| format!("{}: {e}", new.display()))?;
    set_aside(target)?;
    std::fs::rename(&new, target).map_err(|e| format!("{}: {e}", target.display()))
}

/// Set up the task for the account this (elevated) process runs as: copy `own` —
/// the helper's locked image ([`lock_own_image`]) — into the account's
/// administrator-only folder and register the task. `own` is not needed when the
/// helper already runs from the copy (the task is just registered again).
///
/// # Errors
/// The reason as text; the task is not registered then.
pub fn register(own: Option<&File>) -> Result<(), String> {
    let sid = own_sid().ok_or("no account SID")?;
    let p = copy_paths(&sid).ok_or("Program Files not found")?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    if !p.root.parent().is_some_and(plain_dir) {
        return Err("Program Files is not a plain folder".into());
    }
    ensure_dir(&p.root, COPY_ROOT_SDDL)?;
    ensure_dir(&p.dir, &copy_dir_sddl(&sid))?;
    if !same_path(&me, &p.exe) {
        let mut f = own.ok_or("the helper's own image is not locked")?;
        let mut bytes = Vec::new();
        f.seek(SeekFrom::Start(0))
            .and_then(|_| f.read_to_end(&mut bytes))
            .map_err(|e| format!("own image: {e}"))?;
        let want = nfd_sensors::win::pawnio::sha256(&bytes).ok_or("SHA-256")?;
        put_file(&p.exe, &bytes)?;
        if file_sha256(&p.exe) != Some(want) {
            let _ = set_aside(&p.exe);
            return Err("the copy does not match the helper".into());
        }
    }
    sweep(&p.dir);
    let (exe, dir) = (p.exe.to_str().ok_or("path")?, p.dir.to_str().ok_or("path")?);
    let xml = task_xml(&sid, exe, dir).ok_or("the copy's path cannot be used in a task")?;
    // inside the protected folder: nobody else can change it before schtasks reads it
    let file = p.dir.join("task.xml");
    let mut data = vec![0xFF, 0xFE];
    data.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&file, data).map_err(|e| format!("{}: {e}", file.display()))?;
    let name = task_name(&sid);
    let r = schtasks(&[
        "/create".as_ref(),
        "/tn".as_ref(),
        name.as_ref(),
        "/xml".as_ref(),
        file.as_os_str(),
        "/f".as_ref(),
    ]);
    let _ = std::fs::remove_file(&file);
    r
}

/// Delete the task first (nothing can start the copy any more), then the copy.
fn remove_for(sid: &str, p: &CopyPaths) -> Result<(), String> {
    let name = task_name(sid);
    if task_exists(&name) {
        schtasks(&[
            "/delete".as_ref(),
            "/tn".as_ref(),
            name.as_ref(),
            "/f".as_ref(),
        ])?;
    }
    let mut first = Ok(());
    if let Ok(rd) = std::fs::read_dir(&p.dir) {
        for e in rd.flatten() {
            if let Err(x) = set_aside(&e.path()) {
                first = first.and(Err(x));
            }
        }
    }
    if p.dir.exists() && std::fs::remove_dir(&p.dir).is_err() {
        // a running copy was renamed: the folder goes with it at the next restart
        let w = wide(p.dir.as_os_str());
        // SAFETY: NUL-terminated path; no new name = delete at restart (when empty).
        unsafe { MoveFileExW(w.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
    }
    let _ = std::fs::remove_dir(&p.root); // only when no other account has a copy
    first
}

/// Remove this account's task and copy. Needs administrator rights.
///
/// # Errors
/// The reason as text.
pub fn remove() -> Result<(), String> {
    let sid = own_sid().ok_or("no account SID")?;
    let p = copy_paths(&sid).ok_or("Program Files not found")?;
    remove_for(&sid, &p)
}

/// Remove every account's task and copy (an elevated, per-machine uninstall).
///
/// # Errors
/// The first reason as text; the others are still removed.
pub fn remove_all() -> Result<(), String> {
    let root = program_files()
        .ok_or("Program Files not found")?
        .join(COPY_ROOT);
    let Ok(rd) = std::fs::read_dir(&root) else {
        return Ok(());
    };
    let mut first = Ok(());
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(p) = copy_paths(&name) {
            first = first.and(remove_for(&name, &p));
        }
    }
    first
}
