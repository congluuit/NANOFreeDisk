//! Real-time kernel trace session (needs admin): file I/O per process and file.
//!
//! A private SystemTraceProvider session (`EVENT_TRACE_SYSTEM_LOGGER_MODE`,
//! Windows 8+) named [`SESSION`] — the shared "NT Kernel Logger" stays free for
//! Process Monitor, WPR and friends. Flags: `FILE_IO_INIT` (Create, Read, Write,
//! Delete, Rename…), `DISK_FILE_IO` (file names) and `THREAD` (TID → PID). Events
//! go to a [`Collector`]; [`EtwSession::take`] hands over one interval.
//!
//! Kernel sessions outlive their process, so a session left by a crash is stopped
//! before a new one starts, and [`Drop`] always stops ours. Names of files opened
//! before the session started come from a short second session whose stop makes
//! the kernel enumerate every open file (FileRundown).

use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_SUCCESS};
use windows_sys::Win32::Storage::FileSystem::{GetLogicalDrives, QueryDosDeviceW};
use windows_sys::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, EVENT_HEADER_FLAG_32_BIT_HEADER,
    EVENT_HEADER_FLAG_64_BIT_HEADER, EVENT_RECORD, EVENT_TRACE_CONTROL_QUERY,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_FLAG, EVENT_TRACE_FLAG_DISK_FILE_IO,
    EVENT_TRACE_FLAG_FILE_IO_INIT, EVENT_TRACE_FLAG_THREAD, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, EVENT_TRACE_SYSTEM_LOGGER_MODE, OpenTraceW,
    PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME, PROCESSTRACE_HANDLE,
    ProcessTrace, StartTraceW, WNODE_FLAG_TRACED_GUID,
};
use windows_sys::core::GUID;

use crate::Tick;
use crate::etw_parse::{Collector, Devices, Header, Provider};

/// Name of the monitor's trace session.
pub const SESSION: &str = "NANOFreeDisk Disk Monitor";
const RUNDOWN: &str = "NANOFreeDisk Disk Monitor Rundown";

/// Session GUIDs (ours, random).
const SESSION_GUID: GUID = GUID::from_u128(0x6e3b_2f1a_8c4d_4b7e_9a51_d2c3_f0e1_a6b7);
const RUNDOWN_GUID: GUID = GUID::from_u128(0x6e3b_2f1a_8c4d_4b7e_9a51_d2c3_f0e1_a6b8);

/// Kernel provider GUIDs (classic events).
const FILE_IO: GUID = GUID::from_u128(0x90cb_dc39_4a3e_11d1_84f4_0000_f804_64e3);
const THREAD: GUID = GUID::from_u128(0x3d6f_a8d1_fe05_11d0_9dda_00c0_4fd7_ba7c);

const ERROR_WMI_INSTANCE_NOT_FOUND: u32 = 4201;
const INVALID_PROCESSTRACE_HANDLE: u64 = u64::MAX;
/// FILETIME of the Unix epoch.
const EPOCH_100NS: i64 = 116_444_736_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "error", content = "code")]
pub enum EtwError {
    /// Kernel tracing needs an elevated process.
    AccessDenied,
    /// `StartTraceW` failed with this Win32 error.
    Start(u32),
    /// `OpenTraceW` failed.
    Open(u32),
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn same(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

/// `EVENT_TRACE_PROPERTIES` followed by room for the logger name.
struct Props {
    buf: Vec<u64>,
}

impl Props {
    fn new(name: &str) -> Self {
        let name_bytes = (name.encode_utf16().count() + 1) * 2;
        let total = size_of::<EVENT_TRACE_PROPERTIES>() + name_bytes * 2 + 16;
        let mut buf = vec![0u64; total.div_ceil(8)];
        // SAFETY: the buffer is large enough and 8-byte aligned.
        let p = unsafe { &mut *buf.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>() };
        p.Wnode.BufferSize = (buf.len() * 8) as u32;
        p.LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
        Self { buf }
    }

    fn get(&mut self) -> &mut EVENT_TRACE_PROPERTIES {
        // SAFETY: see new().
        unsafe { &mut *self.buf.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>() }
    }
}

/// Stop a session by name (ours from an earlier run, or the rundown helper).
/// Returns `ERROR_SUCCESS`, `ERROR_WMI_INSTANCE_NOT_FOUND`, or another error.
fn control(name: &str, code: u32) -> (u32, Props) {
    let mut props = Props::new(name);
    let n = wide(name);
    // SAFETY: valid properties buffer and name.
    let rc = unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE { Value: 0 },
            n.as_ptr(),
            props.get(),
            code,
        )
    };
    (rc, props)
}

/// Is a session with this name running?
pub fn session_exists(name: &str) -> bool {
    control(name, EVENT_TRACE_CONTROL_QUERY).0 != ERROR_WMI_INSTANCE_NOT_FOUND
}

