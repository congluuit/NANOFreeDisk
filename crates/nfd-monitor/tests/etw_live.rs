//! The real kernel trace session. It needs an elevated process: GitHub's Windows
//! runners are, a normal developer shell is not — there the test only checks that
//! the session is refused cleanly.
#![cfg(windows)]

use std::io::Write;
use std::time::{Duration, Instant};

use nfd_monitor::TouchOp;
use nfd_monitor::win::etw::{EtwError, EtwSession, SESSION, session_exists};
use nfd_monitor::win::is_elevated;

#[test]
fn etw_attributes_file_writes_to_this_process() {
    if !is_elevated() {
        assert_eq!(EtwSession::start(None).err(), Some(EtwError::AccessDenied));
        assert!(!session_exists(SESSION));
        eprintln!("skipped: kernel tracing needs an elevated process");
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let name = format!("nfd-etw-{}.bin", std::process::id());
    let path = dir.path().join(&name);
    let renamed = dir.path().join(format!("{name}.old"));
    let mut session =
        EtwSession::start(Some(&dir.path().to_string_lossy())).expect("start ETW session");
    assert!(session_exists(SESSION));
    std::thread::sleep(Duration::from_millis(500));
    let _ = session.take();

    let chunk = vec![0x5au8; 64 * 1024];
    {
        let mut f = std::fs::File::create(&path).expect("create");
        for _ in 0..64 {
            f.write_all(&chunk).expect("write");
        }
        f.sync_all().expect("flush");
    }
    std::fs::rename(&path, &renamed).expect("rename");
    std::fs::remove_file(&renamed).expect("delete");

    let me = std::process::id();
    let (mut written, mut file_seen) = (0u64, false);
    let mut touches = Vec::new();
    let mut events = 0;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline && !(written >= 4 << 20 && file_seen && touches.len() >= 3) {
        std::thread::sleep(Duration::from_millis(1000));
        let t = session.take();
        events += t.events;
        written += t
            .procs
            .iter()
            .filter(|p| p.pid == me)
            .map(|p| p.counts.write)
            .sum::<u64>();
        file_seen |= t.files.iter().any(|f| {
            f.pid == me
                && f.path
                    .to_ascii_lowercase()
                    .ends_with(&name.to_ascii_lowercase())
        });
        touches.extend(t.touches.into_iter().filter(|x| x.pid == me));
        assert_eq!(t.undecoded, 0, "every event layout is known");
    }
    eprintln!("events {events}, written {written}, touches {touches:?}");
    assert!(
        written >= 4 << 20,
        "saw {written} bytes written by this process"
    );
    assert!(
        written < 6 << 20,
        "{written} bytes: the flush (paging I/O) must not count the data twice"
    );
    assert!(file_seen, "the written file is listed with its path");
    let ops: Vec<TouchOp> = touches.iter().map(|t| t.op).collect();
    for op in [
        TouchOp::Create,
        TouchOp::Write,
        TouchOp::Rename,
        TouchOp::Delete,
    ] {
        assert!(ops.contains(&op), "{op:?} missing from {ops:?}");
    }

    session.stop();
    assert!(!session_exists(SESSION), "the session is gone after stop");
}
