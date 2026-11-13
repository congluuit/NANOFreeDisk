//! The one elevated step of a Clean (`docs/PHASE3_CLEAN.md` §8): junk that only an
//! administrator may remove (`%SystemRoot%\Temp`, CBS logs, kernel dumps, the
//! Windows Update download cache) and that the normal, unelevated run had to leave.
//!
//! The app writes a [`Request`] to a file in its own data folder and starts itself
//! once through UAC as `nanofreedisk.exe --elevated-clean <request> <response>`,
//! only after the user pressed a button that says administrator rights are needed.
//! That file is writable by every program running as the user, so its content is
//! **untrusted**: it can only *narrow* what the elevated process would clean on its
//! own, never widen it. For every [`Target`] the elevated process
//!
//! 1. refuses anything that is not a plain absolute drive path (relative, `.` / `..`,
//!    `\\?\`, UNC, device paths) ([`refuse_target`]);
//! 2. looks the rule up in the packs **it** loaded (built into the binary, never a
//!    folder named by the request) and refuses a rule that is unknown, not marked
//!    `admin`, Protected, or Advanced without Expert mode;
//! 3. refuses a path that rule does not match, a path a Keep rule keeps, and a path
//!    the lexical SafetyGate refuses;
//! 4. scans the accepted rules again **itself** and keeps only the fresh candidates
//!    that lie at or below an accepted target ([`select`]): sizes, dates and the
//!    "may it go as a whole" split all come from this scan, which never follows a
//!    link and never offers one;
//! 5. runs the executor, which checks every item with the live SafetyGate right
//!    before acting (still there, not a link, final path allowed, same volume, not a
//!    SYSTEM file, not a program image under `%SystemRoot%`, unchanged since the
//!    scan), plus [`exact_path`]: the final path must be the path itself, so no link
//!    or short name anywhere along the parents leads somewhere else.
//!
//! Items go to the **Recycle Bin** (restorable from Explorer, which asks for the
//! rights itself). Not the quarantine: it lives in the user's own profile, which the
//! user (and any program of theirs) can write, so an administrator moving files into
//! it could be redirected, and the unelevated app could not purge what it got.
//! `permanent` is honoured only for Safe rules that regenerate (pure caches and
//! temporary files), as in the normal run.
//!
//! Nothing here can be reached without the UAC prompt, there is no long-lived
//! elevated process, and the response only reports what happened.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::exec::{self, ExecOptions, ItemResult, QuarantineRoots, Refusal};
use nfd_core::plan::{self, CleanPlan, DeleteMode, PlanItem, PlanOptions};
use nfd_core::safety::{Origin, Probe};
use serde::{Deserialize, Serialize};

use crate::matcher::{Candidate, MatchOptions, MatchReport, Matcher};

/// Command-line switch: `--elevated-clean <request.json> <response.json>`.
pub const FLAG: &str = "--elevated-clean";
/// Wire version; a mismatch is refused (an older app left a request behind).
pub const PROTOCOL: u32 = 1;
/// Most targets one request may name.
pub const MAX_TARGETS: usize = 10_000;
/// Largest request file read.
pub const MAX_REQUEST_BYTES: u64 = 8 << 20;
/// Activity kind of the elevated run.
pub const KIND: &str = "clean-admin";

/// One leftover of an earlier run to clean with administrator rights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    pub rule_id: String,
    pub path: PathBuf,
}

/// What the app asks the elevated step to do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol: u32,
    /// Operation id the result is journaled under (`YYYYMMDD-HHMMSS-xxxx`).
    pub op_id: String,
    /// Advanced rules are accepted only in Expert mode.
    pub expert: bool,
    /// Delete for good instead of the Recycle Bin (Safe rules that regenerate
    /// only; everything else still goes to the Recycle Bin).
    pub permanent: bool,
    pub targets: Vec<Target>,
}

impl Request {
    #[must_use]
    pub fn new(op_id: &str) -> Self {
        Self {
            protocol: PROTOCOL,
            op_id: op_id.to_owned(),
            ..Self::default()
        }
    }
}

/// A target the elevated step refused, with a stable code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refused {
    pub rule_id: String,
    pub path: PathBuf,
    pub code: String,
}

/// What the elevated step did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub protocol: u32,
    /// Results of the items it acted on (numbered from 0).
    pub items: Vec<ItemResult>,
    /// Targets it refused before acting.
    pub refused: Vec<Refused>,
    /// The whole request was refused.
    pub error: Option<String>,
}

