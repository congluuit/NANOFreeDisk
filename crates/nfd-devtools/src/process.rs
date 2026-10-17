//! Running developer tools (`git`, `vswhere`, package-manager cleanup commands):
//! never through a shell string, stdin closed, no console window, output captured,
//! a hard timeout and an optional cancel flag.

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;

/// Most bytes of stdout / stderr kept (the tail).
const KEEP: usize = 16 * 1024;

/// What a finished (or stopped) command printed.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutput {
    /// Exit code; `None` when killed.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
    pub millis: u64,
    /// stdout was longer than the bytes kept (only its tail is in `stdout`).
    #[serde(skip)]
    pub truncated: bool,
}

impl RunOutput {
    #[must_use]
    pub fn success(&self) -> bool {
        self.code == Some(0) && !self.timed_out && !self.cancelled
    }
}

/// Extensions tried for a bare program name (Windows `PATHEXT` order).
fn extensions() -> Vec<String> {
    if cfg!(windows) {
        vec![".exe".into(), ".cmd".into(), ".bat".into(), ".com".into()]
    } else {
        vec![String::new()]
    }
}

/// Find `name` on `PATH` (absolute path), like `where` / `which`.
#[must_use]
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(name, std::env::var_os("PATH")?)
}

/// [`which`] over an explicit `PATH` value (tests).
#[must_use]
pub fn which_in(name: &str, path: OsString) -> Option<PathBuf> {
    let has_ext = Path::new(name).extension().is_some();
    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() || !dir.is_absolute() {
            continue;
        }
        if has_ext {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
            continue;
        }
        for ext in extensions() {
            let p = dir.join(format!("{name}{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn tail(mut bytes: Vec<u8>, keep: usize) -> (String, bool) {
    let cut = bytes.len() > keep;
    if cut {
        bytes.drain(..bytes.len() - keep);
    }
    (String::from_utf8_lossy(&bytes).into_owned(), cut)
}

fn reader(mut pipe: impl Read + Send + 'static) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    rx
}

/// Output of a reader; a grandchild that still holds the pipe open is not waited for
/// longer than `wait`.
fn collect(rx: Option<mpsc::Receiver<Vec<u8>>>, wait: Duration, keep: usize) -> (String, bool) {
    rx.and_then(|r| r.recv_timeout(wait).ok())
        .map(|b| tail(b, keep))
        .unwrap_or_default()
}

/// Stop `child` and everything it started (`npm.cmd` → `node.exe`).
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("SystemRoot") {
            let taskkill = PathBuf::from(root).join("System32").join("taskkill.exe");
            let mut cmd = Command::new(taskkill);
            cmd.args(["/T", "/F", "/PID", &child.id().to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
            let _ = cmd.status();
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn spawn(program: &Path, args: &[String], cwd: Option<&Path>) -> io::Result<Child> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn()
}

/// Run `program args…` and wait at most `timeout`; `cancel` stops it early. The last
/// 16 KB of stdout / stderr are kept.
///
/// # Errors
/// The program could not be started.
pub fn run(
    program: &Path,
    args: &[String],
    cwd: Option<&Path>,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> io::Result<RunOutput> {
    run_keeping(program, args, cwd, timeout, cancel, KEEP)
}

/// [`run`] keeping up to `keep` bytes of stdout (listings that must be complete:
/// [`RunOutput::truncated`] says when they were not).
///
/// # Errors
/// The program could not be started.
pub fn run_keeping(
    program: &Path,
    args: &[String],
    cwd: Option<&Path>,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    keep: usize,
) -> io::Result<RunOutput> {
    let start = Instant::now();
    let mut child = spawn(program, args, cwd)?;
    let out = child.stdout.take().map(reader);
    let err = child.stderr.take().map(reader);
    let mut result = RunOutput::default();
    loop {
        if let Some(status) = child.try_wait()? {
            result.code = status.code();
            break;
        }
        if start.elapsed() >= timeout {
            result.timed_out = true;
        } else if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            result.cancelled = true;
        }
        if result.timed_out || result.cancelled {
            kill_tree(&mut child);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let wait = if result.code.is_some() {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(2)
    };
    (result.stdout, result.truncated) = collect(out, wait, keep.max(KEEP));
    result.stderr = collect(err, wait, KEEP).0;
    result.millis = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(result)
}

/// `git` on `PATH`, if any.
#[must_use]
pub fn git() -> Option<PathBuf> {
    which("git")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_programs_on_an_explicit_path() {
        let dir = tempfile::tempdir().expect("tmp");
        let name = if cfg!(windows) { "tool.cmd" } else { "tool" };
        std::fs::write(dir.path().join(name), "x").expect("w");
        let path = std::env::join_paths([dir.path()]).expect("join");
        assert_eq!(which_in("tool", path.clone()), Some(dir.path().join(name)));
        assert_eq!(which_in("missing", path), None);
    }

    #[cfg(windows)]
    #[test]
    fn runs_without_a_shell_and_times_out() {
        let cmd = which("cmd").expect("cmd.exe");
        let ok = run(
            &cmd,
            &["/C".into(), "echo".into(), "hi".into()],
            None,
            Duration::from_secs(20),
            None,
        )
        .expect("run");
        assert!(ok.success(), "{ok:?}");
        assert!(ok.stdout.contains("hi"));
        let slow = run(
            &cmd,
            &[
                "/C".into(),
                "ping".into(),
                "-n".into(),
                "30".into(),
                "127.0.0.1".into(),
            ],
            None,
            Duration::from_millis(300),
            None,
        )
        .expect("run");
        assert!(slow.timed_out);
        assert!(!slow.success());
    }
}
