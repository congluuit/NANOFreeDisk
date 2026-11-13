//! `nanofreedisk.exe --elevated-clean <request.json> <response.json>`: the one
//! elevated step of a Clean (`docs/PHASE3_CLEAN.md` §8), modeled on the
//! uninstaller's `--elevated-leftovers`.
//!
//! * [`run`] is the elevated side (no window): it turns on Redirection Guard first,
//!   checks it really is elevated, reads the request (size-limited, never through a
//!   link), loads the rule packs **built into this binary** with machine folders
//!   resolved from Windows instead of the environment, and hands everything to
//!   [`nfd_rules::elevated::execute`], which treats the request as untrusted input.
//!   It writes the response next to the request (a new file, never through a link)
//!   and exits.
//! * [`run_elevated`] is the app's side: it writes the request into its own data
//!   folder, starts this executable once through UAC (`runas`) and waits for the
//!   response. The app itself never runs elevated.

use std::io::Read;
use std::path::{Path, PathBuf};

use nfd_core::exec;
use nfd_core::safety::LiveProbe;
use nfd_rules::elevated::{self, Context, PROTOCOL, Request, Response};
use nfd_rules::matcher::{MatchOptions, Matcher};

pub use nfd_rules::elevated::FLAG;

fn refused(why: &str) -> Response {
    Response {
        protocol: PROTOCOL,
        error: Some(why.to_owned()),
        ..Response::default()
    }
}

/// Machine folders from Windows (`SHGetKnownFolderPath`), never from the
/// environment an elevated process inherits from the user; everything else (user
/// folders only add protected locations to the SafetyGate) as usual.
fn trusted_resolver() -> impl Fn(&str) -> Option<String> {
    move |v: &str| {
        if nfd_rules::MACHINE_VARS.contains(&v) {
            return nfd_platform::elevation::machine_folder(v)
                .map(|p| p.to_string_lossy().replace('\\', "/"));
        }
        use nfd_platform::Platform;
        nfd_platform::Native.resolver()(v)
    }
}

/// The request file: a plain file (not a link) of at most
/// [`elevated::MAX_REQUEST_BYTES`].
fn read_request(path: &Path) -> Result<Request, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("the request is not a plain file".into());
    }
    if meta.len() > elevated::MAX_REQUEST_BYTES {
        return Err("the request is too large".into());
    }
    let mut text = String::new();
    std::fs::File::open(path)
        .and_then(|f| {
            f.take(elevated::MAX_REQUEST_BYTES)
                .read_to_string(&mut text)
        })
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("unreadable request: {e}"))
}

fn handle(req_path: &Path) -> Response {
    if !nfd_platform::registry::is_elevated() {
        return refused("not elevated");
    }
    let request = match read_request(req_path) {
        Ok(r) => r,
        Err(e) => return refused(&e),
    };
    let packs = match nfd_rules::builtin_packs() {
        Ok(p) if !nfd_rules::has_errors(&nfd_rules::validate(&p)) => p,
        _ => return refused("the built-in rule packs do not validate"),
    };
    let resolve = trusted_resolver();
    let matcher = Matcher::new(&packs, &resolve, std::env::consts::OS);
    let running = nfd_platform::locks::running_processes().unwrap_or_default();
    elevated::execute(
        &request,
        &Context {
            matcher: &matcher,
            probe: &LiveProbe,
            running: &running,
            match_opts: MatchOptions::default(),
            now: exec::unix_now(),
        },
    )
}

/// Write the response as a new file (never over an existing one, never through a
/// link) in the request's folder, which must not be a link itself.
fn write_response(path: &Path, response: &Response) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no folder"))?;
    let info = nfd_platform::fsinfo::file_info(parent)?;
    if !info.is_dir || info.link != nfd_platform::fsinfo::LinkKind::None {
        return Err(std::io::Error::other("the response folder is a link"));
    }
    let bytes = serde_json::to_vec_pretty(response).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(&bytes)?;
    f.flush()
}

/// Entry point of the elevated step (the app's `run` calls it for [`FLAG`]). Exit
/// code 0 when a response was written, 1 when it could not be, 2 for a command
/// line of the wrong shape.
#[must_use]
pub fn run(argv: &[String]) -> i32 {
    let Some((req_path, resp_path)) = elevated::args(argv) else {
        return 2;
    };
    // before any file is opened: junctions a non-administrator made are not followed
    let _ = nfd_platform::elevation::enforce_redirection_trust();
    let response = handle(&req_path);
    i32::from(write_response(&resp_path, &response).is_err())
}

/// Run the elevated step through UAC and wait for it. `ErrorKind::Interrupted` when
/// the prompt was declined.
///
/// # Errors
/// It could not start, or it wrote no readable response.
pub fn run_elevated(
    exe: &Path,
    request: &Request,
    dir: &Path,
    owner: isize,
) -> std::io::Result<Response> {
    std::fs::create_dir_all(dir)?;
    let req: PathBuf = dir.join(format!("{}-clean-request.json", request.op_id));
    let resp: PathBuf = dir.join(format!("{}-clean-response.json", request.op_id));
    let _ = std::fs::remove_file(&resp);
    std::fs::write(
        &req,
        serde_json::to_vec_pretty(request).map_err(std::io::Error::other)?,
    )?;
    let args = format!("{FLAG} \"{}\" \"{}\"", req.display(), resp.display());
    let started = nfd_apps::runner::shell_start(exe, &args, Some("runas"), owner);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_and_stay_strict() {
        let mut req = Request::new("20261113-101500-0a1f");
        req.targets.push(elevated::Target {
            rule_id: "windows.temp.system".into(),
            path: PathBuf::from(r"C:\Windows\Temp\x"),
        });
        let text = serde_json::to_string(&req).expect("json");
        let back: Request = serde_json::from_str(&text).expect("parse");
        assert_eq!(back, req);
        // a field the protocol does not have (a rules folder, a command) is refused
        let extra = text.replacen('{', r#"{"rulesDir":"C:\\evil","#, 1);
        assert!(serde_json::from_str::<Request>(&extra).is_err());
    }

    #[test]
    fn the_request_file_must_be_a_small_plain_file() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join("r.json");
        std::fs::write(&p, b"{}").expect("write");
        assert!(read_request(&p).is_err(), "missing fields");
        assert!(read_request(dir.path()).is_err(), "a folder");
        let big = dir.path().join("big.json");
        let f = std::fs::File::create(&big).expect("create");
        f.set_len(elevated::MAX_REQUEST_BYTES + 1).expect("grow");
        assert!(read_request(&big).is_err());
    }

    #[test]
    fn the_response_never_overwrites() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join("resp.json");
        write_response(&p, &refused("x")).expect("first");
        assert!(write_response(&p, &refused("y")).is_err());
        let text = std::fs::read_to_string(&p).expect("read");
        assert!(text.contains("\"x\""));
    }

    #[test]
    fn a_wrong_command_line_does_nothing() {
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(run(&args(&["app.exe", FLAG])), 2);
        assert_eq!(run(&args(&["app.exe", FLAG, "a.json", "b.json"])), 2);
    }
}