impl Response {
    fn refused(why: &str) -> Self {
        Self {
            protocol: PROTOCOL,
            error: Some(why.to_owned()),
            ..Self::default()
        }
    }
}

/// `YYYYMMDD-HHMMSS-xxxx` ([`exec::new_op_id`]): nothing a file name could misread.
#[must_use]
pub fn valid_op_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 15 => *c == b'-',
            0..=7 | 9..=14 => c.is_ascii_digit(),
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
        })
}

/// A plain absolute path: on Windows a drive path (`C:\…`, no `\\?\`, UNC or device
/// prefix), no `.` / `..` components, no name ending in a dot or a space (Windows
/// would silently drop those), nothing the comparable form would have to rewrite.
fn plain_absolute(path: &Path) -> bool {
    if !path.is_absolute() || exec::path_key(path).is_none() {
        return false;
    }
    path.components().all(|c| match c {
        Component::Prefix(p) => !cfg!(windows) || matches!(p.kind(), std::path::Prefix::Disk(_)),
        Component::RootDir => true,
        Component::CurDir | Component::ParentDir => false,
        Component::Normal(n) => {
            let n = n.to_string_lossy();
            !n.is_empty() && !n.ends_with(['.', ' ']) && !n.contains('\0')
        }
    })
}

/// Why the elevated step refuses `t` before scanning anything, if it does (see the
/// module documentation, steps 1 to 3).
#[must_use]
pub fn refuse_target(t: &Target, req: &Request, matcher: &Matcher) -> Option<&'static str> {
    if !plain_absolute(&t.path) {
        return Some("relative-path");
    }
    let Some((rule, _)) = matcher.rule_by_id(&t.rule_id) else {
        return Some("unknown-rule");
    };
    if !rule.admin {
        return Some("not-admin-rule");
    }
    match rule.level {
        RiskLevel::Protected => return Some("protected-level"),
        RiskLevel::Advanced if !req.expert => return Some("expert-only"),
        _ => {}
    }
    if !matcher
        .rules_matching(&t.path)
        .iter()
        .any(|(r, _)| r.id == rule.id)
    {
        return Some("not-matched");
    }
    if matcher.is_kept(&t.path) {
        return Some("kept");
    }
    if let Err(b) = matcher
        .gate()
        .check_path(&t.path, Origin::Rule { level: rule.level })
    {
        return Some(b.code());
    }
    None
}

/// The fresh candidates of `report` that lie at or below one of `targets` (same rule),
/// and for each target whether any did.
#[must_use]
pub fn select(report: &MatchReport, targets: &[&Target]) -> (Vec<Candidate>, Vec<bool>) {
    let mut hit = vec![false; targets.len()];
    let mut out = Vec::new();
    let keys: Vec<Option<String>> = targets.iter().map(|t| exec::path_key(&t.path)).collect();
    for c in &report.candidates {
        let Some(ck) = exec::path_key(&c.path) else {
            continue;
        };
        let mut taken = false;
        for (i, t) in targets.iter().enumerate() {
            let inside = keys[i]
                .as_deref()
                .is_some_and(|tk| exec::key_within(&ck, tk));
            if inside && t.rule_id == c.rule_id {
                hit[i] = true;
                taken = true;
            }
        }
        if taken {
            out.push(c.clone());
        }
    }
    (out, hit)
}

/// The plan of the selected candidates: nested items merged, Advanced selected only
/// in Expert mode, every item to the Recycle Bin except, with `permanent`, items of
/// Safe rules that regenerate.
#[must_use]
pub fn build_plan(
    req: &Request,
    matcher: &Matcher,
    report: &MatchReport,
    found: &[Candidate],
) -> CleanPlan {
    let items: Vec<PlanItem> = found
        .iter()
        .map(|c| c.to_plan_item(report.requires_closed(&c.rule_id)))
        .collect();
    let mut plan = plan::build(
        items,
        PlanOptions {
            expert: req.expert,
            include_caution: true,
            permanent: false,
        },
    );
    for item in &mut plan.items {
        let rule = item
            .rule_id
            .as_deref()
            .and_then(|id| matcher.rule_by_id(id))
            .map(|(r, _)| r);
        item.selected = rule.is_some()
            && match item.level() {
                RiskLevel::Safe | RiskLevel::Caution => true,
                RiskLevel::Advanced => req.expert,
                RiskLevel::Protected => false,
            };
        let pure_cache = rule.is_some_and(|r| r.level == RiskLevel::Safe && r.regenerates);
        item.mode = if req.permanent && pure_cache {
            DeleteMode::Permanent
        } else {
            DeleteMode::RecycleBin
        };
    }
    plan
}

