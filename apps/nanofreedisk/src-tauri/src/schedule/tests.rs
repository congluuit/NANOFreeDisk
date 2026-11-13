use super::*;

const T0: i64 = 1_800_000_000;

fn weekly() -> Config {
    Config {
        frequency: Frequency::Weekly,
        ..Config::default()
    }
}

fn monthly() -> Config {
    Config {
        frequency: Frequency::Monthly,
        ..Config::default()
    }
}

#[test]
fn defaults_are_off() {
    let c = Config::default();
    assert_eq!(c.frequency, Frequency::Off);
    assert!(!c.low_space);
    assert!(c.idle_only);
    let mut s = Status::default();
    s.sync(&c, T0);
    assert_eq!(s.enabled_at, None);
    assert_eq!(s.due(&c, T0), Due::Off);
}

#[test]
fn first_run_is_one_period_after_turning_it_on() {
    let c = weekly();
    let mut s = Status::default();
    s.sync(&c, T0);
    assert_eq!(s.enabled_at, Some(T0));
    assert_eq!(s.due(&c, T0), Due::NotYet(T0 + 7 * DAY));
    assert_eq!(s.due(&c, T0 + 7 * DAY - 1), Due::NotYet(T0 + 7 * DAY));
    assert_eq!(s.due(&c, T0 + 7 * DAY), Due::Now);
    // syncing again keeps the first time
    s.sync(&c, T0 + DAY);
    assert_eq!(s.enabled_at, Some(T0));
}

#[test]
fn weekly_and_monthly_boundaries() {
    for (c, every) in [(weekly(), 7 * DAY), (monthly(), 30 * DAY)] {
        let mut s = Status::default();
        s.sync(&c, T0);
        s.ran(T0, 10, 1, None);
        assert_eq!(s.due(&c, T0 + every - 1), Due::NotYet(T0 + every));
        assert_eq!(s.due(&c, T0 + every), Due::Now);
        assert_eq!(s.due(&c, T0 + every + 1), Due::Now);
    }
}

#[test]
fn a_missed_run_runs_once() {
    let c = weekly();
    let mut s = Status::default();
    s.sync(&c, T0);
    s.ran(T0, 0, 0, None);
    // the app was not running for five weeks: one run now, not five
    let late = T0 + 35 * DAY;
    assert_eq!(s.due(&c, late), Due::Now);
    s.ran(late, 0, 0, None);
    assert_eq!(s.due(&c, late), Due::NotYet(late + 7 * DAY));
    assert_eq!(s.due(&c, late + DAY), Due::NotYet(late + 7 * DAY));
}

#[test]
fn a_clock_moving_backwards_neither_runs_early_nor_blocks() {
    let c = weekly();
    let mut s = Status::default();
    s.sync(&c, T0);
    s.ran(T0, 0, 0, None);
    // the clock goes back a year: nothing is due…
    let back = T0 - 365 * DAY;
    assert!(matches!(s.due(&c, back), Due::NotYet(_)));
    // …and after syncing the next run is one period from the new "now", not a
    // year and a week away
    s.sync(&c, back);
    assert_eq!(s.last_run, Some(back));
    assert_eq!(s.due(&c, back + 7 * DAY), Due::Now);
    // a last run in the future (hand-edited file) is treated the same way
    let mut f = Status {
        last_run: Some(T0 + 1000 * DAY),
        ..Status::default()
    };
    assert_eq!(f.due(&c, T0), Due::NotYet(T0 + 7 * DAY));
    f.sync(&c, T0);
    assert_eq!(f.last_run, Some(T0));
}

#[test]
fn turning_it_off_and_on_again_restarts_the_period() {
    let mut s = Status::default();
    s.sync(&weekly(), T0);
    s.sync(&Config::default(), T0 + DAY);
    assert_eq!(s.enabled_at, None);
    s.sync(&monthly(), T0 + 2 * DAY);
    assert_eq!(s.due(&monthly(), T0 + 3 * DAY), Due::NotYet(T0 + 32 * DAY));
}

const PACK: &str = r#"
[pack]
id = "test"
name = "Test"
version = "1"

[[rule]]
id = "t.temp"
vendor = "V"
app = "A"
category = "system"
platform = ["windows", "macos", "linux"]
paths = ["${TEMP}/**"]
level = "safe"
regenerates = true
impact = "x"
verified_on = 2026-11-13

[[rule]]
id = "t.browser"
vendor = "V"
app = "Browser"
category = "browser"
platform = ["windows", "macos", "linux"]
paths = ["${LOCALAPPDATA}/B/Cache/**"]
level = "safe"
regenerates = true
requires_closed = ["browser.exe"]
impact = "x"
verified_on = 2026-11-13

