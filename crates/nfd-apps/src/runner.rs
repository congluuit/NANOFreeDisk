//! Starting the vendor's uninstaller and waiting for it (`docs/UNINSTALLER.md`
//! §2.3), Store packages, and the elevated leftover step.
//!
//! The uninstaller is started through the shell (`ShellExecuteExW`) so its own
//! manifest decides whether Windows asks for administrator rights. Many
//! uninstallers copy themselves to Temp, start the copy and exit at once (NSIS,
//! Inno Setup): the wait follows the whole tree of processes the uninstaller
//! started — by parent PID and creation time, so a reused PID is never mistaken
//! for one of them — until none is left, or until the user says it is done.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::command::Command;
use crate::inventory::{self, AppxRow};

/// How a wait ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// Every process of the tree has exited.
    Done,
    /// `stop` was set (the user said the uninstaller is finished, or cancelled).
    Stopped,
    /// `timeout` passed.
    TimedOut,
}

/// One process of the tree: PID and creation time (FILETIME). `exact` = the
/// creation time is known; else it is a lower bound (the launch time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tracked {
    pid: u32,
    created: u64,
    exact: bool,
}

/// `(pid, ppid, created)` of every process.
type Snapshot = Vec<(u32, u32, u64)>;

/// Grow `tracked` with the descendants found in `procs` and count the live ones
/// (pure: tested without processes).
fn follow(tracked: &mut Vec<Tracked>, procs: &Snapshot) -> usize {
    // pin the root's real creation time once it is seen
    for t in tracked.iter_mut().filter(|t| !t.exact) {
        if let Some(&(_, _, c)) = procs
            .iter()
            .find(|(pid, _, c)| *pid == t.pid && *c >= t.created)
        {
            t.created = c;
            t.exact = true;
        }
    }
    loop {
        let mut grew = false;
        for &(pid, ppid, created) in procs {
            let known = tracked.iter().any(|t| t.pid == pid && t.created == created);
            let child = tracked
                .iter()
                .any(|t| t.pid == ppid && created >= t.created);
            if !known && child {
                tracked.push(Tracked {
                    pid,
                    created,
                    exact: true,
                });
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    tracked
        .iter()
        .filter(|t| {
            procs.iter().any(|(pid, _, c)| {
                *pid == t.pid
                    && if t.exact {
                        *c == t.created
                    } else {
                        *c >= t.created
                    }
            })
        })
        .count()
}

/// Wait for the process `pid` (started at `launched`, FILETIME) and everything it
/// starts. `list` lists the processes; `tick(alive)` reports progress.
pub fn wait_tree(
    pid: u32,
    launched: u64,
    list: &dyn Fn() -> Snapshot,
    stop: &AtomicBool,
    poll: Duration,
    timeout: Duration,
    tick: &mut dyn FnMut(usize),
) -> Waited {
    let started = std::time::Instant::now();
    let mut tracked = vec![Tracked {
        pid,
        // a little slack: clocks of the launch and of the kernel's record differ
        created: launched.saturating_sub(20_000_000),
        exact: false,
    }];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Waited::Stopped;
        }
        let alive = follow(&mut tracked, &list());
        tick(alive);
        if alive == 0 {
            return Waited::Done;
        }
        if started.elapsed() >= timeout {
            return Waited::TimedOut;
        }
        let step = std::time::Instant::now();
        while step.elapsed() < poll {
            if stop.load(Ordering::Relaxed) {
                return Waited::Stopped;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// A Store package full name (`Publisher.App_1.2.3.0_x64__hash`): nothing a
/// PowerShell command line could misread.
#[must_use]
pub fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 256
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '~'))
}

pub use imp::*;

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::ptr;

    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessId, INFINITE, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, SHLoadIndirectString,
        ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::{AppxRow, Command, Snapshot, inventory};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Now, as FILETIME (100 ns since 1601).
    #[must_use]
    pub fn now_filetime() -> u64 {
        let since_1970 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_nanos() / 100).unwrap_or(u64::MAX));
        since_1970.saturating_add(116_444_736_000_000_000)
    }

    /// A process started through the shell.
    #[derive(Debug)]
    pub struct Started {
        pub pid: u32,
        /// FILETIME right before the start.
        pub launched: u64,
        handle: HANDLE,
    }

    // SAFETY: a process handle may be used from any thread.
    unsafe impl Send for Started {}

    impl Started {
        /// Wait for this process only; its exit code.
        #[must_use]
        pub fn wait(&self) -> Option<u32> {
            // SAFETY: valid handle owned by self.
            unsafe {
                if WaitForSingleObject(self.handle, INFINITE) != WAIT_OBJECT_0 {
                    return None;
                }
                let mut code = 0u32;
                (GetExitCodeProcess(self.handle, &mut code) != 0).then_some(code)
            }
        }
    }

    impl Drop for Started {
        fn drop(&mut self) {
            // SAFETY: owned handle, closed once.
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    /// Start `exe args` through the shell (`verb` = `None` or `"runas"`).
    /// `ErrorKind::Interrupted` when the user declined the UAC prompt.
    ///
    /// # Errors
    /// The program could not be started.
    pub fn shell_start(
        exe: &Path,
        args: &str,
        verb: Option<&str>,
        owner: isize,
    ) -> io::Result<Started> {
        let file = wide(exe.as_os_str());
        let params = wide(std::ffi::OsStr::new(args));
        let verb_w = verb.map(|v| wide(std::ffi::OsStr::new(v)));
        let dir = exe.parent().map(|d| wide(d.as_os_str()));
        // SAFETY: plain C struct, zero is a valid initial state.
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.hwnd = owner as _;
        info.lpVerb = verb_w.as_ref().map_or(ptr::null(), |v| v.as_ptr());
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.lpDirectory = dir.as_ref().map_or(ptr::null(), |d| d.as_ptr());
        info.nShow = SW_SHOWNORMAL;
        let launched = now_filetime();
        // SAFETY: every pointer stays alive for the call.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(ERROR_CANCELLED as i32) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "the administrator prompt was declined",
                ));
            }
            return Err(e);
        }
        if info.hProcess.is_null() {
            return Err(io::Error::other(
                "the program started without a process to follow",
            ));
        }
        // SAFETY: valid process handle.
        let pid = unsafe { GetProcessId(info.hProcess) };
        Ok(Started {
            pid,
            launched,
            handle: info.hProcess,
        })
    }

    /// Start a program's uninstaller.
    ///
    /// # Errors
    /// See [`shell_start`].
    pub fn start_uninstaller(cmd: &Command, owner: isize) -> io::Result<Started> {
        shell_start(&cmd.exe, &cmd.args, None, owner)
    }

    /// Every process as `(pid, ppid, created)`.
    #[must_use]
    pub fn process_snapshot() -> Snapshot {
        nfd_tasks::processes()
            .into_iter()
            .map(|p| (p.pid, p.ppid, p.create_time))
            .collect()
    }

    /// Image paths of the running processes that can be queried.
    #[must_use]
    pub fn running_images() -> Vec<PathBuf> {
        nfd_tasks::processes()
            .into_iter()
            .filter_map(|p| nfd_tasks::image_path_of(p.pid))
            .map(PathBuf::from)
            .collect()
    }

    fn powershell() -> std::process::Command {
        let sys = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let mut c = std::process::Command::new(
            Path::new(&sys).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
        );
        c.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
        ])
        .creation_flags(CREATE_NO_WINDOW);
        c
    }

    /// The Store packages of this user (`Get-AppxPackage`).
    #[must_use]
    pub fn store_rows() -> Vec<AppxRow> {
        powershell()
            .arg(inventory::APPX_SCRIPT)
            .output()
            .ok()
            .map(|o| inventory::parse_appx_json(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default()
    }

    /// Resolve an `@{…? ms-resource://…}` reference.
    #[must_use]
    pub fn resolve_indirect(source: &str) -> Option<String> {
        let src = wide(std::ffi::OsStr::new(source));
        let mut buf = vec![0u16; 1024];
        // SAFETY: buffer and its length in characters.
        let hr = unsafe {
            SHLoadIndirectString(
                src.as_ptr(),
                buf.as_mut_ptr(),
                buf.len() as u32,
                ptr::null(),
            )
        };
        if hr < 0 {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..end]);
        (!s.trim().is_empty()).then_some(s)
    }

    /// Remove a Store package of this user (`Remove-AppxPackage`).
    ///
    /// # Errors
    /// An invalid name, or PowerShell reported a failure.
    pub fn remove_store_package(full_name: &str) -> io::Result<()> {
        if !super::valid_package_name(full_name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a package name",
            ));
        }
        let out = powershell()
            .arg(format!(
                "$ErrorActionPreference='Stop'; Remove-AppxPackage -Package '{full_name}'"
            ))
            .output()?;
        if out.status.success() {
            Ok(())
        } else {
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            Err(io::Error::other(if msg.is_empty() {
                "Remove-AppxPackage failed".to_owned()
            } else {
                msg
            }))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::{Path, PathBuf};

    use super::{AppxRow, Command, Snapshot};

    fn none() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "Windows only")
    }

    #[derive(Debug)]
    pub struct Started {
        pub pid: u32,
        pub launched: u64,
    }

    impl Started {
        #[must_use]
        pub fn wait(&self) -> Option<u32> {
            None
        }
    }

    #[must_use]
    pub fn now_filetime() -> u64 {
        0
    }
    pub fn shell_start(_: &Path, _: &str, _: Option<&str>, _: isize) -> io::Result<Started> {
        Err(none())
    }
    pub fn start_uninstaller(_: &Command, _: isize) -> io::Result<Started> {
        Err(none())
    }
    #[must_use]
    pub fn process_snapshot() -> Snapshot {
        Vec::new()
    }
    #[must_use]
    pub fn running_images() -> Vec<PathBuf> {
        Vec::new()
    }
    #[must_use]
    pub fn store_rows() -> Vec<AppxRow> {
        Vec::new()
    }
    #[must_use]
    pub fn resolve_indirect(_: &str) -> Option<String> {
        None
    }
    pub fn remove_store_package(_: &str) -> std::io::Result<()> {
        Err(none())
    }
}

