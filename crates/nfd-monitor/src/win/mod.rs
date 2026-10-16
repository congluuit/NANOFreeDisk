//! Windows sampling: the process list (`nfd_tasks::processes`, one kernel call),
//! physical disk counters (PDH) and the engine that produces a [`Tick`].

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use nfd_sensors::win::pdh::{Counter, Query};

use crate::rates::{CounterRates, Cumulative};
use crate::{Counts, DiskStat, ProcInfo, Source, Tick};

/// Physical disk counters (`\PhysicalDisk(*)\…`, English names).
#[derive(Debug)]
pub struct Disks {
    q: Option<(Query, [Option<Counter>; 6])>,
}

// SAFETY: PDH handles are process-wide; a sampler is used behind a Mutex.
unsafe impl Send for Disks {}

impl Default for Disks {
    fn default() -> Self {
        Self::new()
    }
}

impl Disks {
    pub fn new() -> Self {
        let q = Query::new().map(|mut q| {
            let c = [
                q.add(r"\PhysicalDisk(*)\Disk Read Bytes/sec"),
                q.add(r"\PhysicalDisk(*)\Disk Write Bytes/sec"),
                q.add(r"\PhysicalDisk(*)\Disk Transfers/sec"),
                q.add(r"\PhysicalDisk(*)\Avg. Disk Queue Length"),
                q.add(r"\PhysicalDisk(*)\Avg. Disk sec/Transfer"),
                q.add(r"\PhysicalDisk(*)\% Idle Time"),
            ];
            q.collect();
            (q, c)
        });
        Self { q }
    }

    /// One row per physical disk (no `_Total`), by instance name.
    pub fn sample(&mut self) -> Vec<DiskStat> {
        let Some((q, c)) = &mut self.q else {
            return Vec::new();
        };
        if !q.collect() {
            return Vec::new();
        }
        let mut rows: HashMap<String, DiskStat> = HashMap::new();
        for (i, counter) in c.iter().enumerate() {
            let Some(counter) = counter else { continue };
            for (inst, v) in q.values(*counter) {
                if inst.eq_ignore_ascii_case("_Total") {
                    continue;
                }
                let r = rows.entry(inst.clone()).or_insert_with(|| DiskStat {
                    name: inst,
                    read: 0.0,
                    write: 0.0,
                    transfers: 0.0,
                    queue: 0.0,
                    latency_ms: 0.0,
                    idle: 100.0,
                });
                let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
                match i {
                    0 => r.read = v,
                    1 => r.write = v,
                    2 => r.transfers = v,
                    3 => r.queue = v,
                    4 => r.latency_ms = v * 1000.0,
                    _ => r.idle = v.min(100.0),
                }
            }
        }
        let mut v: Vec<DiskStat> = rows.into_values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

/// Live processes as the monitor needs them, plus their cumulative I/O counters.
pub fn process_list() -> (Vec<ProcInfo>, Vec<Cumulative>) {
    let raw = nfd_tasks::processes();
    let mut infos = Vec::with_capacity(raw.len());
    let mut cums = Vec::with_capacity(raw.len());
    for p in raw {
        cums.push(Cumulative {
            pid: p.pid,
            create_time: p.create_time,
            counts: Counts {
                read: p.io_read,
                write: p.io_write,
                read_ops: p.io_read_ops,
                write_ops: p.io_write_ops,
            },
        });
        infos.push(ProcInfo {
            pid: p.pid,
            create_time: p.create_time,
            name: p.name,
            path: None,
        });
    }
    (infos, cums)
}

/// Image paths, looked up only for processes that did I/O and cached per process.
#[derive(Debug, Default)]
pub struct Paths {
    cache: HashMap<(u32, u64), Option<String>>,
}

impl Paths {
    pub fn fill(&mut self, infos: &mut [ProcInfo], active: &HashSet<u32>) {
        let live: HashSet<(u32, u64)> = infos.iter().map(|p| (p.pid, p.create_time)).collect();
        self.cache.retain(|k, _| live.contains(k));
        for p in infos.iter_mut() {
            let key = (p.pid, p.create_time);
            if let Some(known) = self.cache.get(&key) {
                p.path.clone_from(known);
            } else if active.contains(&p.pid) {
                let path = nfd_tasks::image_path_of(p.pid);
                self.cache.insert(key, path.clone());
                p.path = path;
            }
        }
    }
}

/// Is this process elevated (ETW needs it)?
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: pseudo-handle of this process; token closed below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let mut e = TOKEN_ELEVATION::default();
    let mut len = 0u32;
    // SAFETY: buffer is a TOKEN_ELEVATION.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&raw mut e).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    } != 0;
    // SAFETY: opened above.
    unsafe { CloseHandle(token) };
    ok && e.TokenIsElevated != 0
}

/// Produces one interval's [`Tick`] with the process list and disk counters.
#[derive(Debug)]
pub struct Sampler {
    counters: CounterRates,
    disks: Disks,
    paths: Paths,
    last: Option<Instant>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

/// One sample: activity, the processes to name it, the disks.
#[derive(Debug)]
pub struct Sample {
    pub source: Source,
    pub tick: Tick,
    pub procs: Vec<ProcInfo>,
    pub disks: Vec<DiskStat>,
}

impl Sampler {
    /// The Counters engine (no admin needed).
    pub fn new() -> Self {
        let mut s = Self {
            counters: CounterRates::default(),
            disks: Disks::new(),
            paths: Paths::default(),
            last: None,
        };
        // baseline, so the first real sample has rates
        let (_, cums) = process_list();
        s.counters.tick(&cums, 0);
        s.last = Some(Instant::now());
        s
    }

    /// Activity since the previous call. `external` replaces the counter deltas
    /// with a file-level tick (ETW in this process or in the helper).
    pub fn sample(&mut self, external: Option<(Source, Tick)>) -> Sample {
        let now = Instant::now();
        let elapsed = self
            .last
            .map_or(0, |t| now.duration_since(t).as_millis() as u64);
        self.last = Some(now);
        let (mut procs, cums) = process_list();
        let counter_tick = self.counters.tick(&cums, elapsed);
        let (source, tick) = external.unwrap_or((Source::Counters, counter_tick));
        let active: HashSet<u32> = tick.procs.iter().map(|p| p.pid).collect();
        self.paths.fill(&mut procs, &active);
        Sample {
            source,
            tick,
            procs,
            disks: self.disks.sample(),
        }
    }
}
