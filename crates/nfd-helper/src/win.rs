//! Named pipe, UAC / scheduled-task launch and the two ends of the protocol.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nfd_sensors::{Discovery, SensorInfo, SensorProvider};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_CANCELLED, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, GetLastError,
    HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile,
    WriteFile,
};
use windows_sys::Win32::System::LibraryLoader::{
    LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetProcessId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::task::task_pipe_name;
use crate::{
    HELPER_FLAG, MAX_FRAME, PROTOCOL, Request, Response, TaskState, WireSensor, decode, encode,
    pipe_name,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Pipe(HANDLE);

// SAFETY: one owner at a time (moved into the provider / helper loop).
unsafe impl Send for Pipe {}

impl Drop for Pipe {
    fn drop(&mut self) {
        // SAFETY: owned handle.
        unsafe { CloseHandle(self.0) };
    }
}

impl std::fmt::Debug for Pipe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pipe")
    }
}

impl Pipe {
    fn write_all(&self, mut buf: &[u8]) -> bool {
        while !buf.is_empty() {
            let mut n = 0u32;
            // SAFETY: synchronous write from a live buffer.
            if unsafe {
                WriteFile(
                    self.0,
                    buf.as_ptr(),
                    buf.len() as u32,
                    &mut n,
                    std::ptr::null_mut(),
                )
            } == 0
                || n == 0
            {
                return false;
            }
            buf = &buf[n as usize..];
        }
        true
    }

    fn read_exact(&self, buf: &mut [u8]) -> bool {
        let mut off = 0;
        while off < buf.len() {
            let mut n = 0u32;
            // SAFETY: synchronous read into the remaining buffer.
            if unsafe {
                ReadFile(
                    self.0,
                    buf[off..].as_mut_ptr(),
                    (buf.len() - off) as u32,
                    &mut n,
                    std::ptr::null_mut(),
                )
            } == 0
                || n == 0
            {
                return false;
            }
            off += n as usize;
        }
        true
    }

    fn send<T: serde::Serialize>(&self, msg: &T) -> bool {
        self.write_all(&encode(msg))
    }

    fn recv<T: for<'de> serde::Deserialize<'de>>(&self) -> Option<T> {
        let mut len = [0u8; 4];
        if !self.read_exact(&mut len) {
            return None;
        }
        let len = u32::from_le_bytes(len) as usize;
        if len > MAX_FRAME {
            return None;
        }
        let mut body = vec![0u8; len];
        self.read_exact(&mut body).then(|| decode(&body)).flatten()
    }
}

pub(crate) fn image_of(pid: u32) -> Option<String> {
    // SAFETY: query-only handle, closed below.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 32_768];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn same_exe(pid: u32) -> bool {
    let me = std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    matches!((image_of(pid), me), (Some(a), Some(b)) if a.eq_ignore_ascii_case(&b))
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case", tag = "error", content = "detail")]
pub enum StartError {
    /// The user said No to UAC.
    Declined,
    /// The helper did not connect (crashed, blocked).
    NoConnection,
    /// Something else answered on the pipe.
    WrongPeer,
    /// "Start without the Windows prompt" is off, or set up for another build.
    NotSetUp,
    Failed(String),
}

/// Our end of a helper pipe: one instance, local clients only, owner /
/// administrators / SYSTEM only. Fails when the name is already taken.
fn create_pipe(name: &str) -> Result<Pipe, StartError> {
    // owner, administrators and SYSTEM only; protected from inheritance
    let sddl = wide("D:P(A;;GA;;;OW)(A;;GA;;;BA)(A;;GA;;;SY)");
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: valid SDDL string; freed with LocalFree.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(StartError::Failed("security descriptor".into()));
    }
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    let wname = wide(name);
    // SAFETY: one instance, local clients only.
    let h = unsafe {
        CreateNamedPipeW(
            wname.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            64 * 1024,
            64 * 1024,
            0,
            &sa,
        )
    };
    // SAFETY: allocated by the SDDL conversion.
    unsafe { LocalFree(sd) };
    if h == INVALID_HANDLE_VALUE {
        return Err(StartError::Failed("pipe".into()));
    }
    Ok(Pipe(h))
}