/// The last check before an item is touched: its final path (every link and short
/// name resolved) must be the path itself.
///
/// # Errors
/// The path cannot be resolved, or resolves somewhere else.
pub fn exact_path(probe: &dyn Probe, path: &Path) -> Result<(), Refusal> {
    match probe.final_path(path) {
        Ok(real) if exec::same_path(&real, path) => Ok(()),
        Ok(real) => Err(Refusal {
            code: "path-changed",
            message: format!("the path leads to {}", real.display()),
        }),
        Err(e) => Err(Refusal {
            code: "unresolvable",
            message: e.to_string(),
        }),
    }
}

/// What the elevated step works with on this machine.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// Built from the packs compiled into the binary, with machine folders that
    /// came from Windows (not from the environment).
    pub matcher: &'a Matcher,
    pub probe: &'a dyn Probe,
    /// Running exe names (for the rules' `requires_closed` notes).
    pub running: &'a [String],
    pub match_opts: MatchOptions,
    pub now: i64,
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").field("now", &self.now).finish()
    }
}

/// Carry out `req` (already elevated): see the module documentation.
#[must_use]
pub fn execute(req: &Request, cx: &Context<'_>) -> Response {
    if req.protocol != PROTOCOL {
        return Response::refused(&format!("protocol {} != {PROTOCOL}", req.protocol));
    }
    if !valid_op_id(&req.op_id) {
        return Response::refused("bad operation id");
    }
    if req.targets.is_empty() || req.targets.len() > MAX_TARGETS {
        return Response::refused("bad number of targets");
    }
    let mut out = Response {
        protocol: PROTOCOL,
        ..Response::default()
    };
    let refuse = |t: &Target, code: &str| Refused {
        rule_id: t.rule_id.clone(),
        path: t.path.clone(),
        code: code.to_owned(),
    };
    let mut accepted: Vec<&Target> = Vec::new();
    for t in &req.targets {
        match refuse_target(t, req, cx.matcher) {
            Some(code) => out.refused.push(refuse(t, code)),
            None => accepted.push(t),
        }
    }
    if accepted.is_empty() {
        return out;
    }
    let ids: BTreeSet<&str> = accepted.iter().map(|t| t.rule_id.as_str()).collect();
    let report = cx.matcher.scan(
        &|r, _| ids.contains(r.id.as_str()),
        &cx.match_opts,
        cx.running,
    );
    let (found, hit) = select(&report, &accepted);
    for (t, h) in accepted.iter().zip(&hit) {
        if !h {
            out.refused.push(refuse(t, "not-found-now"));
        }
    }
    let plan = build_plan(req, cx.matcher, &report, &found);
    if plan.selected().next().is_none() {
        return out;
    }
    let opts = ExecOptions {
        op_id: req.op_id.clone(),
        kind: KIND.into(),
        owner: 0,
        // no quarantine: see the module documentation
        roots: QuarantineRoots::default(),
        now: cx.now,
    };
    let probe = cx.probe;
    let op = exec::execute_verified(
        &plan,
        cx.matcher.gate(),
        probe,
        &opts,
        &mut |item| exact_path(probe, &item.path),
        &mut |_, _, _| true,
    );
    out.items = op.items;
    out
}

/// `exe --elevated-clean <request> <response>`, exactly: both absolute `.json`
/// files in the same folder. Anything else on the command line is refused.
#[must_use]
pub fn args(argv: &[String]) -> Option<(PathBuf, PathBuf)> {
    if argv.len() != 4 || argv[1] != FLAG {
        return None;
    }
    let (req, resp) = (PathBuf::from(&argv[2]), PathBuf::from(&argv[3]));
    let json = |p: &Path| {
        plain_absolute(p)
            && p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    };
    let same_dir = match (req.parent(), resp.parent()) {
        (Some(a), Some(b)) => exec::same_path(a, b),
        _ => false,
    };
    (json(&req) && json(&resp) && same_dir && !exec::same_path(&req, &resp)).then_some((req, resp))
}

// Windows paths and the Windows-only admin rules: like the leftovers' elevated
// step, the tests run on Windows.
#[cfg(all(test, windows))]
mod tests {
    use std::fs;
    use std::io;

