//! Process list for Hardware → Tasks and `nfd ps` (`docs/HARDWARE_MONITOR.md` §2, §5).
//!
//! One `NtQuerySystemInformation(SystemProcessInformation)` call per refresh gives
//! every process with its CPU time, private working set and I/O counters; the
//! [`TaskMonitor`] turns two refreshes into rates. GPU use per process comes from
//! the PDH `GPU Engine` counters, publishers from Authenticode (cached per path).
//!
//! Ending a process is only ever the user's action: [`end_task`] re-identifies the
//! process by PID **and** creation time, re-runs [`safety::classify`] on a live
//! handle and refuses hard-blocked processes outright.

pub mod safety;
#[cfg(windows)]
mod win;

use serde::Serialize;

pub use safety::{BlockReason, Facts, Verdict, WarnReason};
#[cfg(windows)]
pub use win::{TaskDetails, TaskMonitor, details, end_task, publisher_label, verdict};

/// One process as the kernel reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawProcess {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    /// FILETIME (100 ns since 1601). With the PID it identifies the process.
    pub create_time: u64,
    /// Kernel + user time, 100 ns.
    pub cpu_time: u64,
    pub private_ws: u64,
    pub working_set: u64,
    pub threads: u32,
    pub handles: u32,
    pub session: u32,
    pub base_priority: i32,
    pub io_read: u64,
    pub io_write: u64,
}

fn rd<const N: usize>(b: &[u8], o: usize) -> [u8; N] {
    b.get(o..o + N)
        .and_then(|x| x.try_into().ok())
        .unwrap_or([0; N])
}

/// Parse a `SystemProcessInformation` buffer (64-bit layout). `name_at` reads the
/// UTF-16 image name: the buffer holds a pointer, so tests pass a fake reader.
pub fn parse_process_info(buf: &[u8], name_at: &dyn Fn(usize, usize) -> String) -> Vec<RawProcess> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    loop {
        if pos + 256 > buf.len() {
            break;
        }
        let e = &buf[pos..];
        let next = u32::from_le_bytes(rd(e, 0)) as usize;
        let name_len = u16::from_le_bytes(rd(e, 56)) as usize;
        let name_ptr = u64::from_le_bytes(rd(e, 64)) as usize;
        let pid = u64::from_le_bytes(rd(e, 80)) as u32;
        let name = if name_ptr == 0 || name_len == 0 {
            if pid == 0 {
                "System Idle Process".to_owned()
            } else {
                String::new()
            }
        } else {
            name_at(name_ptr, name_len / 2)
        };
        out.push(RawProcess {
            pid,
            ppid: u64::from_le_bytes(rd(e, 88)) as u32,
            name,
            create_time: u64::from_le_bytes(rd(e, 32)),
            cpu_time: u64::from_le_bytes(rd(e, 40)).wrapping_add(u64::from_le_bytes(rd(e, 48))),
            private_ws: u64::from_le_bytes(rd(e, 8)),
            working_set: u64::from_le_bytes(rd(e, 144)),
            threads: u32::from_le_bytes(rd(e, 4)),
            handles: u32::from_le_bytes(rd(e, 96)),
            session: u32::from_le_bytes(rd(e, 100)),
            base_priority: i32::from_le_bytes(rd(e, 72)),
            io_read: u64::from_le_bytes(rd(e, 232)),
            io_write: u64::from_le_bytes(rd(e, 240)),
        });
        if next == 0 {
            break;
        }
        pos += next;
    }
    out
}