fn start_session(
    name: &str,
    guid: GUID,
    flags: EVENT_TRACE_FLAG,
) -> Result<CONTROLTRACE_HANDLE, EtwError> {
    let n = wide(name);
    for attempt in 0..2 {
        let mut props = Props::new(name);
        let p = props.get();
        p.Wnode.Guid = guid;
        p.Wnode.ClientContext = 1; // QPC timestamps
        p.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        p.LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_SYSTEM_LOGGER_MODE;
        p.EnableFlags = flags;
        p.BufferSize = 64; // KB
        p.MinimumBuffers = 16;
        p.MaximumBuffers = 64;
        p.FlushTimer = 1; // s
        let mut h = CONTROLTRACE_HANDLE { Value: 0 };
        // SAFETY: valid properties buffer and name.
        let rc = unsafe { StartTraceW(&mut h, n.as_ptr(), props.get()) };
        match rc {
            ERROR_SUCCESS => return Ok(h),
            ERROR_ALREADY_EXISTS if attempt == 0 => {
                // a session left over by a crash: ours by name, stop it and retry
                control(name, EVENT_TRACE_CONTROL_STOP);
            }
            5 => return Err(EtwError::AccessDenied),
            e => return Err(EtwError::Start(e)),
        }
    }
    Err(EtwError::Start(ERROR_ALREADY_EXISTS))
}

/// The `\Device\…` target of every drive letter.
pub fn devices() -> Devices {
    let mut list = Vec::new();
    // SAFETY: no arguments.
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = format!("{}:", char::from(b'A' + i as u8));
        let w = wide(&letter);
        let mut buf = vec![0u16; 1024];
        // SAFETY: buffer length in characters.
        let n = unsafe { QueryDosDeviceW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        if n == 0 {
            continue;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(n as usize);
        let target = String::from_utf16_lossy(&buf[..end]);
        if target.starts_with(r"\Device\") {
            list.push((target, letter));
        }
    }
    Devices::new(list)
}

/// The ways a folder appears in file events: as given, its long name with links
/// resolved (what Name events carry) and its 8.3 short name (what a process that
/// opened it by that name puts in Create events).
fn spellings(folder: &str) -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::{GetLongPathNameW, GetShortPathNameW};
    let mut out = vec![folder.trim_end_matches('\\').to_owned()];
    if let Ok(p) = std::fs::canonicalize(folder) {
        let s = p.to_string_lossy().into_owned();
        out.push(s.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(s));
    }
    let w = wide(folder);
    for long in [true, false] {
        let mut buf = vec![0u16; 1024];
        // SAFETY: NUL-terminated input, buffer length in characters.
        let n = unsafe {
            if long {
                GetLongPathNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32)
            } else {
                GetShortPathNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32)
            }
        } as usize;
        if n > 0 && n < buf.len() {
            out.push(String::from_utf16_lossy(&buf[..n]));
        }
    }
    out.sort();
    out.dedup();
    out
}

struct Shared {
    collector: Mutex<Collector>,
}

unsafe extern "system" fn on_event(rec: *mut EVENT_RECORD) {
    // SAFETY: ETW passes a valid record for the duration of the callback.
    let rec = unsafe { &*rec };
    if rec.UserContext.is_null() {
        return;
    }
    // SAFETY: Context is the Arc<Shared> kept alive until ProcessTrace returns.
    let shared = unsafe { &*rec.UserContext.cast::<Shared>() };
    let eh = &rec.EventHeader;
    let provider = if same(&eh.ProviderId, &FILE_IO) {
        Provider::FileIo
    } else if same(&eh.ProviderId, &THREAD) {
        Provider::Thread
    } else {
        return;
    };
    let flags = u32::from(eh.Flags);
    let h = Header {
        provider,
        opcode: eh.EventDescriptor.Opcode,
        version: eh.EventDescriptor.Version,
        pid: eh.ProcessId,
        tid: eh.ThreadId,
        ptr64: flags & EVENT_HEADER_FLAG_64_BIT_HEADER != 0
            || (flags & EVENT_HEADER_FLAG_32_BIT_HEADER == 0 && cfg!(target_pointer_width = "64")),
    };
    let data: &[u8] = if rec.UserData.is_null() || rec.UserDataLength == 0 {
        &[]
    } else {
        // SAFETY: UserDataLength bytes at UserData, valid during the callback.
        unsafe {
            std::slice::from_raw_parts(rec.UserData.cast::<u8>(), usize::from(rec.UserDataLength))
        }
    };
    // without PROCESS_TRACE_MODE_RAW_TIMESTAMP the timestamp is a FILETIME
    let now_ms = (eh.TimeStamp - EPOCH_100NS).max(0) as u64 / 10_000;
    if let Ok(mut c) = shared.collector.lock() {
        c.event(&h, data, now_ms);
    }
}