/// Run the elevated leftover step (`exe --elevated-leftovers <req> <resp>`) through
/// UAC and wait for it; `Interrupted` when the prompt was declined.
///
/// # Errors
/// It could not start, or it wrote no response.
pub fn run_elevated_step(
    exe: &Path,
    request: &crate::elevated::Request,
    dir: &Path,
    owner: isize,
) -> std::io::Result<crate::elevated::Response> {
    std::fs::create_dir_all(dir)?;
    let req = dir.join(format!("{}-request.json", request.op_id));
    let resp = dir.join(format!("{}-response.json", request.op_id));
    let _ = std::fs::remove_file(&resp);
    std::fs::write(
        &req,
        serde_json::to_vec_pretty(request).map_err(std::io::Error::other)?,
    )?;
    let args = format!(
        "{} \"{}\" \"{}\"",
        crate::elevated::FLAG,
        req.display(),
        resp.display()
    );
    let started = shell_start(exe, &args, Some("runas"), owner);
    let result = started.and_then(|s| {
        // the exit code says nothing more than the response file
        let _ = s.wait();
        let text = std::fs::read_to_string(&resp)?;
        serde_json::from_str(&text).map_err(std::io::Error::other)
    });
    let _ = std::fs::remove_file(&req);
    let _ = std::fs::remove_file(&resp);
    result
}

