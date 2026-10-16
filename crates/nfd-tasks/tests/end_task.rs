//! End task against processes this test starts itself (never anything else), plus
//! the hard blocks on real system processes — which are only *checked*, never ended.
#![cfg(windows)]

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nfd_tasks::{
    BlockReason, EndMode, Outcome, TaskMonitor, Verdict, WarnReason, end_task, verdict,
};

fn spawn(program: &str, args: &[&str]) -> Child {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn test child")
}

fn create_time(mon: &mut TaskMonitor, pid: u32) -> String {
    for _ in 0..20 {
        if let Some(t) = mon.snapshot().tasks.iter().find(|t| t.pid == pid) {
            return t.create_time.clone();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("pid {pid} not in the snapshot");
}

fn alive(child: &mut Child) -> bool {
    child.try_wait().expect("try_wait").is_none()
}

#[test]
fn snapshot_lists_processes_with_rates() {
    let mut mon = TaskMonitor::new();
    mon.snapshot();
    std::thread::sleep(Duration::from_millis(300));
    let snap = mon.snapshot();
    assert!(snap.tasks.len() > 20, "{}", snap.tasks.len());
    let me = snap
        .tasks
        .iter()
        .find(|t| t.pid == std::process::id())
        .expect("own process");
    assert!(me.memory > 0 && me.threads > 0);
    assert!(me.blocked, "own process is never offered End task");
    assert!(snap.tasks.iter().any(|t| t.pid == 4 && t.blocked));
    assert!(snap.summary.memory_total > snap.summary.memory_used);
    assert!((0.0..=100.0).contains(&snap.summary.cpu));
    assert!(snap.tasks.iter().all(|t| (0.0..=100.0).contains(&t.cpu)));
}

#[test]
fn wrong_creation_time_never_ends_anything() {
    let mut child = spawn("ping", &["-n", "60", "127.0.0.1"]);
    let pid = child.id();
    let r = end_task(pid, "116444736000000000", EndMode::Kill, true);
    assert_eq!(r[0].outcome, Outcome::Changed);
    assert!(alive(&mut child), "a stale PID must not be killed");
    let r = end_task(pid, "not-a-number", EndMode::Kill, true);
    assert_eq!(r[0].outcome, Outcome::Changed);
    assert!(alive(&mut child));
    child.kill().ok();
    child.wait().ok();
}

#[test]
fn windows_component_needs_the_second_confirmation() {
    let mut mon = TaskMonitor::new();
    let mut child = spawn("ping", &["-n", "60", "127.0.0.1"]);
    let pid = child.id();
    let ct = create_time(&mut mon, pid);
    assert_eq!(
        verdict(pid, &ct),
        Ok(Verdict::Warn(vec![WarnReason::WindowsComponent]))
    );
    let r = end_task(pid, &ct, EndMode::Kill, false);
    assert_eq!(
        r[0].outcome,
        Outcome::NeedsConfirm(vec![WarnReason::WindowsComponent])
    );
    assert!(alive(&mut child));
    // ping has no window: Close cannot end it
    assert_eq!(
        end_task(pid, &ct, EndMode::Close, true)[0].outcome,
        Outcome::NoWindow
    );
    let r = end_task(pid, &ct, EndMode::Kill, true);
    assert_eq!(r[0].outcome, Outcome::Ended);
    assert!(child.wait().expect("wait").code().is_some());
    // already exited while our handle keeps it around: nothing to do
    assert_eq!(
        end_task(pid, &ct, EndMode::Kill, true)[0].outcome,
        Outcome::Ended
    );
    drop(child);
    assert_eq!(
        end_task(pid, &ct, EndMode::Kill, true)[0].outcome,
        Outcome::Changed
    );
}

#[test]
fn process_tree_ends_children_first() {
    let mut mon = TaskMonitor::new();
    let mut parent = spawn("cmd", &["/c", "ping -n 60 127.0.0.1 >nul"]);
    let pid = parent.id();
    let ct = create_time(&mut mon, pid);
    // wait for cmd to start ping
    let mut kid = None;
    for _ in 0..40 {
        kid = mon
            .snapshot()
            .tasks
            .iter()
            .find(|t| t.ppid == pid)
            .map(|t| t.pid);
        if kid.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let kid = kid.expect("child of cmd");
    let r = end_task(pid, &ct, EndMode::KillTree, true);
    let order: Vec<u32> = r.iter().map(|x| x.pid).collect();
    assert_eq!(order.last(), Some(&pid), "parent last: {r:?}");
    assert!(order.contains(&kid), "{r:?}");
    assert!(r.iter().all(|x| x.outcome == Outcome::Ended), "{r:?}");
    parent.wait().expect("wait");
}

#[test]
fn system_processes_and_self_are_blocked() {
    let mut mon = TaskMonitor::new();
    let snap = mon.snapshot();
    for name in [
        "csrss.exe",
        "smss.exe",
        "wininit.exe",
        "services.exe",
        "lsass.exe",
        "winlogon.exe",
        "dwm.exe",
    ] {
        for t in snap
            .tasks
            .iter()
            .filter(|t| t.name.eq_ignore_ascii_case(name))
        {
            assert!(t.blocked, "{name}");
            assert!(
                matches!(verdict(t.pid, &t.create_time), Ok(Verdict::Blocked(_))),
                "{name} verdict"
            );
            // checked only: the outcome must be Blocked before anything is opened
            let r = end_task(t.pid, &t.create_time, EndMode::Kill, true);
            assert_eq!(
                r[0].outcome,
                Outcome::Blocked(BlockReason::SystemCore),
                "{name}"
            );
        }
    }
    let me = std::process::id();
    let ct = create_time(&mut mon, me);
    assert_eq!(verdict(me, &ct), Ok(Verdict::Blocked(BlockReason::Own)));
    assert_eq!(
        end_task(me, &ct, EndMode::Kill, true)[0].outcome,
        Outcome::Blocked(BlockReason::Own)
    );
    assert_eq!(
        end_task(4, "0", EndMode::Kill, true)[0].outcome,
        Outcome::Blocked(BlockReason::Kernel)
    );
}