/// Wait up to `timeout` for the helper to connect; the client's PID.
fn accept(pipe: &Pipe, name: &str, timeout: Duration) -> Result<u32, StartError> {
    // if the helper never connects, unblock ConnectNamedPipe by connecting ourselves
    let watchdog_name = name.to_owned();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        if rx.recv_timeout(timeout).is_err() {
            let n = wide(&watchdog_name);
            // SAFETY: opening our own pipe; dropped right away.
            let h = unsafe {
                CreateFileW(
                    n.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if h != INVALID_HANDLE_VALUE {
                std::thread::sleep(Duration::from_millis(200));
                // SAFETY: opened above.
                unsafe { CloseHandle(h) };
            }
        }
    });
    // SAFETY: synchronous connect on our pipe.
    let connected = unsafe { ConnectNamedPipe(pipe.0, std::ptr::null_mut()) } != 0
        // SAFETY: no arguments.
        || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
    let _ = tx.send(());
    if !connected {
        return Err(StartError::NoConnection);
    }
    let mut client = 0u32;
    // SAFETY: connected pipe.
    unsafe { GetNamedPipeClientProcessId(pipe.0, &mut client) };
    // SAFETY: no arguments.
    if client == unsafe { GetCurrentProcessId() } {
        return Err(StartError::NoConnection);
    }
    Ok(client)
}

impl Connection {
    fn new(pipe: Pipe) -> Self {
        Self {
            link: Arc::new(Mutex::new(Link { pipe: Some(pipe) })),
        }
    }
}

/// Start the elevated helper (shows UAC) and return the connection to it.
pub fn start() -> Result<Connection, StartError> {
    // SAFETY: no arguments.
    let me = unsafe { GetCurrentProcessId() };
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
        ^ (u64::from(me) << 32);
    let name = pipe_name(me, nonce);
    let pipe = create_pipe(&name)?;

    let exe = std::env::current_exe().map_err(|e| StartError::Failed(e.to_string()))?;
    let file = wide(&exe.to_string_lossy());
    let params = wide(&format!("{HELPER_FLAG} {name} {me}"));
    let verb = wide("runas");
    let mut sei = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: params.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    // SAFETY: initialized struct; blocks until UAC is answered.
    if unsafe { ShellExecuteExW(&mut sei) } == 0 {
        // SAFETY: no arguments.
        return Err(if unsafe { GetLastError() } == ERROR_CANCELLED {
            StartError::Declined
        } else {
            StartError::Failed(std::io::Error::last_os_error().to_string())
        });
    }
    // SAFETY: process handle from ShellExecuteEx.
    let helper_pid = unsafe { GetProcessId(sei.hProcess) };
    // SAFETY: we own it.
    unsafe { CloseHandle(sei.hProcess) };

    let client = accept(&pipe, &name, Duration::from_secs(20))?;
    if client != helper_pid || !same_exe(client) {
        return Err(StartError::WrongPeer);
    }
    Ok(Connection::new(pipe))
}

/// Start the helper through the account's scheduled task — no prompt (ADR 0003).
/// Only when the task is set up for this very build ([`TaskState::On`]); the only
/// client accepted is the administrator-only copy.
pub fn start_task() -> Result<Connection, StartError> {
    use crate::win_task;
    if win_task::state() != TaskState::On {
        return Err(StartError::NotSetUp);
    }
    let sid = win_task::own_sid().ok_or(StartError::NotSetUp)?;
    let copy = win_task::copy_paths(&sid).ok_or(StartError::NotSetUp)?;
    let name = task_pipe_name(&sid);
    let pipe = create_pipe(&name)?;
    win_task::run_task(&sid).map_err(StartError::Failed)?;
    let client = accept(&pipe, &name, Duration::from_secs(10))?;
    let image = image_of(client).map(std::path::PathBuf::from);
    if !image.is_some_and(|i| win_task::same_path(&i, &copy.exe)) {
        return Err(StartError::WrongPeer);
    }
    Ok(Connection::new(pipe))
}

#[derive(Debug)]
struct Link {
    pipe: Option<Pipe>,
}

impl Drop for Link {
    fn drop(&mut self) {
        if let Some(p) = &self.pipe {
            let _ = p.send(&Request::Shutdown);
        }
    }
}

/// The app's end of the pipe, shared by the sensor provider and the monitor (one
/// request / response at a time). The helper is told to exit when the last clone
/// is dropped.
#[derive(Debug, Clone)]
pub struct Connection {
    link: Arc<Mutex<Link>>,
}