/// Open a real-time consumer on `name` and run it on a thread until the session stops.
fn consume(name: &str, shared: &Arc<Shared>) -> Result<JoinHandle<()>, EtwError> {
    let mut n = wide(name);
    // SAFETY: zeroed is a valid EVENT_TRACE_LOGFILEW (all pointers null).
    let mut lf: EVENT_TRACE_LOGFILEW = unsafe { std::mem::zeroed() };
    lf.LoggerName = n.as_mut_ptr();
    lf.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
    lf.Anonymous2.EventRecordCallback = Some(on_event);
    lf.Context = Arc::as_ptr(shared).cast_mut().cast::<c_void>();
    // SAFETY: initialized logfile; the name buffer outlives the call.
    let h = unsafe { OpenTraceW(&mut lf) };
    if h.Value == INVALID_PROCESSTRACE_HANDLE {
        return Err(EtwError::Open(
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32,
        ));
    }
    let keep = shared.clone();
    std::thread::Builder::new()
        .name("nfd-etw".into())
        .spawn(move || {
            let handle = PROCESSTRACE_HANDLE { Value: h.Value };
            // SAFETY: handle from OpenTraceW; blocks until the session stops.
            unsafe {
                ProcessTrace(&handle, 1, std::ptr::null(), std::ptr::null());
                CloseTrace(handle);
            }
            drop(keep);
        })
        .map_err(|_| EtwError::Open(0))
}

/// A running monitor session. Dropping it stops the session.
pub struct EtwSession {
    shared: Arc<Shared>,
    consumer: Option<JoinHandle<()>>,
    last: Instant,
}

impl std::fmt::Debug for EtwSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EtwSession").finish_non_exhaustive()
    }
}

impl EtwSession {
    /// Start the session. `watch`: a folder whose file events are reported
    /// ("Who touched this?").
    pub fn start(watch: Option<&str>) -> Result<Self, EtwError> {
        let watch = watch.map(spellings).unwrap_or_default();
        let shared = Arc::new(Shared {
            collector: Mutex::new(Collector::new(devices(), &watch)),
        });
        start_session(
            SESSION,
            SESSION_GUID,
            EVENT_TRACE_FLAG_FILE_IO_INIT | EVENT_TRACE_FLAG_DISK_FILE_IO | EVENT_TRACE_FLAG_THREAD,
        )?;
        let consumer = match consume(SESSION, &shared) {
            Ok(c) => c,
            Err(e) => {
                control(SESSION, EVENT_TRACE_CONTROL_STOP);
                return Err(e);
            }
        };
        // names of files that are already open
        let rundown = shared.clone();
        std::thread::spawn(move || {
            if start_session(RUNDOWN, RUNDOWN_GUID, EVENT_TRACE_FLAG_DISK_FILE_IO).is_ok() {
                if let Ok(t) = consume(RUNDOWN, &rundown) {
                    std::thread::sleep(Duration::from_millis(1500));
                    control(RUNDOWN, EVENT_TRACE_CONTROL_STOP);
                    let _ = t.join();
                } else {
                    control(RUNDOWN, EVENT_TRACE_CONTROL_STOP);
                }
            }
        });
        Ok(Self {
            shared,
            consumer: Some(consumer),
            last: Instant::now(),
        })
    }

    /// Events dropped by the session so far.
    fn lost() -> u64 {
        let (rc, mut props) = control(SESSION, EVENT_TRACE_CONTROL_QUERY);
        if rc != ERROR_SUCCESS {
            return 0;
        }
        let p = props.get();
        u64::from(p.EventsLost) + u64::from(p.RealTimeBuffersLost)
    }

    /// Is the consumer still running (the session was not stopped from outside)?
    pub fn alive(&self) -> bool {
        self.consumer.as_ref().is_some_and(|c| !c.is_finished())
    }

    /// Activity since the previous call.
    pub fn take(&mut self) -> Tick {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_millis() as u64;
        self.last = now;
        let lost = Self::lost();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        match self.shared.collector.lock() {
            Ok(mut c) => c.take(elapsed, lost, now_ms),
            Err(_) => Tick {
                elapsed_ms: elapsed,
                ..Tick::default()
            },
        }
    }

    /// The last Create events as decoded (diagnostics).
    pub fn recent_creates(&self) -> Vec<String> {
        self.shared
            .collector
            .lock()
            .map(|c| c.recent_creates())
            .unwrap_or_default()
    }

    pub fn stop(&mut self) {
        if let Some(t) = self.consumer.take() {
            control(SESSION, EVENT_TRACE_CONTROL_STOP);
            let _ = t.join();
        }
    }
}

/// Stop the session when the console is closed or Ctrl+C is pressed (the CLI): a
/// kernel session would otherwise outlive the process until the next start.
pub fn stop_on_ctrl_c() {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
    unsafe extern "system" fn handler(_ctrl: u32) -> i32 {
        control(SESSION, EVENT_TRACE_CONTROL_STOP);
        0 // let the default handler end the process
    }
    // SAFETY: a plain function pointer that lives for the whole process.
    unsafe { SetConsoleCtrlHandler(Some(handler), 1) };
}

impl Drop for EtwSession {
    fn drop(&mut self) {
        self.stop();
    }
}