/// The registry's programs of this machine, without sizes (fast: leftover scans
/// check what is still installed with it).
#[must_use]
pub fn registry_list(own_dir: Option<PathBuf>) -> Vec<inventory::Program> {
    use nfd_platform::Platform;
    let native = nfd_platform::Native;
    let resolve = native.resolver();
    let env = |k: &str| {
        std::env::var(k)
            .ok()
            .or_else(|| resolve(&k.to_uppercase()).map(|v| v.replace('/', "\\")))
    };
    let ienv = inventory::Env {
        env: &env,
        exists: &|p: &Path| p.exists(),
        system_root: system_root(),
        own_dir,
    };
    inventory::registry_programs(&crate::registry::LiveRegistry, &ienv)
}

/// `%SystemRoot%`.
#[must_use]
pub fn system_root() -> PathBuf {
    PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
}

/// The installed programs of this machine: the registry's entries and the Store
/// packages, sizes measured for entries that registered none.
#[must_use]
pub fn list_programs(
    own_dir: Option<PathBuf>,
    store: bool,
    max_entries: u64,
) -> Vec<inventory::Program> {
    use nfd_platform::Platform;
    let native = nfd_platform::Native;
    let resolve = native.resolver();
    let env = |k: &str| {
        std::env::var(k)
            .ok()
            .or_else(|| resolve(&k.to_uppercase()).map(|v| v.replace('/', "\\")))
    };
    let system_root =
        PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()));
    let ienv = inventory::Env {
        env: &env,
        exists: &|p: &Path| p.exists(),
        system_root,
        own_dir,
    };
    let mut list = inventory::registry_programs(&crate::registry::LiveRegistry, &ienv);
    if store {
        let read = |p: &Path| std::fs::read_to_string(p).ok();
        list.extend(inventory::store_programs(
            store_rows(),
            &read,
            &resolve_indirect,
        ));
    }
    let roots = crate::leftovers::Roots::detect(&|v| resolve(v), &|k| std::env::var(k).ok()).all();
    inventory::measure_sizes(&mut list, max_entries, &roots);
    // Store apps: install folder + data folder
    let local = native.known_folder("LOCALAPPDATA");
    for p in list.iter_mut().filter(|p| p.source == crate::Source::Store) {
        let mut bytes = p
            .install_location
            .as_deref()
            .map_or(0, |l| crate::fsutil::totals(l, max_entries).bytes);
        if let (Some(l), Some(pkg)) = (&local, &p.package) {
            bytes +=
                crate::fsutil::totals(&l.join("Packages").join(&pkg.family), max_entries).bytes;
        }
        p.size = bytes;
        p.size_measured = true;
    }
    list.sort_by_key(|p| p.name.to_lowercase());
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_children_and_ignores_reused_pids() {
        // root 10 (launched at 1000) → 11 → 12; an old process reused pid 11 earlier
        let mut tracked = vec![Tracked {
            pid: 10,
            created: 900,
            exact: false,
        }];
        let snap: Snapshot = vec![
            (10, 1, 1005),
            (11, 10, 1010),
            (12, 11, 1020),
            (13, 10, 500),
            (20, 1, 1100),
        ];
        assert_eq!(follow(&mut tracked, &snap), 3);
        assert!(
            tracked.iter().all(|t| t.pid != 13),
            "older than its parent: a reused pid"
        );
        assert!(tracked.iter().all(|t| t.pid != 20));
        // the root exits, the copy it started in Temp goes on
        let snap: Snapshot = vec![(11, 10, 1010), (12, 11, 1020)];
        assert_eq!(follow(&mut tracked, &snap), 2);
        // a new process got pid 10 again: not the root
        let snap: Snapshot = vec![(10, 1, 5000)];
        assert_eq!(follow(&mut tracked, &snap), 0);
    }

    #[test]
    fn the_root_may_be_gone_before_the_first_look() {
        let mut tracked = vec![Tracked {
            pid: 10,
            created: 900,
            exact: false,
        }];
        // only the child is left (NSIS: Un_A.exe)
        let snap: Snapshot = vec![(11, 10, 1010)];
        assert_eq!(follow(&mut tracked, &snap), 1);
    }

    #[test]
    fn waits_until_the_tree_is_gone() {
        let calls = std::cell::Cell::new(0);
        let list = || {
            calls.set(calls.get() + 1);
            if calls.get() < 3 {
                vec![(10, 1, 1000)]
            } else {
                Vec::new()
            }
        };
        let stop = AtomicBool::new(false);
        let mut ticks = Vec::new();
        let w = wait_tree(
            10,
            1000,
            &list,
            &stop,
            Duration::from_millis(1),
            Duration::from_secs(5),
            &mut |a| ticks.push(a),
        );
        assert_eq!(w, Waited::Done);
        assert_eq!(ticks, vec![1, 1, 0]);
        stop.store(true, Ordering::Relaxed);
        assert_eq!(
            wait_tree(
                10,
                1000,
                &|| vec![(10, 1, 1000)],
                &stop,
                Duration::from_millis(1),
                Duration::from_secs(5),
                &mut |_| {}
            ),
            Waited::Stopped
        );
        let stop = AtomicBool::new(false);
        assert_eq!(
            wait_tree(
                10,
                1000,
                &|| vec![(10, 1, 1000)],
                &stop,
                Duration::from_millis(1),
                Duration::ZERO,
                &mut |_| {}
            ),
            Waited::TimedOut
        );
    }

    #[test]
    fn package_names() {
        assert!(valid_package_name(
            "Microsoft.WindowsCalculator_11.2401.0.0_x64__8wekyb3d8bbwe"
        ));
        assert!(!valid_package_name("x'; Remove-Item C:\\ -Recurse; '"));
        assert!(!valid_package_name(""));
    }
}
