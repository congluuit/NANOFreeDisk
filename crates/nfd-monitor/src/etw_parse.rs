//! Decoding of the kernel `FileIo` and `Thread` events (classic MOF layouts) and
//! the [`Collector`] that turns them into a [`Tick`]. Pure code: the Windows
//! session in `win::etw` only hands over the header fields and the payload, so
//! everything here is tested with synthetic records on any OS.
//!
//! Layouts by event version (P = pointer size, 8 on x64):
//!
//! | event (opcode) | v2 | v3 (Windows 8+) |
//! |---|---|---|
//! | Create (64) | Irp P, TTID P, FileObject P, Options, Attributes, Share, OpenPath | Irp P, FileObject P, TTID u32, Options, Attributes, Share, OpenPath |
//! | Read / Write (67/68) | Offset u64, Irp P, TTID P, FileObject P, FileKey P, IoSize, IoFlags | Offset u64, Irp P, FileObject P, FileKey P, TTID u32, IoSize, IoFlags |
//! | Cleanup / Close (65/66) | Irp P, TTID P, FileObject P, FileKey P | Irp P, FileObject P, FileKey P, TTID u32 |
//! | SetInfo / Delete / Rename (69/70/71) | Irp P, TTID P, FileObject P, FileKey P, Extra P, Class | Irp P, FileObject P, FileKey P, Extra P, TTID u32, Class |
//! | DeletePath / RenamePath / SetLinkPath (79/80/81) | — | as SetInfo v3, then FileName |
//! | Name / FileCreate / FileDelete / FileRundown (0/32/35/36) | FileKey P, FileName | same |
//! | Thread Start / End / DCStart (1/2/3) | ProcessId u32, ThreadId u32, … | same |
//!
//! Any other version or a truncated payload is *undecoded* (counted, skipped).

use std::collections::HashMap;
use std::sync::Arc;

use crate::{Counts, FileCounts, ProcCounts, Tick, Touch, TouchOp};

/// Which kernel provider an event comes from (by its provider GUID).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    FileIo,
    Thread,
    Other,
}