/// Task Manager-style grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Has a visible top-level window.
    App,
    Background,
    /// Kernel, Windows folder, session-0 system processes.
    Windows,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Task {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    /// FILETIME as a decimal string (exceeds JavaScript's safe integers).
    pub create_time: String,
    pub path: Option<String>,
    /// Main window title of an app.
    pub title: Option<String>,
    pub group: Group,
    /// % of all logical processors.
    pub cpu: f64,
    /// Private working set, bytes (Task Manager's "Memory").
    pub memory: u64,
    pub io_read: f64,
    pub io_write: f64,
    /// % of the busiest GPU engine.
    pub gpu: f64,
    pub threads: u32,
    pub handles: u32,
    pub session: u32,
    pub priority: i32,
    pub publisher: Option<String>,
    /// Cheap pre-check so the UI can hide End task for hard-blocked processes; the
    /// authoritative check runs again in [`end_task`].
    pub blocked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Summary {
    pub cpu: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    /// Physical disk bytes / s (all disks).
    pub disk: f64,
    pub gpu: f64,
    pub processes: usize,
    pub threads: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskSnapshot {
    pub tasks: Vec<Task>,
    pub summary: Summary,
    pub elapsed_ms: f64,
}

/// How to end a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EndMode {
    /// Ask the app to close (`WM_CLOSE` to its windows); it may save or refuse.
    Close,
    /// `TerminateProcess`.
    Kill,
    /// The process and everything it started, children first.
    KillTree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "status", content = "detail")]
pub enum Outcome {
    Ended,
    /// `WM_CLOSE` sent; the UI checks again after a few seconds.
    CloseRequested,
    /// No window to close: only Kill can end it.
    NoWindow,
    Blocked(BlockReason),
    /// Needs the second confirmation for these reasons.
    NeedsConfirm(Vec<WarnReason>),
    /// PID gone or reused by another process since the list was shown.
    Changed,
    /// Belongs to another user or runs elevated (needs the admin helper).
    AccessDenied,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EndResult {
    pub pid: u32,
    pub name: String,
    pub outcome: Outcome,
}

/// PIDs started (directly or not) by `root`, deepest first. A child must have been
/// created after its parent — a PID reused by an older process is not a child.
pub fn descendants(procs: &[RawProcess], root: u32, root_created: u64) -> Vec<(u32, u64)> {
    let mut out = Vec::new();
    let mut frontier = vec![(root, root_created)];
    while let Some((pid, created)) = frontier.pop() {
        for p in procs {
            if p.ppid == pid
                && p.pid != pid
                && p.create_time >= created
                && !out.iter().any(|(x, _)| *x == p.pid)
            {
                out.push((p.pid, p.create_time));
                frontier.push((p.pid, p.create_time));
            }
        }
    }
    out.reverse();
    out
}

/// Helper processes of an app (Chromium renderers, Electron GPU process…) share its
/// image name and descend from it: group them with the app, like Task Manager does.
pub fn group_app_children(tasks: &mut [Task]) {
    let index: std::collections::HashMap<u32, usize> =
        tasks.iter().enumerate().map(|(i, t)| (t.pid, i)).collect();
    let mut promote = Vec::new();
    for (i, t) in tasks.iter().enumerate() {
        if t.group != Group::Background {
            continue;
        }
        let mut cur = t.ppid;
        for _ in 0..16 {
            let Some(&j) = index.get(&cur) else { break };
            let parent = &tasks[j];
            if parent.pid == cur
                && parent.create_time.parse::<u64>().ok() > t.create_time.parse::<u64>().ok()
            {
                break; // reused PID: not an ancestor
            }
            if parent.group == Group::App && parent.name.eq_ignore_ascii_case(&t.name) {
                promote.push(i);
                break;
            }
            if parent.ppid == cur {
                break;
            }
            cur = parent.ppid;
        }
    }
    for i in promote {
        tasks[i].group = Group::App;
    }
}

/// CPU % between two samples: process time over wall time × logical CPUs.
pub fn cpu_percent(delta_time: u64, elapsed_100ns: u64, cpus: u32) -> f64 {
    if elapsed_100ns == 0 || cpus == 0 {
        return 0.0;
    }
    (delta_time as f64 * 100.0 / (elapsed_100ns as f64 * f64::from(cpus))).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pid: u64, ppid: u64, next: u32, name_ptr: u64, name_len: u16) -> Vec<u8> {
        let mut e = vec![0u8; 256];
        e[0..4].copy_from_slice(&next.to_le_bytes());
        e[4..8].copy_from_slice(&7u32.to_le_bytes());
        e[8..16].copy_from_slice(&(10u64 << 20).to_le_bytes());
        e[32..40].copy_from_slice(&(133_000_000_000_000_000u64 + pid).to_le_bytes());
        e[40..48].copy_from_slice(&1_000u64.to_le_bytes());
        e[48..56].copy_from_slice(&500u64.to_le_bytes());
        e[56..58].copy_from_slice(&name_len.to_le_bytes());
        e[64..72].copy_from_slice(&name_ptr.to_le_bytes());
        e[72..76].copy_from_slice(&8i32.to_le_bytes());
        e[80..88].copy_from_slice(&pid.to_le_bytes());
        e[88..96].copy_from_slice(&ppid.to_le_bytes());
        e[96..100].copy_from_slice(&42u32.to_le_bytes());
        e[100..104].copy_from_slice(&1u32.to_le_bytes());
        e[232..240].copy_from_slice(&4096u64.to_le_bytes());
        e[240..248].copy_from_slice(&8192u64.to_le_bytes());
        e
    }

