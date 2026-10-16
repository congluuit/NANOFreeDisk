//! Windows implementation: process snapshot, windows, publishers, GPU per process,
//! and the guarded End task.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::time::Instant;

use nfd_sensors::win::pdh::{Counter, Query};
use serde::Serialize;
use windows_sys::Wdk::System::SystemInformation::{
    NtQuerySystemInformation, SystemProcessInformation,
};
use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessCommandLineInformation,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, FILETIME, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Cryptography::{
    CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW,
};
use windows_sys::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO,
    WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
    WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvSignerFromChain,
    WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, LookupAccountSidW, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, SYNCHRONIZE, VerQueryValueW,
};
use windows_sys::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW,
    SC_ENUM_PROCESS_INFO, SC_MANAGER_ENUMERATE_SERVICE,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetExitCodeProcess, GetProcessInformation, GetProcessTimes,
    IsProcessCritical, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_PROTECTION_LEVEL_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    PROTECTION_LEVEL_NONE, ProcessProtectionLevelInfo, QueryFullProcessImageNameW,
    TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongPtrW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE, WS_EX_TOOLWINDOW,
};

use crate::safety::{self, Facts, HARD_BLOCK, Verdict};
use crate::{
    EndMode, EndResult, Group, Outcome, RawProcess, Summary, Task, TaskSnapshot, cpu_percent,
    descendants, parse_process_info,
};

const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

struct Handle(HANDLE);

impl Handle {
    fn open(pid: u32, access: u32) -> Option<Self> {
        // SAFETY: plain open; closed in Drop.
        let h = unsafe { OpenProcess(access, 0, pid) };
        (!h.is_null() && h != INVALID_HANDLE_VALUE).then_some(Self(h))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: opened by OpenProcess.
        unsafe { CloseHandle(self.0) };
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(b: &[u16]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..end])
}

/// # Safety
/// `p` must be null or NUL-terminated UTF-16.
unsafe fn pwstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    // SAFETY: caller guarantees termination.
    unsafe {
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }
}

/// Every process, from one kernel call.
pub fn processes() -> Vec<RawProcess> {
    let mut size = 512 * 1024usize;
    for _ in 0..6 {
        let mut buf = vec![0u64; size / 8];
        let mut got = 0u32;
        // SAFETY: buffer of `size` bytes.
        let st = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buf.as_mut_ptr().cast(),
                size as u32,
                &mut got,
            )
        };
        if st == STATUS_INFO_LENGTH_MISMATCH {
            size = (got as usize).max(size) + 128 * 1024;
            continue;
        }
        if st < 0 {
            return Vec::new();
        }
        let base = buf.as_ptr() as usize;
        let len = (got as usize).min(size);
        // SAFETY: the bytes the kernel wrote.
        let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), len) };
        // image names point into the same buffer; anything outside it is ignored
        let reader = |ptr: usize, chars: usize| -> String {
            if ptr < base || ptr + chars * 2 > base + len {
                return String::new();
            }
            // SAFETY: bounds checked against the buffer above.
            let s = unsafe { std::slice::from_raw_parts(ptr as *const u16, chars) };
            String::from_utf16_lossy(s)
        };
        return parse_process_info(bytes, &reader);
    }
    Vec::new()
}