    use nfd_core::exec::ItemStatus;
    use nfd_core::safety::LiveProbe;
    use nfd_core::safety::gate::Facts;

    use super::*;
    use crate::parse_pack;

    const PACK: &str = r#"
[pack]
id = "t"
name = "test"
version = "0.0.1"

[[rule]]
id = "t.systemp"
vendor = "Microsoft"
app = "Windows"
category = "system"
paths = ["${SYSTEMROOT}/Temp/**"]
level = "safe"
regenerates = true
admin = true
impact = "System-wide temporary files."
verified_on = 2026-11-13

[[rule]]
id = "t.update"
vendor = "Microsoft"
app = "Windows Update"
category = "system"
paths = ["${SYSTEMROOT}/SoftwareDistribution/Download/**"]
level = "advanced"
regenerates = true
admin = true
impact = "Downloaded update packages."
verified_on = 2026-11-13

[[rule]]
id = "t.keep"
vendor = "Microsoft"
app = "Windows"
category = "system"
paths = ["${SYSTEMROOT}/Temp/keep/**"]
level = "protected"
impact = "Kept on purpose by this test."
verified_on = 2026-11-13

[[rule]]
id = "t.user"
vendor = "Microsoft"
app = "Windows"
category = "system"
paths = ["${SYSTEMROOT}/Logs/CBS/*.log"]
level = "safe"
regenerates = true
impact = "Not marked admin in this test."
verified_on = 2026-11-13
"#;

    struct Fixture {
        _dir: tempfile::TempDir,
        base: PathBuf,
        matcher: Matcher,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tmp");
            // the long form of the folder (CI runners have `RUNNER~1` in TEMP): the
            // elevated step refuses a path whose final form differs
            let long = fs::canonicalize(dir.path()).expect("canonicalize");
            let long = PathBuf::from(long.to_string_lossy().trim_start_matches(r"\\?\"));
            let win = long.join("win");
            for d in [
                "Temp/keep",
                "SoftwareDistribution/Download",
                "Logs/CBS",
                "System32",
            ] {
                fs::create_dir_all(win.join(d)).expect("mkdir");
            }
            let root = win.to_string_lossy().replace('\\', "/");
            let resolve = move |v: &str| (v == "SYSTEMROOT").then(|| root.clone());
            let pack = parse_pack(PACK, None).expect("pack");
            assert!(!crate::has_errors(&crate::validate(std::slice::from_ref(
                &pack
            ))));
            let matcher = Matcher::new(&[pack], &resolve, "windows");
            Self {
                _dir: dir,
                base: long,
                matcher,
            }
        }

        fn win(&self, rel: &str) -> PathBuf {
            self.base.join("win").join(rel)
        }

        fn write(&self, rel: &str) -> PathBuf {
            let p = self.win(rel);
            fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            fs::write(&p, b"junk").expect("write");
            p
        }

        fn run(&self, req: &Request) -> Response {
            execute(
                req,
                &Context {
                    matcher: &self.matcher,
                    probe: &LiveProbe,
                    running: &[],
                    match_opts: MatchOptions::default(),
                    now: exec::unix_now(),
                },
            )
        }
    }

    fn target(rule: &str, path: PathBuf) -> Target {
        Target {
            rule_id: rule.into(),
            path,
        }
    }

    fn request(targets: Vec<Target>) -> Request {
        Request {
            protocol: PROTOCOL,
            op_id: "20261113-101500-0a1f".into(),
            expert: false,
            permanent: true,
            targets,
        }
    }

    fn code(f: &Fixture, t: Target) -> Option<&'static str> {
        refuse_target(&t, &request(Vec::new()), &f.matcher)
    }