    #[test]
    fn parses_the_kernel_layout() {
        let mut buf = entry(0, 0, 256, 0, 0);
        buf.extend(entry(1234, 4, 0, 0xABCD, 22));
        let procs = parse_process_info(&buf, &|ptr, chars| format!("p{ptr:x}:{chars}"));
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[0].name, "System Idle Process");
        let p = &procs[1];
        assert_eq!((p.pid, p.ppid, p.name.as_str()), (1234, 4, "pabcd:11"));
        assert_eq!(p.cpu_time, 1500);
        assert_eq!(p.private_ws, 10 << 20);
        assert_eq!(
            (p.threads, p.handles, p.session, p.base_priority),
            (7, 42, 1, 8)
        );
        assert_eq!((p.io_read, p.io_write), (4096, 8192));
        assert_eq!(p.create_time, 133_000_000_000_001_234);
        assert!(parse_process_info(&buf[..100], &|_, _| String::new()).is_empty());
    }

    #[test]
    fn tree_is_children_first_and_ignores_reused_pids() {
        let p = |pid, ppid, t| RawProcess {
            pid,
            ppid,
            create_time: t,
            ..RawProcess::default()
        };
        let procs = vec![
            p(10, 1, 100),
            p(11, 10, 110),
            p(12, 11, 120),
            p(13, 10, 50),
            p(14, 99, 130),
        ];
        let d = descendants(&procs, 10, 100);
        let pids: Vec<u32> = d.iter().map(|x| x.0).collect();
        assert_eq!(
            pids,
            vec![12, 11],
            "13 is older than 10: a reused parent PID"
        );
    }

    #[test]
    fn app_helpers_join_their_app() {
        let t = |pid, ppid, name: &str, group, ct: u64| Task {
            pid,
            ppid,
            name: name.into(),
            create_time: ct.to_string(),
            path: None,
            title: None,
            group,
            cpu: 0.0,
            memory: 0,
            io_read: 0.0,
            io_write: 0.0,
            gpu: 0.0,
            threads: 1,
            handles: 1,
            session: 1,
            priority: 8,
            publisher: None,
            blocked: false,
        };
        let mut tasks = vec![
            t(10, 1, "brave.exe", Group::App, 100),
            t(11, 10, "brave.exe", Group::Background, 110),
            t(12, 11, "brave.exe", Group::Background, 120),
            t(13, 10, "helper.exe", Group::Background, 130),
            t(14, 10, "brave.exe", Group::Background, 50),
        ];
        group_app_children(&mut tasks);
        let groups: Vec<Group> = tasks.iter().map(|x| x.group).collect();
        assert_eq!(
            groups,
            vec![
                Group::App,
                Group::App,
                Group::App,
                Group::Background,
                Group::Background
            ]
        );
    }

    #[test]
    fn cpu_percent_is_normalized_by_cpus() {
        assert_eq!(cpu_percent(10_000_000, 10_000_000, 4), 25.0);
        assert_eq!(cpu_percent(1, 0, 4), 0.0);
        assert_eq!(cpu_percent(u64::MAX, 1, 1), 100.0);
    }
}