fn filetime(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

fn creation_time(h: &Handle) -> Option<u64> {
    let mut c = FILETIME::default();
    let (mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: valid handle, out-params.
    (unsafe { GetProcessTimes(h.0, &mut c, &mut e, &mut k, &mut u) } != 0).then(|| filetime(c))
}

fn image_path(h: &Handle) -> Option<String> {
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY: buffer length in characters.
    (unsafe { QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) }
        != 0)
        .then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// Full image path of a live process (query-limited access; `None` for protected
/// or exited processes).
pub fn image_path_of(pid: u32) -> Option<String> {
    if pid == 0 || pid == 4 {
        return None;
    }
    Handle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION).and_then(|h| image_path(&h))
}

fn critical(h: &Handle) -> bool {
    let mut c = 0;
    // SAFETY: valid handle.
    unsafe { IsProcessCritical(h.0, &mut c) != 0 && c != 0 }
}

fn protected(h: &Handle) -> bool {
    let mut info = PROCESS_PROTECTION_LEVEL_INFORMATION {
        ProtectionLevel: PROTECTION_LEVEL_NONE,
    };
    // SAFETY: struct of the requested class.
    let ok = unsafe {
        GetProcessInformation(
            h.0,
            ProcessProtectionLevelInfo,
            (&mut info as *mut PROCESS_PROTECTION_LEVEL_INFORMATION).cast(),
            size_of::<PROCESS_PROTECTION_LEVEL_INFORMATION>() as u32,
        )
    } != 0;
    ok && info.ProtectionLevel != PROTECTION_LEVEL_NONE
}

fn system_root() -> String {
    let mut r = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    if !r.ends_with('\\') {
        r.push('\\');
    }
    r.to_ascii_lowercase()
}

fn own_exe() -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().to_ascii_lowercase())
}