    #[test]
    fn hostile_targets_are_refused_before_anything_is_scanned() {
        let f = Fixture::new();
        let ok = f.win("Temp/old.log");
        assert_eq!(code(&f, target("t.systemp", ok.clone())), None);
        // relative, `..`, verbatim and UNC forms
        for p in [
            PathBuf::from(r"Temp\old.log"),
            PathBuf::from(r"\Windows\Temp\old.log"),
            f.win(r"Temp\..\System32\kernel32.dll"),
            PathBuf::from(format!(r"\\?\{}", ok.display())),
            PathBuf::from(r"\\server\share\Temp\old.log"),
            f.win("Temp/old.log."),
        ] {
            assert_eq!(
                code(&f, target("t.systemp", p.clone())),
                Some("relative-path"),
                "{}",
                p.display()
            );
        }
        // outside every rule: the rule decides, not the request
        assert_eq!(
            code(&f, target("t.systemp", f.win("System32/drivers/etc/hosts"))),
            Some("not-matched")
        );
        assert_eq!(
            code(&f, target("t.systemp", f.base.join("elsewhere.log"))),
            Some("not-matched")
        );
        // the base folder itself is not a match of `Temp/**`
        assert_eq!(
            code(&f, target("t.systemp", f.win("Temp"))),
            Some("not-matched")
        );
        // a rule the pack does not have, one not marked admin, a Keep rule
        assert_eq!(code(&f, target("t.nope", ok.clone())), Some("unknown-rule"));
        assert_eq!(
            code(&f, target("t.user", f.win("Logs/CBS/a.log"))),
            Some("not-admin-rule")
        );
        assert_eq!(
            code(&f, target("t.keep", f.win("Temp/keep/x"))),
            Some("not-admin-rule")
        );
        // matched by the admin rule but kept by a Keep rule
        assert_eq!(
            code(&f, target("t.systemp", f.win("Temp/keep/x"))),
            Some("kept")
        );
        // Advanced only in Expert mode
        let upd = target("t.update", f.win("SoftwareDistribution/Download/abc"));
        assert_eq!(code(&f, upd.clone()), Some("expert-only"));
        let mut expert = request(Vec::new());
        expert.expert = true;
        assert_eq!(refuse_target(&upd, &expert, &f.matcher), None);
        // what the lexical SafetyGate refuses
        assert_eq!(
            code(&f, target("t.systemp", f.win("Temp/nul"))),
            Some("device")
        );
        assert_eq!(
            code(&f, target("t.systemp", f.win("Temp/a.log:evil"))),
            Some("stream")
        );
    }

    #[test]
    fn only_fresh_candidates_inside_accepted_targets_are_cleaned() {
        let f = Fixture::new();
        let old = f.write("Temp/old.log");
        let other = f.write("Temp/other.log");
        let kept = f.write("Temp/keep/k.log");
        let resp = f.run(&request(vec![
            target("t.systemp", old.clone()),
            target("t.systemp", f.win("Temp/gone.log")),
            target("t.systemp", f.win("System32/config/SAM")),
            target("t.user", f.win("Logs/CBS/x.log")),
        ]));
        assert!(resp.error.is_none(), "{resp:?}");
        assert_eq!(resp.items.len(), 1, "{resp:#?}");
        assert_eq!(resp.items[0].status, ItemStatus::Done);
        assert_eq!(resp.items[0].mode, DeleteMode::Permanent);
        assert!(!old.exists());
        assert!(other.exists(), "not requested");
        assert!(kept.exists());
        let codes: Vec<&str> = resp.refused.iter().map(|r| r.code.as_str()).collect();
        assert_eq!(codes, ["not-matched", "not-admin-rule", "not-found-now"]);
    }

