//! Who is using a file (PHASE3_CLEAN §2.1).
//!
//! [`lockers`] asks the Windows **Restart Manager** (`RmStartSession` →
//! `RmRegisterResources` → `RmGetList`) which processes hold the given files open, so
//! the Clean view can say "in use by Google Chrome" instead of failing halfway. The
//! application never closes or restarts those processes itself (the user can, from
//! the Tasks view or the app's own window).
//!
//! [`running_processes`] lists the executable names of every running process
//! (`CreateToolhelp32Snapshot`) for rules that declare `requires_closed`.
//!
//! Both are read-only. On other platforms they report nothing (the files are not
//! locked in the Windows sense and the rule packs are Windows-only for now).

use std::io;
use std::path::Path;

/// A process that holds a file open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locker {
    pub pid: u32,
    /// Friendly application name as Restart Manager reports it ("Google Chrome",
    /// "Windows Explorer"), or the service name.
    pub name: String,
}

/// Processes holding any of `files` open. Directories are not supported by Restart
/// Manager — pass the files inside them. At most 512 paths are registered per call
/// (the rest are ignored; callers sample large folders).
///
/// # Errors
/// Restart Manager could not start a session (it is limited to 64 per user).
pub fn lockers(files: &[&Path]) -> io::Result<Vec<Locker>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    imp::lockers(&files[..files.len().min(512)])
}

/// Lower-case executable names (`chrome.exe`) of all running processes.
///
/// # Errors
/// The process snapshot could not be taken.
pub fn running_processes() -> io::Result<Vec<String>> {
    imp::running_processes()
}

/// `true` when `exe` (case-insensitive, with or without `.exe`) is in `running`.
#[must_use]
pub fn is_running(running: &[String], exe: &str) -> bool {
    let exe = exe.to_ascii_lowercase();
    let with_ext = if exe.ends_with(".exe") {
        exe
    } else {
        format!("{exe}.exe")
    };
    running.contains(&with_ext)
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_MORE_DATA, ERROR_SUCCESS, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::RestartManager::{
        CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmGetList, RmRegisterResources,
        RmStartSession,
    };

    use super::Locker;

    fn wide_str(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    struct Session(u32);

    impl Drop for Session {
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful RmStartSession.
            unsafe { RmEndSession(self.0) };
        }
    }

    pub(super) fn lockers(files: &[&Path]) -> io::Result<Vec<Locker>> {
        let mut handle = 0u32;
        let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
        // SAFETY: `handle` and `key` are valid for writes; key has room for
        // CCH_RM_SESSION_KEY characters plus the terminator.
        let rc = unsafe { RmStartSession(&mut handle, 0, key.as_mut_ptr()) };
        if rc != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(rc as i32));
        }
        let session = Session(handle);

        let wide: Vec<Vec<u16>> = files
            .iter()
            .map(|p| p.as_os_str().encode_wide().chain([0]).collect())
            .collect();
        let ptrs: Vec<*const u16> = wide.iter().map(|w| w.as_ptr()).collect();
        // SAFETY: `ptrs` holds NUL-terminated strings that outlive the call.
        let rc = unsafe {
            RmRegisterResources(
                session.0,
                ptrs.len() as u32,
                ptrs.as_ptr(),
                0,
                ptr::null(),
                0,
                ptr::null(),
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(rc as i32));
        }

        let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
        for _ in 0..4 {
            let mut needed = 0u32;
            let mut count = infos.len() as u32;
            let mut reasons = 0u32;
            // SAFETY: `infos` has room for `count` entries (null when empty).
            let rc = unsafe {
                RmGetList(
                    session.0,
                    &mut needed,
                    &mut count,
                    if infos.is_empty() {
                        ptr::null_mut()
                    } else {
                        infos.as_mut_ptr()
                    },
                    &mut reasons,
                )
            };
            if rc == ERROR_MORE_DATA {
                infos = vec![RM_PROCESS_INFO::default(); needed as usize + 4];
                continue;
            }
            if rc != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(rc as i32));
            }
            infos.truncate(count as usize);
            let mut out: Vec<Locker> = infos
                .iter()
                .map(|i| {
                    let mut name = wide_str(&i.strAppName);
                    if name.is_empty() {
                        name = wide_str(&i.strServiceShortName);
                    }
                    Locker {
                        pid: i.Process.dwProcessId,
                        name,
                    }
                })
                .collect();
            out.sort_by(|a, b| a.name.cmp(&b.name).then(a.pid.cmp(&b.pid)));
            out.dedup();
            return Ok(out);
        }
        Err(io::Error::other("the list of processes kept changing"))
    }

    pub(super) fn running_processes() -> io::Result<Vec<String>> {
        // SAFETY: plain snapshot call; the handle is closed below.
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snap == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut out = Vec::new();
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        // SAFETY: `entry.dwSize` is set; `snap` is a valid snapshot handle.
        let mut ok = unsafe { Process32FirstW(snap, &mut entry) };
        while ok != 0 {
            out.push(wide_str(&entry.szExeFile).to_ascii_lowercase());
            // SAFETY: as above.
            ok = unsafe { Process32NextW(snap, &mut entry) };
        }
        // SAFETY: `snap` is a handle we own.
        unsafe { CloseHandle(snap) };
        out.sort();
        out.dedup();
        Ok(out)
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::Path;

    use super::Locker;

    pub(super) fn lockers(_files: &[&Path]) -> io::Result<Vec<Locker>> {
        Ok(Vec::new())
    }

    pub(super) fn running_processes() -> io::Result<Vec<String>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_running_matches_with_or_without_extension() {
        let running = vec!["chrome.exe".to_owned(), "explorer.exe".to_owned()];
        assert!(is_running(&running, "Chrome.exe"));
        assert!(is_running(&running, "explorer"));
        assert!(!is_running(&running, "firefox.exe"));
    }

    #[test]
    fn empty_input_has_no_lockers() {
        assert!(lockers(&[]).expect("no session needed").is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn finds_this_process_holding_a_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("locked.txt");
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(b"x").expect("write");
        let found = lockers(&[&path]).expect("restart manager");
        assert!(
            found.iter().any(|l| l.pid == std::process::id()),
            "{found:?}"
        );
        drop(file);
        let after = lockers(&[&path]).expect("restart manager");
        assert!(after.iter().all(|l| l.pid != std::process::id()));
    }

    #[cfg(windows)]
    #[test]
    fn lists_running_processes() {
        let all = running_processes().expect("snapshot");
        assert!(!all.is_empty());
        assert!(all.iter().all(|n| *n == n.to_ascii_lowercase()));
    }
}