[[rule]]
id = "t.logs"
vendor = "V"
app = "A"
category = "system"
platform = ["windows", "macos", "linux"]
paths = ["${LOCALAPPDATA}/A/logs/**"]
level = "safe"
impact = "kept: does not regenerate"
verified_on = 2026-11-13

[[rule]]
id = "t.admin"
vendor = "V"
app = "A"
category = "system"
platform = ["windows", "macos", "linux"]
paths = ["${SYSTEMROOT}/Temp/**"]
level = "safe"
regenerates = true
admin = true
impact = "x"
verified_on = 2026-11-13

[[rule]]
id = "t.caution"
vendor = "V"
app = "A"
category = "system"
platform = ["windows", "macos", "linux"]
paths = ["${LOCALAPPDATA}/A/old/**"]
level = "caution"
regenerates = true
impact = "x"
verified_on = 2026-11-13

[[rule]]
id = "t.bin"
vendor = "V"
app = "A"
category = "system"
platform = ["windows", "macos", "linux"]
paths = ["${SYSTEMDRIVE}/$Recycle.Bin/**"]
level = "safe"
regenerates = true
impact = "x"
verified_on = 2026-11-13

[[rule]]
id = "t.winsxs"
vendor = "V"
app = "A"
category = "system"
platform = ["windows", "macos", "linux"]
paths = ["${SYSTEMROOT}/WinSxS"]
level = "protected"
tool = "disk-cleanup"
impact = "x"
verified_on = 2026-11-13

[[rule]]
id = "t.mac-only"
vendor = "V"
app = "A"
category = "system"
platform = ["macos"]
paths = ["${HOME}/Library/Caches/A/**"]
level = "safe"
regenerates = true
impact = "x"
verified_on = 2026-11-13
"#;

fn packs() -> Vec<nfd_rules::Pack> {
    vec![nfd_rules::parse_pack(PACK, None).expect("test pack")]
}

#[test]
fn only_safe_regenerating_user_rules_qualify() {
    let packs = packs();
    let c = weekly();
    assert_eq!(
        picks(&packs, &c, Some(&[]), "windows"),
        vec!["t.temp".to_owned(), "t.browser".to_owned()]
    );
    let why = |id: &str, running: Option<&[String]>| {
        let r = packs[0].rules.iter().find(|r| r.id == id).expect("rule");
        qualifies(&RuleFacts::of(r), &c, running)
    };
    assert_eq!(why("t.logs", Some(&[])), Err(Exclusion::NotRegenerating));
    assert_eq!(why("t.admin", Some(&[])), Err(Exclusion::Admin));
    assert_eq!(why("t.caution", Some(&[])), Err(Exclusion::NotSafe));
    assert_eq!(why("t.bin", Some(&[])), Err(Exclusion::RecycleBin));
    assert_eq!(why("t.winsxs", Some(&[])), Err(Exclusion::NotSafe));
}

#[test]
fn a_running_program_or_an_unknown_process_list_keeps_its_rule_out() {
    let packs = packs();
    let c = weekly();
    let running = vec!["browser.exe".to_owned()];
    assert_eq!(
        picks(&packs, &c, Some(&running), "windows"),
        vec!["t.temp".to_owned()]
    );
    // the process list could not be read: assume the program is open
    assert_eq!(
        picks(&packs, &c, None, "windows"),
        vec!["t.temp".to_owned()]
    );
}

#[test]
fn categories_can_only_be_left_out() {
    let packs = packs();
    assert_eq!(
        categories(&packs, "windows"),
        vec!["browser".to_owned(), "system".to_owned()]
    );
    let c = Config {
        off_categories: vec!["SYSTEM".into()],
        ..weekly()
    };
    assert_eq!(
        picks(&packs, &c, Some(&[]), "windows"),
        vec!["t.browser".to_owned()]
    );
}