/// Visible, unowned, titled top-level windows per PID.
fn windows() -> HashMap<u32, Vec<(HWND, String)>> {
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> i32 {
        // SAFETY: lp is the &mut map passed to EnumWindows below.
        let map = unsafe { &mut *(lp as *mut HashMap<u32, Vec<(HWND, String)>>) };
        // SAFETY: plain window queries on a live HWND.
        unsafe {
            if IsWindowVisible(hwnd) == 0 || !GetWindow(hwnd, GW_OWNER).is_null() {
                return 1;
            }
            if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0 {
                return 1;
            }
            let mut buf = [0u16; 256];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if n <= 0 {
                return 1;
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            map.entry(pid)
                .or_default()
                .push((hwnd, String::from_utf16_lossy(&buf[..n as usize])));
        }
        1
    }
    let mut map: HashMap<u32, Vec<(HWND, String)>> = HashMap::new();
    // SAFETY: the callback only runs during this call; map outlives it.
    unsafe { EnumWindows(Some(cb), &mut map as *mut _ as LPARAM) };
    map
}

/// Authenticode signer of an embedded signature (`None`: unsigned or catalog-signed).
fn signer(path: &str) -> Option<String> {
    let p = wide(path);
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: p.as_ptr(),
        ..Default::default()
    };
    let mut data: WINTRUST_DATA = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        ..Default::default()
    };
    data.Anonymous.pFile = &mut file;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: documented use with an initialized WINTRUST_DATA; offline (cache-only).
    let rc = unsafe {
        WinVerifyTrust(
            INVALID_HANDLE_VALUE as HWND,
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    let mut name = None;
    if rc == 0 {
        // SAFETY: state data is valid until STATEACTION_CLOSE below.
        unsafe {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if !prov.is_null() {
                let sgnr = WTHelperGetProvSignerFromChain(prov, 0, 0, 0);
                if !sgnr.is_null() && (*sgnr).csCertChain > 0 && !(*sgnr).pasCertChain.is_null() {
                    let cert = (*(*sgnr).pasCertChain).pCert;
                    let mut buf = [0u16; 256];
                    let n = CertGetNameStringW(
                        cert,
                        CERT_NAME_SIMPLE_DISPLAY_TYPE,
                        0,
                        std::ptr::null(),
                        buf.as_mut_ptr(),
                        buf.len() as u32,
                    );
                    if n > 1 {
                        name = Some(from_wide(&buf));
                    }
                }
            }
        }
    }
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: releases the state opened above.
    unsafe {
        WinVerifyTrust(
            INVALID_HANDLE_VALUE as HWND,
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    name
}

/// `CompanyName` from the version resource (not verified).
fn company(path: &str) -> Option<String> {
    let p = wide(path);
    let mut h = 0;
    // SAFETY: size query.
    let size = unsafe { GetFileVersionInfoSizeW(p.as_ptr(), &mut h) };
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: buffer of `size` bytes.
    if unsafe { GetFileVersionInfoW(p.as_ptr(), 0, size, buf.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut len = 0u32;
    let q = wide(r"\VarFileInfo\Translation");
    // SAFETY: querying the block just read.
    if unsafe { VerQueryValueW(buf.as_ptr().cast(), q.as_ptr(), &mut ptr, &mut len) } == 0
        || len < 4
    {
        return None;
    }
    // SAFETY: at least one (lang, codepage) pair.
    let (lang, cp) = unsafe { (*(ptr as *const u16), *(ptr as *const u16).add(1)) };
    let q = wide(&format!(r"\StringFileInfo\{lang:04x}{cp:04x}\CompanyName"));
    // SAFETY: as above.
    if unsafe { VerQueryValueW(buf.as_ptr().cast(), q.as_ptr(), &mut ptr, &mut len) } == 0
        || len == 0
    {
        return None;
    }
    // SAFETY: NUL-terminated string inside `buf`.
    let s = unsafe { pwstr(ptr as *const u16) }.trim().to_owned();
    (!s.is_empty()).then_some(s)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Publisher {
    name: String,
    verified: bool,
}

/// Publisher shown for a path: the verified signer, or `CompanyName *` (unverified).
pub fn publisher_label(path: &str) -> Option<String> {
    publisher(path).map(|p| {
        if p.verified {
            p.name
        } else {
            format!("{} *", p.name)
        }
    })
}

fn publisher(path: &str) -> Option<Publisher> {
    if let Some(name) = signer(path) {
        return Some(Publisher {
            name,
            verified: true,
        });
    }
    company(path).map(|name| Publisher {
        name,
        verified: false,
    })
}

/// Live process list with rates. Keep one per view and call [`TaskMonitor::snapshot`]
/// about once a second.
pub struct TaskMonitor {
    prev: HashMap<(u32, u64), (u64, u64, u64)>,
    last: Option<Instant>,
    paths: HashMap<(u32, u64), Option<String>>,
    publishers: HashMap<String, Option<Publisher>>,
    pdh: Option<(Query, Option<Counter>, Option<Counter>)>,
    cpus: u32,
    raw: Vec<RawProcess>,
    own_pid: u32,
}

// SAFETY: PDH handles are process-wide; the monitor is used behind a Mutex.
unsafe impl Send for TaskMonitor {}

impl std::fmt::Debug for TaskMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskMonitor")
            .field("processes", &self.raw.len())
            .finish_non_exhaustive()
    }
}

impl Default for TaskMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Publisher lookups per refresh (Authenticode is slow): the column fills in over a
/// few seconds instead of stalling the first refresh.
const PUBLISHER_BUDGET: usize = 12;

impl TaskMonitor {
    pub fn new() -> Self {
        let mut q = Query::new();
        let counters = q.as_mut().map(|q| {
            (
                q.add(r"\GPU Engine(*)\Utilization Percentage"),
                q.add(r"\PhysicalDisk(_Total)\Disk Bytes/sec"),
            )
        });
        let pdh = match (q, counters) {
            (Some(mut q), Some((g, d))) => {
                q.collect();
                Some((q, g, d))
            }
            _ => None,
        };
        Self {
            prev: HashMap::new(),
            last: None,
            paths: HashMap::new(),
            publishers: HashMap::new(),
            pdh,
            cpus: std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
            raw: Vec::new(),
            // SAFETY: no arguments.
            own_pid: unsafe { GetCurrentProcessId() },
        }
    }

    /// The raw list of the last snapshot (for "End process tree").
    pub fn raw(&self) -> &[RawProcess] {
        &self.raw
    }

    pub fn snapshot(&mut self) -> TaskSnapshot {
        let start = Instant::now();
        let procs = processes();
        let elapsed = self
            .last
            .map(|t| t.elapsed().as_nanos() as u64 / 100)
            .unwrap_or(0);
        self.last = Some(start);
        let wins = windows();
        let root = system_root();
        let own = own_exe();
        // GPU per process: busiest engine; total: busiest engine over all processes
        let mut gpu_pid: HashMap<u32, f64> = HashMap::new();
        let mut engines: HashMap<String, f64> = HashMap::new();
        let mut disk = 0.0;
        if let Some((q, g, d)) = &mut self.pdh {
            if q.collect() {
                if let Some(g) = g {
                    for (inst, v) in q.values(*g) {
                        let l = inst.to_ascii_lowercase();
                        let pid = l
                            .strip_prefix("pid_")
                            .and_then(|r| r.split('_').next())
                            .and_then(|p| p.parse::<u32>().ok());
                        if let Some(pid) = pid {
                            let e = gpu_pid.entry(pid).or_insert(0.0);
                            *e = e.max(v);
                        }
                        if let Some(eng) = l.split("_luid_").nth(1) {
                            *engines.entry(eng.to_owned()).or_insert(0.0) += v;
                        }
                    }
                }
                if let Some(d) = d {
                    disk = q.value(*d).unwrap_or(0.0);
                }
            }
        }
        let mut next_prev = HashMap::with_capacity(procs.len());
        let mut live = HashSet::with_capacity(procs.len());
        let mut budget = PUBLISHER_BUDGET;
        let mut tasks = Vec::with_capacity(procs.len());
        let mut total_cpu = 0.0;
        let mut threads = 0u64;
        for p in &procs {
            let key = (p.pid, p.create_time);
            live.insert(key);
            next_prev.insert(key, (p.cpu_time, p.io_read, p.io_write));
            threads += u64::from(p.threads);
            let (cpu, rd, wr) = match self.prev.get(&key) {
                Some(&(c, r, w)) if elapsed > 0 => {
                    let secs = elapsed as f64 / 1e7;
                    (
                        cpu_percent(p.cpu_time.saturating_sub(c), elapsed, self.cpus),
                        p.io_read.saturating_sub(r) as f64 / secs,
                        p.io_write.saturating_sub(w) as f64 / secs,
                    )
                }
                _ => (0.0, 0.0, 0.0),
            };
            if p.pid != 0 {
                total_cpu += cpu;
            }
            let path = self
                .paths
                .entry(key)
                .or_insert_with(|| {
                    if p.pid == 0 || p.pid == 4 {
                        return None;
                    }
                    Handle::open(p.pid, PROCESS_QUERY_LIMITED_INFORMATION)
                        .and_then(|h| image_path(&h))
                })
                .clone();
            let publisher = match &path {
                Some(pa) => {
                    if let Some(known) = self.publishers.get(pa) {
                        known.clone()
                    } else if budget > 0 {
                        budget -= 1;
                        let v = publisher(pa);
                        self.publishers.insert(pa.clone(), v.clone());
                        v
                    } else {
                        None
                    }
                }
                None => None,
            };
            let title = wins
                .get(&p.pid)
                .and_then(|w| w.first())
                .map(|(_, t)| t.clone());
            let lname = p.name.to_ascii_lowercase();
            let in_windows = path
                .as_deref()
                .is_some_and(|x| x.to_ascii_lowercase().starts_with(&root));
            let blocked = p.pid == 0
                || p.pid == 4
                || p.pid == self.own_pid
                || HARD_BLOCK.contains(&lname.as_str())
                || path
                    .as_deref()
                    .zip(own.as_deref())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b));
            let group = if title.is_some() && p.session != 0 {
                Group::App
            } else if p.pid <= 4
                || in_windows
                || HARD_BLOCK.contains(&lname.as_str())
                || (path.is_none() && p.session == 0)
            {
                Group::Windows
            } else {
                Group::Background
            };
            tasks.push(Task {
                pid: p.pid,
                ppid: p.ppid,
                name: if p.pid == 0 {
                    "System Idle Process".into()
                } else {
                    p.name.clone()
                },
                create_time: p.create_time.to_string(),
                path,
                title,
                group,
                cpu,
                memory: p.private_ws,
                io_read: rd,
                io_write: wr,
                gpu: gpu_pid.get(&p.pid).copied().unwrap_or(0.0).min(100.0),
                threads: p.threads,
                handles: p.handles,
                session: p.session,
                priority: p.base_priority,
                publisher: publisher.map(|x| {
                    if x.verified {
                        x.name
                    } else {
                        format!("{} *", x.name)
                    }
                }),
                blocked,
            });
        }
        crate::group_app_children(&mut tasks);
        self.prev = next_prev;
        self.paths.retain(|k, _| live.contains(k));
        let mut perf = PERFORMANCE_INFORMATION::default();
        // SAFETY: cb is the struct size.
        let have_perf =
            unsafe { GetPerformanceInfo(&mut perf, size_of::<PERFORMANCE_INFORMATION>() as u32) }
                != 0;
        let page = perf.PageSize as u64;
        let total = perf.PhysicalTotal as u64 * page;
        let avail = perf.PhysicalAvailable as u64 * page;
        self.raw = procs;
        TaskSnapshot {
            summary: Summary {
                cpu: total_cpu.min(100.0),
                memory_used: if have_perf { total - avail } else { 0 },
                memory_total: total,
                disk,
                gpu: engines.values().fold(0.0f64, |a, &b| a.max(b)).min(100.0),
                processes: tasks.len(),
                threads,
            },
            tasks,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }
}

/// Open the process and confirm it is still the one the user saw.
fn reopen(pid: u32, create_time: u64, access: u32) -> Result<Handle, Outcome> {
    let Some(h) = Handle::open(pid, access) else {
        // still there (same PID and start time) but not ours to open, or gone
        let alive = processes()
            .iter()
            .any(|p| p.pid == pid && p.create_time == create_time);
        return Err(if alive {
            Outcome::AccessDenied
        } else {
            Outcome::Changed
        });
    };
    match creation_time(&h) {
        Some(t) if t == create_time => {}
        _ => return Err(Outcome::Changed),
    }
    // an exited process lingers while someone holds a handle to it
    const STILL_ACTIVE: u32 = 259;
    let mut code = 0u32;
    // SAFETY: valid handle, out-param.
    if unsafe { GetExitCodeProcess(h.0, &mut code) } != 0 && code != STILL_ACTIVE {
        return Err(Outcome::Ended);
    }
    Ok(h)
}

fn facts(pid: u32, h: &Handle, fallback_name: &str) -> Facts {
    let path = image_path(h);
    let name = path
        .as_deref()
        .and_then(|p| p.rsplit('\\').next())
        .map(str::to_owned)
        .unwrap_or_else(|| fallback_name.to_owned());
    let own_path = own_exe();
    Facts {
        pid,
        name,
        critical: critical(h),
        protected: protected(h),
        // SAFETY: no arguments.
        own: pid == unsafe { GetCurrentProcessId() }
            || path
                .as_deref()
                .zip(own_path.as_deref())
                .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b)),
        path,
        system_root: system_root(),
    }
}

fn name_of(pid: u32) -> String {
    processes()
        .into_iter()
        .find(|p| p.pid == pid)
        .map(|p| p.name)
        .unwrap_or_default()
}

/// The verdict for the confirmation dialog (`Err`: process changed / not accessible).
pub fn verdict(pid: u32, create_time: &str) -> Result<Verdict, Outcome> {
    let ct: u64 = create_time.parse().map_err(|_| Outcome::Changed)?;
    let name = name_of(pid);
    // name-only rules first: works even when the process cannot be opened
    let pre = safety::classify(&Facts {
        pid,
        name: name.clone(),
        system_root: system_root(),
        ..Facts::default()
    });
    if let Verdict::Blocked(r) = pre {
        return Ok(Verdict::Blocked(r));
    }
    let h = reopen(pid, ct, PROCESS_QUERY_LIMITED_INFORMATION)?;
    Ok(safety::classify(&facts(pid, &h, &name)))
}

fn end_one(pid: u32, ct: u64, mode: EndMode, confirmed: bool) -> EndResult {
    let name = name_of(pid);
    let done = |outcome| EndResult {
        pid,
        name: name.clone(),
        outcome,
    };
    if let Verdict::Blocked(r) = safety::classify(&Facts {
        pid,
        name: name.clone(),
        system_root: system_root(),
        ..Facts::default()
    }) {
        return done(Outcome::Blocked(r));
    }
    let access = PROCESS_QUERY_LIMITED_INFORMATION
        | SYNCHRONIZE
        | if mode == EndMode::Close {
            0
        } else {
            PROCESS_TERMINATE
        };
    let h = match reopen(pid, ct, access) {
        Ok(h) => h,
        Err(o) => return done(o),
    };
    match safety::classify(&facts(pid, &h, &name)) {
        Verdict::Blocked(r) => return done(Outcome::Blocked(r)),
        Verdict::Warn(w) if !confirmed => return done(Outcome::NeedsConfirm(w)),
        _ => {}
    }
    if mode == EndMode::Close {
        let wins = windows();
        let Some(list) = wins.get(&pid) else {
            return done(Outcome::NoWindow);
        };
        for (hwnd, _) in list {
            // SAFETY: posting WM_CLOSE to a window of the verified process.
            unsafe { PostMessageW(*hwnd, WM_CLOSE, 0, 0) };
        }
        return done(Outcome::CloseRequested);
    }
    // SAFETY: handle opened with PROCESS_TERMINATE for the verified process.
    if unsafe { TerminateProcess(h.0, 1) } == 0 {
        return done(Outcome::Failed(std::io::Error::last_os_error().to_string()));
    }
    // SAFETY: SYNCHRONIZE access requested.
    let waited = unsafe { WaitForSingleObject(h.0, 3000) };
    if waited == WAIT_OBJECT_0 {
        done(Outcome::Ended)
    } else {
        done(Outcome::Failed("still running after 3 s".into()))
    }
}

/// End a process the user picked and confirmed. `create_time` is the value the list
/// showed; a mismatch means the PID now belongs to another process and nothing is
/// done. `confirmed` is the second confirmation for [`Verdict::Warn`] processes.
pub fn end_task(pid: u32, create_time: &str, mode: EndMode, confirmed: bool) -> Vec<EndResult> {
    let Ok(ct) = create_time.parse::<u64>() else {
        return vec![EndResult {
            pid,
            name: String::new(),
            outcome: Outcome::Changed,
        }];
    };
    if mode != EndMode::KillTree {
        return vec![end_one(pid, ct, mode, confirmed)];
    }
    // children first; a blocked child is skipped (and reported), never forced
    let root = end_one_check(pid, ct, confirmed);
    if let Err(r) = root {
        return vec![r];
    }
    let procs = processes();
    let mut out: Vec<EndResult> = descendants(&procs, pid, ct)
        .into_iter()
        .map(|(p, t)| end_one(p, t, EndMode::Kill, true))
        .collect();
    out.push(end_one(pid, ct, EndMode::Kill, confirmed));
    out
}

/// Verdict gate for the tree root before touching any child.
fn end_one_check(pid: u32, ct: u64, confirmed: bool) -> Result<(), EndResult> {
    let fail = |outcome| EndResult {
        pid,
        name: name_of(pid),
        outcome,
    };
    match verdict(pid, &ct.to_string()) {
        Ok(Verdict::Blocked(r)) => Err(fail(Outcome::Blocked(r))),
        Ok(Verdict::Warn(w)) if !confirmed => Err(fail(Outcome::NeedsConfirm(w))),
        Ok(_) => Ok(()),
        Err(o) => Err(fail(o)),
    }
}

/// Extra facts for the detail pane and the confirmation dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TaskDetails {
    pub command_line: Option<String>,
    pub user: Option<String>,
    pub publisher: Option<String>,
    /// Authenticode signature verified (embedded).
    pub verified: bool,
    /// Services hosted by this process (svchost).
    pub services: Vec<String>,
    pub children: usize,
}