    #[test]
    fn junctions_are_never_followed() {
        let f = Fixture::new();
        let precious = f.base.join("precious");
        fs::create_dir_all(&precious).expect("mkdir");
        fs::write(precious.join("data.txt"), b"keep me").expect("write");
        let link = f.win("Temp/link");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&precious)
            .output()
            .is_ok_and(|o| o.status.success());
        if !ok {
            return;
        }
        let resp = f.run(&request(vec![
            target("t.systemp", link.clone()),
            target("t.systemp", link.join("data.txt")),
        ]));
        assert!(resp.items.is_empty(), "{resp:#?}");
        assert!(resp.refused.iter().all(|r| r.code == "not-found-now"));
        assert_eq!(
            fs::read(precious.join("data.txt")).expect("read"),
            b"keep me"
        );
        fs::remove_dir(&link).expect("rm link");
    }

    /// A probe whose final path is somewhere else (a link appeared among the
    /// parents after the scan).
    struct Moved;

    impl Probe for Moved {
        fn facts(&self, path: &Path) -> io::Result<Facts> {
            LiveProbe.facts(path)
        }
        fn final_path(&self, _: &Path) -> io::Result<PathBuf> {
            Ok(PathBuf::from(r"C:\Windows\System32\config\SAM"))
        }
        fn root_volume(&self, root: &Path) -> Option<u64> {
            LiveProbe.root_volume(root)
        }
        fn lockers(&self, _: &Path, _: bool) -> Vec<String> {
            Vec::new()
        }
        fn running(&self) -> Vec<String> {
            Vec::new()
        }
    }

    #[test]
    fn a_path_that_resolves_elsewhere_is_left_alone() {
        let f = Fixture::new();
        let old = f.write("Temp/old.log");
        assert!(exact_path(&LiveProbe, &old).is_ok());
        let refusal = exact_path(&Moved, &old).expect_err("elsewhere");
        assert_eq!(refusal.code, "path-changed");
        // through the whole run: the gate refuses the final path first, the exact
        // check would refuse it otherwise; either way nothing moves
        let resp = execute(
            &request(vec![target("t.systemp", old.clone())]),
            &Context {
                matcher: &f.matcher,
                probe: &Moved,
                running: &[],
                match_opts: MatchOptions::default(),
                now: exec::unix_now(),
            },
        );
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].status, ItemStatus::Skipped, "{resp:#?}");
        assert!(old.exists());
    }

    #[test]
    fn bad_requests_are_refused_whole() {
        let f = Fixture::new();
        let old = f.write("Temp/old.log");
        let mut r = request(vec![target("t.systemp", old.clone())]);
        r.protocol = 0;
        assert!(f.run(&r).error.is_some());
        let mut r = request(vec![target("t.systemp", old.clone())]);
        r.op_id = r"..\..\x".into();
        assert!(f.run(&r).error.is_some());
        assert!(f.run(&request(Vec::new())).error.is_some());
        assert!(old.exists());
        // unknown fields are not silently ignored (the request is JSON on the wire;
        // this crate has no JSON dependency, and the TOML deserializer follows the
        // same serde attributes)
        let text = "protocol = 1\nopId = \"20261113-101500-0a1f\"\nexpert = false\npermanent = false\ntargets = []\n";
        assert!(toml::from_str::<Request>(text).is_ok());
        let extra = format!("{text}rulesDir = 'C:/evil'\n");
        assert!(toml::from_str::<Request>(&extra).is_err());
    }

    #[test]
    fn modes_follow_the_rule_not_the_request() {
        let f = Fixture::new();
        f.write("Temp/a.log");
        f.write("SoftwareDistribution/Download/b.cab");
        let report = f.matcher.scan(&|_, _| true, &MatchOptions::default(), &[]);
        let mut req = request(Vec::new());
        req.expert = true;
        req.permanent = false;
        let plan = build_plan(&req, &f.matcher, &report, &report.candidates);
        assert!(plan.items.iter().all(|i| i.mode == DeleteMode::RecycleBin));
        req.permanent = true;
        let plan = build_plan(&req, &f.matcher, &report, &report.candidates);
        for i in &plan.items {
            let want = if i.rule_id.as_deref() == Some("t.systemp") {
                DeleteMode::Permanent
            } else {
                // Advanced: never for good, even when asked
                DeleteMode::RecycleBin
            };
            assert_eq!(i.mode, want, "{}", i.path.display());
            assert!(i.selected);
        }
        req.expert = false;
        let plan = build_plan(&req, &f.matcher, &report, &report.candidates);
        assert!(
            plan.items
                .iter()
                .filter(|i| i.level() == RiskLevel::Advanced)
                .all(|i| !i.selected)
        );
    }

    #[test]
    fn operation_ids_and_arguments() {
        assert!(valid_op_id("20261113-101500-0a1f"));
        assert!(!valid_op_id("20261113-101500-0A1F"));
        assert!(!valid_op_id(r"..\..\evil"));
        assert!(!valid_op_id(""));
        let a = |v: &[&str]| args(&v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert!(a(&["app.exe", FLAG, r"C:\d\req.json", r"C:\d\resp.json"]).is_some());
        // other folders, relative, not json, the same file, extra or missing arguments
        assert!(a(&["app.exe", FLAG, r"C:\d\req.json", r"C:\Windows\resp.json"]).is_none());
        assert!(a(&["app.exe", FLAG, "req.json", r"C:\d\resp.json"]).is_none());
        assert!(a(&["app.exe", FLAG, r"C:\d\req.txt", r"C:\d\resp.json"]).is_none());
        assert!(a(&["app.exe", FLAG, r"C:\d\req.json", r"C:\d\req.json"]).is_none());
        assert!(a(&["app.exe", FLAG, r"C:\d\a.json", r"C:\d\b.json", "--x"]).is_none());
        assert!(a(&["app.exe", "--x", FLAG, r"C:\d\a.json"]).is_none());
        assert!(a(&["app.exe", FLAG]).is_none());
    }
}