/// The header fields the decoder needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub provider: Provider,
    pub opcode: u8,
    pub version: u8,
    /// `u32::MAX` when the kernel did not know the process.
    pub pid: u32,
    pub tid: u32,
    /// 64-bit pointers in the payload (`EVENT_HEADER_FLAG_64_BIT_HEADER`).
    pub ptr64: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameOp {
    Name,
    Create,
    Delete,
    Rundown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoOp {
    SetInfo,
    Delete,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathOp {
    Delete,
    Rename,
    SetLink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Name {
        op: NameOp,
        key: u64,
        path: String,
    },
    Create {
        tid: u32,
        object: u64,
        options: u32,
        path: String,
    },
    Rw {
        write: bool,
        tid: u32,
        object: u64,
        key: u64,
        size: u32,
        /// IRP flags (`IRP_PAGING_IO`…).
        flags: u32,
    },
    Close {
        object: u64,
    },
    Info {
        op: InfoOp,
        tid: u32,
        object: u64,
        key: u64,
    },
    Path {
        op: PathOp,
        tid: u32,
        object: u64,
        key: u64,
        path: String,
    },
    ThreadStart {
        pid: u32,
        tid: u32,
    },
    ThreadEnd {
        tid: u32,
    },
}

/// The payload does not match the layout of its version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Undecoded;

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
    ptr: usize,
}

impl Reader<'_> {
    fn u32(&mut self) -> Result<u32, Undecoded> {
        let v = self.b.get(self.at..self.at + 4).ok_or(Undecoded)?;
        self.at += 4;
        Ok(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
    }

    fn u64(&mut self) -> Result<u64, Undecoded> {
        let v = self.b.get(self.at..self.at + 8).ok_or(Undecoded)?;
        self.at += 8;
        let mut a = [0u8; 8];
        a.copy_from_slice(v);
        Ok(u64::from_le_bytes(a))
    }

    fn ptr(&mut self) -> Result<u64, Undecoded> {
        if self.ptr == 8 {
            self.u64()
        } else {
            self.u32().map(u64::from)
        }
    }

    /// NUL-terminated UTF-16LE (or the rest of the payload).
    fn wstr(&mut self) -> Result<String, Undecoded> {
        let rest = self.b.get(self.at..).ok_or(Undecoded)?;
        let mut units = Vec::with_capacity(rest.len() / 2);
        for c in rest.chunks_exact(2) {
            let u = u16::from_le_bytes([c[0], c[1]]);
            if u == 0 {
                break;
            }
            units.push(u);
        }
        self.at = self.b.len();
        Ok(String::from_utf16_lossy(&units))
    }
}

/// Decode one event. `Ok(None)`: an event this monitor does not use.
pub fn decode(h: &Header, data: &[u8]) -> Result<Option<Event>, Undecoded> {
    let mut r = Reader {
        b: data,
        at: 0,
        ptr: if h.ptr64 { 8 } else { 4 },
    };
    match h.provider {
        Provider::Other => Ok(None),
        Provider::Thread => match h.opcode {
            1 | 3 => {
                let pid = r.u32()?;
                let tid = r.u32()?;
                Ok(Some(Event::ThreadStart { pid, tid }))
            }
            2 => {
                r.u32()?;
                Ok(Some(Event::ThreadEnd { tid: r.u32()? }))
            }
            _ => Ok(None),
        },
        Provider::FileIo => {
            let v3 = match h.version {
                2 => false,
                3..=4 => true,
                _ if matches!(h.opcode, 0 | 32 | 35 | 36) && h.version <= 4 => false,
                _ => return Err(Undecoded),
            };
            // TTID: a pointer before FileObject in v2, a u32 after the pointers in v3
            match h.opcode {
                0 | 32 | 35 | 36 => {
                    let key = r.ptr()?;
                    let path = r.wstr()?;
                    let op = match h.opcode {
                        0 => NameOp::Name,
                        32 => NameOp::Create,
                        35 => NameOp::Delete,
                        _ => NameOp::Rundown,
                    };
                    Ok(Some(Event::Name { op, key, path }))
                }
                64 => {
                    r.ptr()?; // Irp
                    let (tid, object) = if v3 {
                        let o = r.ptr()?;
                        (r.u32()?, o)
                    } else {
                        let t = r.ptr()? as u32;
                        (t, r.ptr()?)
                    };
                    let options = r.u32()?;
                    r.u32()?; // attributes
                    r.u32()?; // share access
                    let path = r.wstr()?;
                    Ok(Some(Event::Create {
                        tid,
                        object,
                        options,
                        path,
                    }))
                }
                67 | 68 => {
                    r.u64()?; // offset
                    r.ptr()?; // Irp
                    let (tid, object, key) = if v3 {
                        let o = r.ptr()?;
                        let k = r.ptr()?;
                        (r.u32()?, o, k)
                    } else {
                        let t = r.ptr()? as u32;
                        let o = r.ptr()?;
                        (t, o, r.ptr()?)
                    };
                    let size = r.u32()?;
                    let flags = r.u32()?;
                    Ok(Some(Event::Rw {
                        write: h.opcode == 68,
                        tid,
                        object,
                        key,
                        size,
                        flags,
                    }))
                }
                65 | 66 => {
                    r.ptr()?;
                    if !v3 {
                        r.ptr()?;
                    }
                    let object = r.ptr()?;
                    r.ptr()?; // key
                    Ok(Some(Event::Close { object }))
                }
                69..=71 | 79..=81 => {
                    r.ptr()?; // Irp
                    let (tid, object, key) = if v3 {
                        let o = r.ptr()?;
                        let k = r.ptr()?;
                        r.ptr()?; // extra
                        let t = r.u32()?;
                        r.u32()?; // class
                        (t, o, k)
                    } else {
                        let t = r.ptr()? as u32;
                        let o = r.ptr()?;
                        let k = r.ptr()?;
                        r.ptr()?;
                        r.u32()?;
                        (t, o, k)
                    };
                    match h.opcode {
                        69 => Ok(Some(Event::Info {
                            op: InfoOp::SetInfo,
                            tid,
                            object,
                            key,
                        })),
                        70 => Ok(Some(Event::Info {
                            op: InfoOp::Delete,
                            tid,
                            object,
                            key,
                        })),
                        71 => Ok(Some(Event::Info {
                            op: InfoOp::Rename,
                            tid,
                            object,
                            key,
                        })),
                        op => {
                            if !v3 {
                                return Err(Undecoded);
                            }
                            let path = r.wstr()?;
                            Ok(Some(Event::Path {
                                op: match op {
                                    79 => PathOp::Delete,
                                    80 => PathOp::Rename,
                                    _ => PathOp::SetLink,
                                },
                                tid,
                                object,
                                key,
                                path,
                            }))
                        }
                    }
                }
                _ => Ok(None),
            }
        }
    }
}

/// `CreateOptions` high byte = disposition; these may create or replace a file.
fn creates(options: u32) -> bool {
    matches!(options >> 24, 0 | 2 | 4 | 5) // SUPERSEDE, CREATE, OVERWRITE, OVERWRITE_IF
}

/// NT device paths of the volumes with a drive letter: `(\Device\HarddiskVolume3, C:)`.
#[derive(Debug, Clone, Default)]
pub struct Devices {
    /// Longest device name first, lowercase.
    map: Vec<(String, String)>,
}

impl Devices {
    pub fn new(mut list: Vec<(String, String)>) -> Self {
        for (d, _) in &mut list {
            *d = d.to_ascii_lowercase();
        }
        list.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
        Self { map: list }
    }

    /// `\Device\HarddiskVolume3\x\y` → `C:\x\y`; `None` for anything that is not a
    /// file on a lettered local volume (pipes, network, volumes without letters).
    pub fn to_win32(&self, device_path: &str) -> Option<String> {
        let bytes = device_path.as_bytes();
        for (dev, letter) in &self.map {
            let n = dev.len();
            if bytes.len() >= n
                && device_path.is_char_boundary(n)
                && device_path[..n].eq_ignore_ascii_case(dev)
                && (bytes.len() == n || bytes[n] == b'\\')
            {
                let rest = &device_path[n..];
                return Some(if rest.is_empty() {
                    format!("{letter}\\")
                } else {
                    format!("{letter}{rest}")
                });
            }
        }
        None
    }

    /// A path as a process opened it may be in DOS-device form (`\??\C:\x`, the
    /// name handed to `NtCreateFile`): turn it into the device form events use
    /// elsewhere. Anything else is returned unchanged.
    pub fn normalize(&self, path: String) -> String {
        let Some(rest) = path
            .get(..4)
            .filter(|p| p.eq_ignore_ascii_case(r"\??\"))
            .and_then(|_| path.get(4..))
        else {
            return path;
        };
        let Some(letter) = rest.get(..2).filter(|l| l.ends_with(':')) else {
            return path;
        };
        match self
            .map
            .iter()
            .find(|(_, l)| l.eq_ignore_ascii_case(letter))
        {
            Some((dev, _)) => format!("{dev}{}", &rest[2..]),
            None => path,
        }
    }

    /// `C:\x` → `\device\harddiskvolume3\x` (lowercase), for matching device paths.
    pub fn to_device(&self, win32: &str) -> Option<String> {
        let (letter, rest) = win32.split_at_checked(2)?;
        let (dev, _) = self
            .map
            .iter()
            .find(|(_, l)| l.eq_ignore_ascii_case(letter))?;
        Some(format!("{dev}{}", rest.trim_end_matches('\\')).to_ascii_lowercase())
    }
}

/// Map sizes past which a map is cleared (a leak guard; names come back with the
/// next Create / Name event).
const MAX_NAMES: usize = 200_000;
const MAX_TOUCHES: usize = 500;

/// Accumulates decoded events between two ticks. Runs inside the ETW callback, so
/// a Read / Write costs a few hash lookups and no allocation once a file is known.
#[derive(Debug, Default)]
pub struct Collector {
    devices: Devices,
    /// Lowercase device paths of the watched folder (its long name, and the
    /// spellings a process may open it by, such as an 8.3 short name).
    watch: Vec<String>,
    by_key: HashMap<u64, Arc<str>>,
    by_object: HashMap<u64, Arc<str>>,
    threads: HashMap<u32, u32>,
    files: HashMap<(u32, Arc<str>), Counts>,
    /// Read / write of a file whose name has not been seen yet: (pid, key, object).
    pending: HashMap<(u32, u64, u64), Counts>,
    touches: Vec<Touch>,
    events: u64,
    undecoded: u64,
    /// The last few Create events as decoded (diagnostics).
    recent_creates: std::collections::VecDeque<String>,
}

/// `IRP_PAGING_IO`: the cache or memory manager moving pages (a flush, a mapped
/// view, a page fault), not a request of the process — counting it would count a
/// flushed write twice.
const IRP_PAGING_IO: u32 = 0x2;

fn under(prefix: &str, device_path: &str) -> bool {
    device_path.len() > prefix.len()
        && device_path.is_char_boundary(prefix.len())
        && device_path[..prefix.len()].eq_ignore_ascii_case(prefix)
        && device_path.as_bytes()[prefix.len()] == b'\\'
}

impl Collector {
    /// `watch`: Win32 spellings of one folder (see [`Collector::watch`]).
    pub fn new(devices: Devices, watch: &[String]) -> Self {
        let mut w: Vec<String> = watch.iter().filter_map(|w| devices.to_device(w)).collect();
        w.sort();
        w.dedup();
        Self {
            devices,
            watch: w,
            ..Self::default()
        }
    }

    /// The last Create events as decoded (version, PID, options, path), for
    /// diagnosing a Windows build whose events do not match.
    pub fn recent_creates(&self) -> Vec<String> {
        self.recent_creates.iter().cloned().collect()
    }

    /// Watching a folder ("Who touched this?")?
    pub fn watching(&self) -> bool {
        !self.watch.is_empty()
    }

    fn pid(&self, h: &Header, tid: u32) -> Option<u32> {
        if h.pid != u32::MAX {
            return Some(h.pid);
        }
        self.threads.get(&tid).copied()
    }

    fn under_watch(&self, device_path: &str) -> bool {
        self.watch.iter().any(|w| under(w, device_path))
    }

    fn touch(&mut self, now_ms: u64, pid: u32, op: TouchOp, path: &str, target: Option<&str>) {
        if self.touches.len() >= MAX_TOUCHES || !self.under_watch(path) {
            return;
        }
        let Some(p) = self.devices.to_win32(path) else {
            return;
        };
        self.touches.push(Touch {
            at: now_ms,
            pid,
            op,
            path: p,
            target: target.and_then(|t| self.devices.to_win32(t)),
        });
    }

    fn name_of(&self, key: u64, object: u64) -> Option<Arc<str>> {
        self.by_key
            .get(&key)
            .or_else(|| self.by_object.get(&object))
            .cloned()
    }

    /// Feed one raw event.
    pub fn event(&mut self, h: &Header, data: &[u8], now_ms: u64) {
        let ev = match decode(h, data) {
            Ok(Some(ev)) => ev,
            Ok(None) => return,
            Err(Undecoded) => {
                self.undecoded += 1;
                return;
            }
        };
        if self.by_key.len() > MAX_NAMES {
            self.by_key.clear();
        }
        if self.by_object.len() > MAX_NAMES {
            self.by_object.clear();
        }
        match ev {
            Event::ThreadStart { pid, tid } => {
                self.threads.insert(tid, pid);
            }
            Event::ThreadEnd { tid } => {
                self.threads.remove(&tid);
            }
            Event::Name { op, key, path } => match op {
                NameOp::Delete => {
                    self.by_key.remove(&key);
                }
                _ => {
                    self.by_key.insert(key, Arc::from(path));
                }
            },
            Event::Create {
                tid,
                object,
                options,
                path,
            } => {
                let path = self.devices.normalize(path);
                if self.recent_creates.len() >= 16 {
                    self.recent_creates.pop_front();
                }
                self.recent_creates.push_back(format!(
                    "v{} pid {} opt {options:#x} {path}",
                    h.version, h.pid
                ));
                if creates(options) {
                    if let Some(pid) = self.pid(h, tid) {
                        self.touch(now_ms, pid, TouchOp::Create, &path, None);
                    }
                }
                self.by_object.insert(object, Arc::from(path));
            }
            Event::Close { object } => {
                self.by_object.remove(&object);
            }
            Event::Rw {
                write,
                tid,
                object,
                key,
                size,
                flags,
            } => {
                let Some(pid) = self.pid(h, tid) else { return };
                self.events += 1;
                if flags & IRP_PAGING_IO != 0 {
                    return;
                }
                let c = match self.name_of(key, object) {
                    Some(name) => self.files.entry((pid, name)).or_default(),
                    None => self.pending.entry((pid, key, object)).or_default(),
                };
                if write {
                    c.write += u64::from(size);
                    c.write_ops += 1;
                } else {
                    c.read += u64::from(size);
                    c.read_ops += 1;
                }
            }
            Event::Info {
                op,
                tid,
                object,
                key,
            } => {
                if self.watch.is_empty() || op == InfoOp::SetInfo {
                    return;
                }
                let (Some(pid), Some(name)) = (self.pid(h, tid), self.name_of(key, object)) else {
                    return;
                };
                let op = if op == InfoOp::Delete {
                    TouchOp::Delete
                } else {
                    TouchOp::Rename
                };
                self.touch(now_ms, pid, op, &name, None);
            }
            Event::Path {
                op,
                tid,
                object,
                key,
                path,
            } => {
                let Some(pid) = self.pid(h, tid) else { return };
                let path = self.devices.normalize(path);
                let old = self.name_of(key, object);
                match op {
                    PathOp::Delete => self.touch(now_ms, pid, TouchOp::Delete, &path, None),
                    PathOp::Rename => {
                        if let Some(old) = old {
                            self.touch(now_ms, pid, TouchOp::Rename, &old, Some(&path));
                        }
                    }
                    PathOp::SetLink => {}
                }
            }
        }
    }

    /// Everything since the last call, with Win32 paths; files on volumes without a
    /// drive letter, pipes and network files are left out.
    pub fn take(&mut self, elapsed_ms: u64, lost: u64, now_ms: u64) -> Tick {
        // names that arrived after the I/O
        for ((pid, key, object), c) in std::mem::take(&mut self.pending) {
            if let Some(name) = self.name_of(key, object) {
                self.files.entry((pid, name)).or_default().add(&c);
            }
        }
        let mut procs: HashMap<u32, Counts> = HashMap::new();
        let mut files = Vec::with_capacity(self.files.len());
        let mut converted: HashMap<Arc<str>, Option<String>> = HashMap::new();
        let mut write_touches = Vec::new();
        for ((pid, dev), c) in self.files.drain() {
            let path = converted
                .entry(dev.clone())
                .or_insert_with(|| self.devices.to_win32(&dev))
                .clone();
            let Some(path) = path else { continue };
            procs.entry(pid).or_default().add(&c);
            if c.write > 0
                && write_touches.len() < MAX_TOUCHES
                && self.watch.iter().any(|w| under(w, &dev))
            {
                write_touches.push(Touch {
                    at: now_ms,
                    pid,
                    op: TouchOp::Write,
                    path: path.clone(),
                    target: None,
                });
            }
            files.push(FileCounts {
                pid,
                path,
                counts: c,
            });
        }
        let mut touches = dedup_touches(std::mem::take(&mut self.touches));
        touches.extend(write_touches);
        touches.sort_by_key(|t| t.at);
        let mut procs: Vec<ProcCounts> = procs
            .into_iter()
            .map(|(pid, counts)| ProcCounts {
                pid,
                create_time: 0,
                counts,
            })
            .collect();
        procs.sort_by_key(|p| p.pid);
        Tick {
            elapsed_ms,
            procs,
            files,
            touches,
            events: std::mem::take(&mut self.events),
            lost,
            undecoded: self.undecoded,
        }
    }
}

/// Windows 10+ logs a delete or rename twice (the request, then the path
/// operation with the new name): keep one of each, the one with the new name.
fn dedup_touches(mut v: Vec<Touch>) -> Vec<Touch> {
    let named: std::collections::HashSet<(u32, String)> = v
        .iter()
        .filter(|t| t.op == TouchOp::Rename && t.target.is_some())
        .map(|t| (t.pid, t.path.to_ascii_lowercase()))
        .collect();
    v.retain(|t| {
        !(t.op == TouchOp::Rename
            && t.target.is_none()
            && named.contains(&(t.pid, t.path.to_ascii_lowercase())))
    });
    let mut seen = std::collections::HashSet::new();
    v.retain(|t| {
        seen.insert((
            t.pid,
            t.op,
            t.path.to_ascii_lowercase(),
            t.target.as_deref().map(str::to_ascii_lowercase),
        ))
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr(provider: Provider, opcode: u8, version: u8, pid: u32) -> Header {
        Header {
            provider,
            opcode,
            version,
            pid,
            tid: 77,
            ptr64: true,
        }
    }

    /// Little-endian payload builder.
    #[derive(Default)]
    struct B(Vec<u8>);
    impl B {
        fn p(mut self, v: u64) -> Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn d(mut self, v: u32) -> Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn s(mut self, s: &str) -> Self {
            for u in s.encode_utf16().chain(std::iter::once(0)) {
                self.0.extend(u.to_le_bytes());
            }
            self
        }
    }

    const VOL: &str = r"\Device\HarddiskVolume3";

    fn devices() -> Devices {
        Devices::new(vec![
            (VOL.into(), "C:".into()),
            (r"\Device\HarddiskVolume30".into(), "D:".into()),
        ])
    }

    fn rw_v3(write: bool, object: u64, key: u64, size: u32) -> (Header, Vec<u8>) {
        (
            hdr(Provider::FileIo, if write { 68 } else { 67 }, 3, 1234),
            B::default()
                .p(4096) // offset
                .p(0xAAAA) // irp
                .p(object)
                .p(key)
                .d(77) // ttid
                .d(size)
                .d(0)
                .0,
        )
    }

    fn name(key: u64, path: &str) -> (Header, Vec<u8>) {
        (
            hdr(Provider::FileIo, 0, 2, u32::MAX),
            B::default().p(key).s(path).0,
        )
    }

    #[test]
    fn read_write_v2_and_v3_layouts() {
        let (h, d) = rw_v3(true, 0x10, 0x20, 512);
        assert_eq!(
            decode(&h, &d),
            Ok(Some(Event::Rw {
                write: true,
                tid: 77,
                object: 0x10,
                key: 0x20,
                size: 512,
                flags: 0
            }))
        );
        let v2 = B::default()
            .p(0)
            .p(0xAAAA)
            .p(88) // ttid as pointer
            .p(0x10)
            .p(0x20)
            .d(100)
            .d(0x43)
            .0;
        assert_eq!(
            decode(&hdr(Provider::FileIo, 67, 2, 1), &v2),
            Ok(Some(Event::Rw {
                write: false,
                tid: 88,
                object: 0x10,
                key: 0x20,
                size: 100,
                flags: 0x43
            }))
        );
    }

    #[test]
    fn create_name_and_path_events() {
        let v3 = B::default()
            .p(1)
            .p(0x55)
            .d(9)
            .d(2 << 24)
            .d(0x80)
            .d(7)
            .s(r"\Device\HarddiskVolume3\x\new.txt")
            .0;
        assert_eq!(
            decode(&hdr(Provider::FileIo, 64, 3, 1), &v3),
            Ok(Some(Event::Create {
                tid: 9,
                object: 0x55,
                options: 2 << 24,
                path: r"\Device\HarddiskVolume3\x\new.txt".into()
            }))
        );
        let v2 = B::default().p(1).p(9).p(0x55).d(1 << 24).d(0).d(0).s("p").0;
        assert!(matches!(
            decode(&hdr(Provider::FileIo, 64, 2, 1), &v2),
            Ok(Some(Event::Create {
                tid: 9,
                object: 0x55,
                ..
            }))
        ));
        let (h, d) = name(0x20, r"\Device\HarddiskVolume3\a.log");
        assert!(matches!(
            decode(&h, &d),
            Ok(Some(Event::Name {
                op: NameOp::Name,
                key: 0x20,
                ..
            }))
        ));
        let ren = B::default()
            .p(1)
            .p(0x10)
            .p(0x20)
            .p(0)
            .d(9)
            .d(10)
            .s(r"\Device\HarddiskVolume3\b.log")
            .0;
        assert!(matches!(
            decode(&hdr(Provider::FileIo, 80, 3, 1), &ren),
            Ok(Some(Event::Path {
                op: PathOp::Rename,
                tid: 9,
                key: 0x20,
                ..
            }))
        ));
        let th = B::default().d(500).d(600).p(0).0;
        assert_eq!(
            decode(&hdr(Provider::Thread, 3, 4, u32::MAX), &th),
            Ok(Some(Event::ThreadStart { pid: 500, tid: 600 }))
        );
    }

    #[test]
    fn truncated_or_unknown_versions_are_undecoded() {
        let (h, d) = rw_v3(true, 1, 2, 3);
        assert_eq!(decode(&h, &d[..20]), Err(Undecoded));
        let mut h9 = h;
        h9.version = 9;
        assert_eq!(decode(&h9, &d), Err(Undecoded));
        assert_eq!(
            decode(&hdr(Provider::FileIo, 74, 3, 1), &d),
            Ok(None),
            "QueryInfo is ignored"
        );
        assert_eq!(decode(&hdr(Provider::Other, 10, 2, 1), &d), Ok(None));
    }

    #[test]
    fn device_paths_map_to_drive_letters() {
        let dv = devices();
        assert_eq!(
            dv.to_win32(r"\Device\HarddiskVolume3\Users\a.txt")
                .as_deref(),
            Some(r"C:\Users\a.txt")
        );
        assert_eq!(
            dv.to_win32(r"\Device\HarddiskVolume30\x").as_deref(),
            Some(r"D:\x"),
            "the longer device wins"
        );
        assert_eq!(
            dv.to_win32(r"\device\harddiskvolume3").as_deref(),
            Some(r"C:\")
        );
        assert_eq!(dv.to_win32(r"\Device\HarddiskVolume33\x"), None);
        assert_eq!(dv.to_win32(r"\Device\NamedPipe\x"), None);
        assert_eq!(dv.to_win32(r"\Device\Mup\server\share\f"), None);
        assert_eq!(
            dv.to_device(r"C:\Users\").as_deref(),
            Some(r"\device\harddiskvolume3\users")
        );
        assert_eq!(dv.to_device("Q:\\x"), None);
        assert_eq!(
            dv.to_win32(&dv.normalize(r"\??\C:\Users\a.txt".into()))
                .as_deref(),
            Some(r"C:\Users\a.txt")
        );
        assert_eq!(dv.normalize(r"\??\Q:\x".into()), r"\??\Q:\x");
        assert_eq!(
            dv.normalize(r"\Device\NamedPipe\p".into()),
            r"\Device\NamedPipe\p"
        );
    }

    #[test]
    fn collector_attributes_io_to_files_and_processes() {
        let mut c = Collector::new(devices(), &[]);
        let (h, d) = name(0x20, r"\Device\HarddiskVolume3\logs\a.log");
        c.event(&h, &d, 0);
        for _ in 0..3 {
            let (h, d) = rw_v3(true, 0x10, 0x20, 1000);
            c.event(&h, &d, 0);
        }
        // I/O before the name is known is resolved at take()
        let (h, d) = rw_v3(false, 0x11, 0x21, 50);
        c.event(&h, &d, 0);
        let (h, d) = name(0x21, r"\Device\HarddiskVolume3\data.bin");
        c.event(&h, &d, 0);
        // a pipe: never a file
        let (h, d) = name(0x22, r"\Device\NamedPipe\chrome.1");
        c.event(&h, &d, 0);
        let (h, d) = rw_v3(true, 0x12, 0x22, 9999);
        c.event(&h, &d, 0);
        // unknown key: dropped
        let (h, d) = rw_v3(true, 0x13, 0x23, 7);
        c.event(&h, &d, 0);
        let t = c.take(1000, 3, 0);
        assert_eq!(t.events, 6);
        assert_eq!(t.lost, 3);
        assert_eq!(t.procs.len(), 1);
        assert_eq!(t.procs[0].pid, 1234);
        assert_eq!(t.procs[0].counts.write, 3000);
        assert_eq!(t.procs[0].counts.read, 50);
        assert_eq!(t.procs[0].counts.write_ops, 3);
        let mut paths: Vec<&str> = t.files.iter().map(|f| f.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, vec![r"C:\data.bin", r"C:\logs\a.log"]);
        let again = c.take(1000, 3, 0);
        assert!(again.procs.is_empty() && again.files.is_empty());
    }

    #[test]
    fn missing_header_pid_falls_back_to_the_thread_map() {
        let mut c = Collector::new(devices(), &[]);
        let th = B::default().d(42).d(77).0;
        c.event(&hdr(Provider::Thread, 1, 3, u32::MAX), &th, 0);
        let (h, d) = name(0x20, r"\Device\HarddiskVolume3\f");
        c.event(&h, &d, 0);
        let (mut h, d) = rw_v3(true, 0x10, 0x20, 10);
        h.pid = u32::MAX;
        c.event(&h, &d, 0);
        let t = c.take(1000, 0, 0);
        assert_eq!(t.procs[0].pid, 42);
    }

    #[test]
    fn watched_folder_gets_create_write_delete_rename() {
        let mut c = Collector::new(devices(), &[r"C:\work".into()]);
        assert!(c.watching());
        let create = |path: &str, disposition: u32| {
            B::default()
                .p(1)
                .p(0x10)
                .d(77)
                .d(disposition << 24)
                .d(0)
                .d(0)
                .s(path)
                .0
        };
        let h = hdr(Provider::FileIo, 64, 3, 1234);
        c.event(&h, &create(r"\Device\HarddiskVolume3\work\new.txt", 2), 1);
        c.event(&h, &create(r"\Device\HarddiskVolume3\work\open.txt", 1), 2); // plain open
        c.event(&h, &create(r"\Device\HarddiskVolume3\workshop\x.txt", 2), 3); // not under
        let (h, d) = name(0x20, r"\Device\HarddiskVolume3\work\new.txt");
        c.event(&h, &d, 4);
        let (h, d) = rw_v3(true, 0x10, 0x20, 10);
        c.event(&h, &d, 5);
        let ren = B::default()
            .p(1)
            .p(0x10)
            .p(0x20)
            .p(0)
            .d(77)
            .d(10)
            .s(r"\Device\HarddiskVolume3\work\renamed.txt")
            .0;
        c.event(&hdr(Provider::FileIo, 80, 3, 1234), &ren, 6);
        let del = B::default()
            .p(1)
            .p(0x10)
            .p(0x20)
            .p(0)
            .d(77)
            .d(13)
            .s(r"\Device\HarddiskVolume3\work\renamed.txt")
            .0;
        c.event(&hdr(Provider::FileIo, 79, 3, 1234), &del, 7);
        let t = c.take(1000, 0, 8);
        let ops: Vec<(TouchOp, &str)> = t.touches.iter().map(|x| (x.op, x.path.as_str())).collect();
        assert_eq!(
            ops,
            vec![
                (TouchOp::Create, r"C:\work\new.txt"),
                (TouchOp::Rename, r"C:\work\new.txt"),
                (TouchOp::Delete, r"C:\work\renamed.txt"),
                (TouchOp::Write, r"C:\work\new.txt"),
            ]
        );
        assert_eq!(t.touches[1].target.as_deref(), Some(r"C:\work\renamed.txt"));
    }

    #[test]
    fn paging_io_is_not_counted_twice() {
        let mut c = Collector::new(devices(), &[]);
        let (h, d) = name(0x20, r"\Device\HarddiskVolume3\f.bin");
        c.event(&h, &d, 0);
        let (h, d) = rw_v3(true, 0x10, 0x20, 4096);
        c.event(&h, &d, 0);
        // the flush of the same bytes by the cache manager (IRP_PAGING_IO | NOCACHE)
        let (h, mut d) = rw_v3(true, 0x10, 0x20, 4096);
        let n = d.len();
        d[n - 4..].copy_from_slice(&0x43u32.to_le_bytes());
        c.event(&h, &d, 0);
        let t = c.take(1000, 0, 0);
        assert_eq!(t.events, 2);
        assert_eq!(t.procs[0].counts.write, 4096);
        assert_eq!(t.procs[0].counts.write_ops, 1);
    }

    #[test]
    fn a_folder_opened_by_its_short_name_is_still_watched() {
        let mut c = Collector::new(
            devices(),
            &[r"C:\Users\runneradmin".into(), r"C:\Users\RUNNER~1".into()],
        );
        let create = B::default()
            .p(1)
            .p(0x10)
            .d(77)
            .d(5 << 24) // OVERWRITE_IF: CreateFile(CREATE_ALWAYS)
            .d(0)
            .d(0)
            .s(r"\Device\HarddiskVolume3\Users\RUNNER~1\AppData\x.bin")
            .0;
        c.event(&hdr(Provider::FileIo, 64, 3, 9), &create, 1);
        let t = c.take(1000, 0, 2);
        assert_eq!(t.touches.len(), 1);
        assert_eq!(t.touches[0].op, TouchOp::Create);
        assert_eq!(t.touches[0].path, r"C:\Users\RUNNER~1\AppData\x.bin");
    }

    #[test]
    fn double_logged_renames_and_deletes_are_reported_once() {
        let t = |op, path: &str, target: Option<&str>| Touch {
            at: 1,
            pid: 5,
            op,
            path: path.into(),
            target: target.map(Into::into),
        };
        let v = dedup_touches(vec![
            t(TouchOp::Rename, r"C:\w\a", None),
            t(TouchOp::Rename, r"C:\w\a", Some(r"C:\w\b")),
            t(TouchOp::Delete, r"C:\w\b", None),
            t(TouchOp::Delete, r"C:\W\B", None),
            t(TouchOp::Rename, r"C:\w\c", None),
        ]);
        let ops: Vec<(TouchOp, &str)> = v.iter().map(|x| (x.op, x.path.as_str())).collect();
        assert_eq!(
            ops,
            vec![
                (TouchOp::Rename, r"C:\w\a"),
                (TouchOp::Delete, r"C:\w\b"),
                (TouchOp::Rename, r"C:\w\c"),
            ]
        );
        assert!(v[0].target.is_some());
    }

    proptest::proptest! {
        #[test]
        fn random_payloads_never_panic(
            opcode in 0u8..=90,
            version in 0u8..=5,
            data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..96),
            thread in proptest::prelude::any::<bool>(),
        ) {
            let provider = if thread { Provider::Thread } else { Provider::FileIo };
            let mut c = Collector::new(devices(), &[r"C:\w".into()]);
            c.event(&hdr(provider, opcode, version, 1), &data, 0);
            let _ = c.take(1000, 0, 0);
        }
    }
}