impl Connection {
    fn ask(&self, req: &Request) -> Option<Response> {
        let mut link = self.link.lock().ok()?;
        let pipe = link.pipe.as_ref()?;
        let r = if pipe.send(req) { pipe.recv() } else { None };
        if r.is_none() {
            link.pipe = None; // helper gone
        }
        r
    }

    pub fn connected(&self) -> bool {
        self.link.lock().is_ok_and(|l| l.pipe.is_some())
    }

    fn task_request(&self, req: &Request) -> Result<(), String> {
        match self.ask(req) {
            Some(Response::TaskDone) => Ok(()),
            Some(Response::Error { message }) => Err(message),
            Some(_) => Err("protocol".into()),
            None => Err("helper-stopped".into()),
        }
    }

    /// Set up "start without the Windows prompt" for this account (ADR 0003).
    ///
    /// # Errors
    /// The helper's reason as text.
    pub fn task_register(&self) -> Result<(), String> {
        self.task_request(&Request::TaskRegister { protocol: PROTOCOL })
    }

    /// Remove that task and the protected copy.
    ///
    /// # Errors
    /// The helper's reason as text.
    pub fn task_remove(&self) -> Result<(), String> {
        self.task_request(&Request::TaskRemove { protocol: PROTOCOL })
    }

    /// Administrator-only drive reads (Phase 7).
    pub fn health(
        &self,
        drives: &[(u32, bool)],
        volumes: &[String],
    ) -> Option<nfd_health::Privileged> {
        let req = Request::Health {
            protocol: PROTOCOL,
            drives: drives
                .iter()
                .map(|&(index, usb)| crate::HealthDrive { index, usb })
                .collect(),
            volumes: volumes.to_vec(),
        };
        match self.ask(&req)? {
            Response::Health { privileged } => Some(privileged),
            _ => None,
        }
    }
}

/// The app's end: Advanced sensors from the helper, as a normal provider.
#[derive(Debug)]
pub struct RemoteProvider {
    conn: Connection,
    /// Why the helper has no sensors (PawnIO missing, module rejected…).
    pub unavailable: Option<String>,
}

impl RemoteProvider {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            unavailable: None,
        }
    }

    fn ask(&mut self, req: &Request) -> Option<Response> {
        self.conn.ask(req)
    }

    pub fn connected(&self) -> bool {
        self.conn.connected()
    }
}

/// The app's end of the disk activity trace session in the helper.
#[derive(Debug)]
pub struct RemoteMonitor {
    conn: Connection,
    running: bool,
}

impl RemoteMonitor {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            running: false,
        }
    }

    /// Start (or restart, with another `watch` folder) the session.
    ///
    /// # Errors
    /// The session could not start (the reason as text) or the helper is gone.
    pub fn start(&mut self, watch: Option<String>) -> Result<(), String> {
        match self.conn.ask(&Request::MonitorStart {
            protocol: PROTOCOL,
            watch,
        }) {
            Some(Response::MonitorStarted) => {
                self.running = true;
                Ok(())
            }
            Some(Response::MonitorError { error }) => Err(error),
            Some(_) => Err("protocol".into()),
            None => Err("helper-stopped".into()),
        }
    }

    /// Activity since the previous poll.
    ///
    /// # Errors
    /// The session stopped or the helper is gone.
    pub fn poll(&mut self) -> Result<nfd_monitor::Tick, String> {
        match self.conn.ask(&Request::MonitorPoll) {
            Some(Response::Monitor { tick }) => Ok(tick),
            Some(Response::MonitorError { error }) => Err(error),
            Some(_) => Err("protocol".into()),
            None => Err("helper-stopped".into()),
        }
    }

    pub fn stop(&mut self) {
        if std::mem::take(&mut self.running) {
            let _ = self.conn.ask(&Request::MonitorStop);
        }
    }

    pub fn connected(&self) -> bool {
        self.conn.connected()
    }
}

impl Drop for RemoteMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Sensor sources are static strings; the helper's are all PawnIO.
const REMOTE_SOURCE: &str = "PawnIO (elevated helper)";