fn command_line(h: &Handle) -> Option<String> {
    let mut need = 0u32;
    // SAFETY: size query.
    unsafe {
        NtQueryInformationProcess(
            h.0,
            ProcessCommandLineInformation,
            std::ptr::null_mut(),
            0,
            &mut need,
        )
    };
    if need == 0 || need > 1 << 20 {
        return None;
    }
    let mut buf = vec![0u64; (need as usize).div_ceil(8)];
    // SAFETY: buffer of `need` bytes: UNICODE_STRING followed by its characters.
    let st = unsafe {
        NtQueryInformationProcess(
            h.0,
            ProcessCommandLineInformation,
            buf.as_mut_ptr().cast(),
            need,
            &mut need,
        )
    };
    if st < 0 {
        return None;
    }
    let bytes = buf.as_ptr().cast::<u8>();
    // SAFETY: UNICODE_STRING { Length u16, MaximumLength u16, pad, Buffer *u16 }
    unsafe {
        let len = *(bytes as *const u16) as usize / 2;
        let ptr = *(bytes.add(8) as *const *const u16);
        let base = bytes as usize;
        if ptr.is_null() || (ptr as usize) < base || ptr as usize + len * 2 > base + need as usize {
            return None;
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            ptr, len,
        )))
    }
}

fn user(h: &Handle) -> Option<String> {
    let mut tok: HANDLE = std::ptr::null_mut();
    // SAFETY: query-only token.
    if unsafe { OpenProcessToken(h.0, TOKEN_QUERY, &mut tok) } == 0 {
        return None;
    }
    let tok = Handle(tok);
    let mut buf = vec![0u64; 64];
    let mut len = 0u32;
    // SAFETY: buffer of 512 bytes.
    if unsafe { GetTokenInformation(tok.0, TokenUser, buf.as_mut_ptr().cast(), 512, &mut len) } == 0
    {
        return None;
    }
    // SAFETY: TokenUser fills a TOKEN_USER at the start.
    let sid = unsafe { (*(buf.as_ptr() as *const TOKEN_USER)).User.Sid };
    let (mut name, mut dom) = ([0u16; 256], [0u16; 256]);
    let (mut nl, mut dl) = (256u32, 256u32);
    let mut use_ = SID_NAME_USE::default();
    // SAFETY: buffers sized in characters.
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid,
            name.as_mut_ptr(),
            &mut nl,
            dom.as_mut_ptr(),
            &mut dl,
            &mut use_,
        )
    } != 0;
    ok.then(|| {
        let d = from_wide(&dom);
        let n = from_wide(&name);
        if d.is_empty() { n } else { format!("{d}\\{n}") }
    })
}