#[test]
fn a_hostile_settings_file_cannot_widen_the_selection() {
    let packs = packs();
    // fields the app never writes (a rule list, a level, "expert", admin) are
    // ignored; categories of excluded rules do not let them in either
    let json = r#"{
        "frequency": "weekly",
        "rules": ["t.caution", "t.admin", "t.logs"],
        "level": "caution",
        "expert": true,
        "admin": true,
        "permanent": true,
        "offCategories": [],
        "categories": ["system"]
    }"#;
    let c: Config = serde_json::from_str(json).expect("parses, extras ignored");
    let picked = picks(&packs, &c, Some(&[]), "windows");
    assert_eq!(picked, vec!["t.temp".to_owned(), "t.browser".to_owned()]);
    for bad in ["t.caution", "t.admin", "t.logs", "t.bin", "t.winsxs"] {
        assert!(!picked.iter().any(|p| p == bad), "{bad} must never qualify");
    }
    // an unknown frequency does not parse: the caller then treats it as off
    assert!(serde_json::from_str::<Config>(r#"{"frequency": "hourly"}"#).is_err());
    // silly numbers are pulled back into range
    let c: Config = serde_json::from_str(r#"{"lowPercent": 1e9, "lowGb": -5}"#).expect("numbers");
    let c = c.sanitized();
    assert!((c.low_percent - 50.0).abs() < f64::EPSILON);
    assert!((c.low_gb - 1.0).abs() < f64::EPSILON);
}

#[test]
fn builtin_picks_are_all_safe_regenerating_and_never_admin() {
    let packs = nfd_rules::builtin_packs().expect("builtin packs");
    let picked = picks(&packs, &weekly(), Some(&[]), "windows");
    assert!(!picked.is_empty());
    for id in &picked {
        let r = packs
            .iter()
            .flat_map(|p| &p.rules)
            .find(|r| &r.id == id)
            .expect("rule");
        assert_eq!(r.level, RiskLevel::Safe, "{id}");
        assert!(r.regenerates, "{id}");
        assert!(!r.admin, "{id}");
        assert!(r.tool.is_none(), "{id}");
    }
    assert!(!picked.iter().any(|p| p == "windows.temp.system"));
    // Explorer is always running: its thumbnail cache is never cleaned automatically
    let running = vec!["explorer.exe".to_owned()];
    let picked = picks(&packs, &weekly(), Some(&running), "windows");
    assert!(!picked.iter().any(|p| p == "windows.explorer.thumbcache"));
}

const GB: u64 = 1 << 30;

#[test]
fn threshold_is_the_smaller_of_percent_and_gb() {
    let c = Config::default();
    // 1 TB drive: 10 % is 100 GB, 10 GB is smaller
    assert_eq!(low_threshold(1000 * GB, &c), 10 * GB);
    // 64 GB drive: 10 % is 6.4 GB, smaller than 10 GB
    assert_eq!(low_threshold(64 * GB, &c), 64 * GB / 10);
}

#[test]
fn low_space_alerts_once_per_crossing_and_at_most_daily() {
    let c = Config {
        low_space: true,
        ..Config::default()
    };
    let mut s = Status::default();
    let d = |free: u64| vec![("C:\\".to_owned(), free, 500 * GB)];
    assert!(low_space(&mut s, &d(50 * GB), &c, T0).is_empty());
    let a = low_space(&mut s, &d(5 * GB), &c, T0);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].threshold, 10 * GB);
    // still low a day later: no second notification
    assert!(low_space(&mut s, &d(4 * GB), &c, T0 + 2 * DAY).is_empty());
    // a little above the line is not "recovered" (margin of at least 1 GiB)
    assert!(low_space(&mut s, &d(10 * GB + GB / 2), &c, T0 + 2 * DAY).is_empty());
    assert!(low_space(&mut s, &d(5 * GB), &c, T0 + 2 * DAY).is_empty());
    // recovered, then low again within a day: quiet
    assert!(low_space(&mut s, &d(20 * GB), &c, T0 + 2 * DAY).is_empty());
    let a = low_space(&mut s, &d(5 * GB), &c, T0 + 2 * DAY + 60);
    assert_eq!(a.len(), 1, "two days after the first alert");
    assert!(low_space(&mut s, &d(20 * GB), &c, T0 + 2 * DAY + 120).is_empty());
    assert!(low_space(&mut s, &d(5 * GB), &c, T0 + 2 * DAY + 180).is_empty());
    // the next crossing after a full day alerts again
    assert!(low_space(&mut s, &d(20 * GB), &c, T0 + 4 * DAY).is_empty());
    assert_eq!(low_space(&mut s, &d(5 * GB), &c, T0 + 4 * DAY).len(), 1);
}

#[test]
fn low_space_off_changes_nothing() {
    let mut s = Status::default();
    let d = vec![("C:\\".to_owned(), GB, 500 * GB)];
    assert!(low_space(&mut s, &d, &Config::default(), T0).is_empty());
    assert!(s.drives.is_empty());
    // drives without a size are skipped
    let c = Config {
        low_space: true,
        ..Config::default()
    };
    assert!(low_space(&mut s, &[("X:\\".to_owned(), 0, 0)], &c, T0).is_empty());
}

#[test]
fn autostart_entries() {
    assert_eq!(
        autostart_command(r"C:\Apps\NANOFreeDisk\nanofreedisk.exe"),
        r#""C:\Apps\NANOFreeDisk\nanofreedisk.exe" --tray"#
    );
    let plist = launch_agent(
        "vn.io.nanodev.nanofreedisk",
        "/Apps/A&B.app/Contents/MacOS/x",
    );
    assert!(plist.contains("<string>/Apps/A&amp;B.app/Contents/MacOS/x</string>"));
    assert!(plist.contains("<string>--tray</string>"));
    assert!(plist.contains("<key>RunAtLoad</key>"));
}