impl SensorProvider for RemoteProvider {
    fn id(&self) -> &'static str {
        "helper"
    }

    /// The helper's sources are rates too (RAPL power, APERF clocks).
    fn warmup(&self) -> Duration {
        nfd_sensors::RATE_WARMUP
    }

    fn discover(&mut self) -> Discovery {
        match self.ask(&Request::Discover { protocol: PROTOCOL }) {
            Some(Response::Discovery {
                protocol,
                devices,
                sensors,
                notes,
            }) if protocol == PROTOCOL => Discovery {
                devices,
                sensors: sensors
                    .into_iter()
                    .map(|w| SensorInfo {
                        id: w.id,
                        device: w.device,
                        kind: w.kind,
                        label: w.label,
                        index: w.index,
                        detail: w.detail,
                        primary: w.primary,
                        estimated: w.estimated,
                        source: REMOTE_SOURCE,
                    })
                    .collect(),
                notes,
            },
            Some(Response::Unavailable { reason }) => {
                self.unavailable = Some(reason);
                Discovery::default()
            }
            _ => {
                self.unavailable = Some("protocol".into());
                Discovery::default()
            }
        }
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        if let Some(Response::Sample { values }) = self.ask(&Request::Sample) {
            out.extend(values);
        }
    }
}

/// Libraries this elevated process loads at run time come from System32 only
/// (its static imports are all Windows DLLs).
fn harden_dll_search() {
    // SAFETY: plain flag call.
    unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) };
}