fn services_of(pid: u32) -> Vec<String> {
    // SAFETY: enumerate-only access to the service manager.
    let scm = unsafe {
        OpenSCManagerW(
            std::ptr::null(),
            std::ptr::null(),
            SC_MANAGER_ENUMERATE_SERVICE,
        )
    };
    if scm.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut need = 0u32;
    let mut count = 0u32;
    let mut resume = 0u32;
    const SERVICE_WIN32: u32 = 0x30;
    const SERVICE_ACTIVE: u32 = 1;
    // SAFETY: size query.
    unsafe {
        EnumServicesStatusExW(
            scm,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_ACTIVE,
            std::ptr::null_mut(),
            0,
            &mut need,
            &mut count,
            &mut resume,
            std::ptr::null(),
        )
    };
    if need > 0 {
        let mut buf = vec![0u64; (need as usize).div_ceil(8)];
        resume = 0;
        // SAFETY: buffer of `need` bytes.
        let ok = unsafe {
            EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_ACTIVE,
                buf.as_mut_ptr().cast(),
                need,
                &mut need,
                &mut count,
                &mut resume,
                std::ptr::null(),
            )
        } != 0;
        if ok {
            // SAFETY: `count` entries at the start of the buffer.
            let list = unsafe {
                std::slice::from_raw_parts(
                    buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW,
                    count as usize,
                )
            };
            for s in list {
                if s.ServiceStatusProcess.dwProcessId == pid {
                    // SAFETY: strings inside `buf`.
                    out.push(unsafe { pwstr(s.lpDisplayName) });
                }
            }
        }
    }
    // SAFETY: opened above.
    unsafe { CloseServiceHandle(scm) };
    out.sort();
    out
}

pub fn details(pid: u32, create_time: &str) -> Result<TaskDetails, Outcome> {
    let ct: u64 = create_time.parse().map_err(|_| Outcome::Changed)?;
    let h = reopen(pid, ct, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let path = image_path(&h);
    let publisher = path.as_deref().and_then(publisher);
    Ok(TaskDetails {
        command_line: command_line(&h),
        user: user(&h),
        verified: publisher.as_ref().is_some_and(|p| p.verified),
        publisher: publisher.map(|p| p.name),
        services: services_of(pid),
        children: descendants(&processes(), pid, ct).len(),
    })
}