/// Open the app's pipe as its client; the server's PID.
fn open_app_pipe(name: &str) -> Option<(Pipe, u32)> {
    let wname = wide(name);
    // SAFETY: plain client open of the app's pipe.
    let h = unsafe {
        CreateFileW(
            wname.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return None;
    }
    let pipe = Pipe(h);
    let mut server = 0u32;
    // SAFETY: connected pipe.
    unsafe { GetNamedPipeServerProcessId(pipe.0, &mut server) };
    Some((pipe, server))
}

/// The helper started through UAC (`nanofreedisk.exe --elevated-helper <pipe>
/// <pid>`). Returns the process exit code.
pub fn run_helper(args: &[String]) -> i32 {
    harden_dll_search();
    let Some((name, parent)) = crate::helper_args(args) else {
        return 2;
    };
    // before anything else: the image a TaskRegister would copy (ADR 0003)
    let own = crate::win_task::lock_own_image();
    let Some((pipe, server)) = open_app_pipe(&name) else {
        return 3;
    };
    // only the app that started us, running the same executable
    if server != parent || !same_exe(server) {
        return 4;
    }
    serve(&pipe, own.as_ref())
}

/// The helper started by the account's scheduled task (`<Program Files>\NANOFreeDisk
/// Helper\<SID>\nanofreedisk.exe --elevated-helper-task`, ADR 0003). It serves an
/// app of its own account whose executable hashes like itself, nothing else.
pub fn run_task_helper(args: &[String]) -> i32 {
    use crate::win_task;
    harden_dll_search();
    if !crate::task::task_args(args) {
        return 2;
    }
    let Some(sid) = win_task::own_sid() else {
        return 2;
    };
    let Some(copy) = win_task::copy_paths(&sid) else {
        return 2;
    };
    // only ever as the protected copy
    let me = std::env::current_exe().ok();
    if !me.is_some_and(|m| win_task::same_path(&m, &copy.exe)) {
        return 2;
    }
    let Some((pipe, server)) = open_app_pipe(&task_pipe_name(&sid)) else {
        return 3;
    };
    if win_task::process_sid(Some(server)).as_deref() != Some(sid.as_str()) {
        return 4;
    }
    let mine = win_task::file_sha256(&copy.exe);
    let theirs = image_of(server).and_then(|p| win_task::file_sha256(std::path::Path::new(&p)));
    if mine.is_none() || mine != theirs {
        return 4; // another build (stale copy) or another program
    }
    serve(&pipe, None)
}

/// Answer the app's requests until it closes the pipe or says Shutdown. `own`: the
/// helper's locked image when it runs through UAC (what TaskRegister copies).
fn serve(pipe: &Pipe, own: Option<&std::fs::File>) -> i32 {
    let mut providers: Vec<Box<dyn SensorProvider>> = Vec::new();
    let mut maps: Vec<usize> = Vec::new(); // provider start offsets
    // the monitor's trace session; dropping it (return, app gone) stops it
    let mut etw: Option<nfd_monitor::win::etw::EtwSession> = None;
    // the last hardware read, served again to faster requests
    let mut last: Option<(Instant, Vec<(usize, f64)>)> = None;
    loop {
        let Some(req) = pipe.recv::<Request>() else {
            return 0; // app closed the pipe
        };
        let resp = match req {
            Request::Shutdown => return 0,
            Request::MonitorStart { protocol, .. } if protocol != PROTOCOL => {
                Response::MonitorError {
                    error: format!("protocol {protocol} != {PROTOCOL}"),
                }
            }
            Request::MonitorStart { watch, .. } => {
                etw = None; // stop a previous session first (same name)
                match nfd_monitor::win::etw::EtwSession::start(watch.as_deref()) {
                    Ok(s) => {
                        etw = Some(s);
                        Response::MonitorStarted
                    }
                    Err(e) => Response::MonitorError {
                        error: format!("{e:?}"),
                    },
                }
            }
            Request::MonitorPoll => match etw.as_mut() {
                Some(s) if s.alive() => {
                    let mut tick = s.take();
                    tick.trim_files(crate::MONITOR_FILES);
                    Response::Monitor { tick }
                }
                Some(_) => {
                    etw = None;
                    Response::MonitorError {
                        error: "session-stopped".into(),
                    }
                }
                None => Response::MonitorError {
                    error: "not-started".into(),
                },
            },
            Request::MonitorStop => {
                etw = None;
                Response::MonitorStopped
            }
            Request::Discover { protocol } if protocol != PROTOCOL => Response::Error {
                message: format!("protocol {protocol} != {PROTOCOL}"),
            },
            Request::Discover { .. } => match nfd_sensors::win::pawnio_providers() {
                Err(e) => Response::Unavailable {
                    reason: serde_json::to_value(e)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                },
                Ok(list) => {
                    providers = list;
                    maps.clear();
                    last = None;
                    let (mut devices, mut sensors, mut notes) =
                        (Vec::new(), Vec::new(), Vec::new());
                    for p in &mut providers {
                        let d = p.discover();
                        maps.push(sensors.len());
                        devices.extend(d.devices);
                        notes.extend(d.notes);
                        sensors.extend(d.sensors.into_iter().map(|s| WireSensor {
                            id: s.id,
                            device: s.device,
                            kind: s.kind,
                            label: s.label,
                            index: s.index,
                            detail: s.detail,
                            primary: s.primary,
                            estimated: s.estimated,
                        }));
                    }
                    Response::Discovery {
                        protocol: PROTOCOL,
                        devices,
                        sensors,
                        notes,
                    }
                }
            },
            Request::Health { protocol, .. } if protocol != PROTOCOL => Response::Error {
                message: format!("protocol {protocol} != {PROTOCOL}"),
            },
            Request::Health {
                drives, volumes, ..
            } => {
                // bounded: no more drives / volumes than a PC can have
                let drives: Vec<(u32, bool)> = drives
                    .iter()
                    .filter(|d| d.index < 64)
                    .take(64)
                    .map(|d| (d.index, d.usb))
                    .collect();
                let volumes: Vec<String> = volumes.into_iter().take(26).collect();
                Response::Health {
                    privileged: nfd_health::privileged_for(&drives, &volumes),
                }
            }
            Request::Sample => match &last {
                Some((at, values)) if at.elapsed() < crate::MIN_SAMPLE_INTERVAL => {
                    Response::Sample {
                        values: values.clone(),
                    }
                }
                _ => {
                    let mut values = Vec::new();
                    let mut buf = Vec::new();
                    for (p, &off) in providers.iter_mut().zip(&maps) {
                        buf.clear();
                        p.sample(&mut buf);
                        values.extend(buf.iter().map(|&(i, v)| (off + i, v)));
                    }
                    last = Some((Instant::now(), values.clone()));
                    Response::Sample { values }
                }
            },
            Request::TaskRegister { protocol } | Request::TaskRemove { protocol }
                if protocol != PROTOCOL =>
            {
                Response::Error {
                    message: format!("protocol {protocol} != {PROTOCOL}"),
                }
            }
            Request::TaskRegister { .. } => match crate::win_task::register(own) {
                Ok(()) => Response::TaskDone,
                Err(message) => Response::Error { message },
            },
            Request::TaskRemove { .. } => match crate::win_task::remove() {
                Ok(()) => Response::TaskDone,
                Err(message) => Response::Error { message },
            },
        };
        if !pipe.send(&resp) {
            return 0;
        }
    }
}
