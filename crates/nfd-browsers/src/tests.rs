#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use nfd_core::exec::{self, ExecOptions, ItemStatus, QuarantineRoots, RestoreStatus};
use nfd_core::safety::{LiveProbe, SafetyGate};

use super::*;

/// `<root>/Local/Google/Chrome/User Data/Default` and a Firefox profile.
fn machine() -> (tempfile::TempDir, Env, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let env = Env::fixture(root.path());
    let chrome = root
        .path()
        .join("Local")
        .join("Google")
        .join("Chrome")
        .join("User Data")
        .join("Default");
    fixture::chromium_profile(&chrome);
    // a folder of User Data that is not a profile
    std::fs::create_dir_all(chrome.parent().unwrap().join("Crashpad")).unwrap();
    let ff = root
        .path()
        .join("Local")
        .join("Mozilla")
        .join("Firefox")
        .join("Profiles")
        .join("abcd.default-release");
    fixture::firefox_profile(&ff);
    (root, env, chrome, ff)
}

fn target(browser: &str, site: &str, subdomains: bool) -> Target {
    Target {
        browser: browser.into(),
        profile: None,
        site: site.into(),
        subdomains,
    }
}

#[test]
fn every_browser_has_its_safe_rule() {
    let packs = nfd_rules::builtin_packs().unwrap();
    let pack = packs.iter().find(|p| p.pack.id == "browsers").unwrap();
    for b in BROWSERS {
        let rule = pack.rules.iter().find(|r| r.id == b.rule_id).unwrap();
        assert_eq!(rule.level, RiskLevel::Safe, "{}", b.id);
        assert!(rule.requires_closed.iter().any(|e| e == b.exe), "{}", b.id);
    }
}

#[test]
fn scan_lists_sites_per_store() {
    let (_root, env, chrome, _) = machine();
    let r = scan(&env, &ScanOptions::default());
    let ids: Vec<&str> = r.browsers.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids, ["chrome", "firefox"]);
    let c = &r.browsers[0];
    assert!(!c.running);
    assert_eq!(c.profiles.len(), 1, "Crashpad is not a profile");
    let p = &c.profiles[0];
    assert_eq!(p.name, "Default");
    assert_eq!(p.path, chrome);
    let http = p
        .stores
        .iter()
        .find(|s| s.kind == StoreKind::HttpCache)
        .unwrap();
    assert_eq!(http.format, Format::Blockfile);
    assert!(!http.cleanable);
    assert_eq!(http.bytes, 5700);
    let js = p
        .stores
        .iter()
        .find(|s| s.kind == StoreKind::CodeCacheJs)
        .unwrap();
    assert_eq!(
        (js.format, js.cleanable, js.entries),
        (Format::Simple, true, 4)
    );
    assert!(js.other_bytes > 900, "the extension's entry has no site");

    let yt = p.sites.iter().find(|s| s.site == "youtube.com").unwrap();
    assert_eq!(yt.listed_bytes, 5000);
    assert_eq!(yt.cleanable_entries, 3, "{yt:?}");
    assert_eq!(
        yt.stores,
        [
            StoreKind::HttpCache,
            StoreKind::CodeCacheJs,
            StoreKind::CodeCacheWasm,
            StoreKind::CacheStorage
        ]
    );
    assert!(p.sites.iter().any(|s| s.site == "m.youtube.com"));
    let ex = p.sites.iter().find(|s| s.site == "example.com").unwrap();
    assert_eq!(ex.listed_bytes, 700);
    assert!(
        !p.sites
            .iter()
            .any(|s| s.site.contains("chrome-extension") || s.site == "abcdef")
    );
    // largest first
    assert!(p.sites.windows(2).all(|w| w[0].bytes >= w[1].bytes));

    let f = &r.browsers[1].profiles[0];
    let yt = f.sites.iter().find(|s| s.site == "youtube.com").unwrap();
    assert_eq!(
        yt.cleanable_entries, 2,
        "the partitioned i.ytimg.com entry counts for youtube.com"
    );
    assert!(f.sites.iter().any(|s| s.site == "mozilla.org"));

    let only = scan(
        &env,
        &ScanOptions {
            browser: Some("FIREFOX".into()),
            ..ScanOptions::default()
        },
    );
    assert_eq!(only.browsers.len(), 1);
}

#[test]
fn plan_takes_whole_entries_of_the_site_only() {
    let (_root, env, chrome, _) = machine();
    let p = plan(&env, &[target("chrome", "youtube.com", false)], 1000);
    assert!(p.running.is_empty() && p.refused.is_empty(), "{p:?}");
    assert_eq!(p.entries, 3);
    assert_eq!(p.listed_only, 5000);
    assert_eq!(p.index_files, 2, "js and wasm indexes are rebuilt");
    let kinds: Vec<StoreKind> = p.by_store.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [
            StoreKind::CodeCacheJs,
            StoreKind::CodeCacheWasm,
            StoreKind::CacheStorage
        ]
    );
    let profile = Profile {
        browser: browser("chrome").unwrap(),
        name: "Default".into(),
        local: chrome.clone(),
        roaming: chrome.clone(),
    };
    for i in &p.plan.items {
        assert!(
            cache_path_allowed(&profile, &i.path),
            "{}",
            i.path.display()
        );
        assert!(i.selected && i.mode == DeleteMode::Quarantine);
        assert_eq!(i.rule_id.as_deref(), Some("browser.chrome.cache"));
        assert_eq!(i.requires_closed, ["chrome.exe"]);
        let text = i.path.to_string_lossy().to_lowercase();
        assert!(!text.contains("example.com") && !text.contains("cache_data"));
    }
    // m.youtube.com only with subdomains (its entry has two files: both go)
    let with_sub = plan(&env, &[target("chrome", "www.youtube.com", true)], 1000);
    assert_eq!(with_sub.entries, 4);
    assert_eq!(with_sub.plan.items.len(), p.plan.items.len() + 2);
    // one profile only
    let other = Target {
        profile: Some(chrome.with_file_name("Profile 2")),
        ..target("chrome", "youtube.com", false)
    };
    assert_eq!(plan(&env, &[other], 1000).entries, 0);
    // unknown browser or site: nothing
    assert!(
        plan(&env, &[target("netscape", "youtube.com", false)], 1000)
            .plan
            .items
            .is_empty()
    );
    assert!(
        plan(&env, &[target("chrome", "nothing.example", true)], 1000)
            .plan
            .items
            .is_empty()
    );
}

#[test]
fn nothing_is_planned_while_the_browser_runs() {
    let (root, _, _, _) = machine();
    std::fs::write(root.path().join("running.txt"), "Chrome.exe\n").unwrap();
    let env = Env::fixture(root.path());
    let r = scan(&env, &ScanOptions::default());
    assert!(r.browsers[0].running && !r.browsers[1].running);
    let p = plan(
        &env,
        &[
            target("chrome", "youtube.com", false),
            target("firefox", "youtube.com", false),
        ],
        1000,
    );
    assert_eq!(p.running, ["chrome.exe"]);
    assert!(
        p.plan
            .items
            .iter()
            .all(|i| i.rule_id.as_deref() == Some("browser.firefox.cache"))
    );
    assert_eq!(p.entries, 2);
}

#[test]
fn allowlist_never_reaches_profile_data() {
    let b = browser("chrome").unwrap();
    let base = PathBuf::from(if cfg!(windows) {
        r"C:\U\Chrome\User Data\Default"
    } else {
        "/u/Chrome/User Data/Default"
    });
    let p = Profile {
        browser: b,
        name: "Default".into(),
        local: base.clone(),
        roaming: base.clone(),
    };
    for ok in [
        "Code Cache/js/0123456789abcdef_0",
        "Code Cache/wasm/index-dir/the-real-index",
        "Service Worker/CacheStorage/abc",
        "Cache/Cache_Data/0123456789abcdef_0",
    ] {
        assert!(cache_path_allowed(&p, &base.join(ok)), "{ok}");
    }
    for bad in [
        "Cookies",
        "Login Data",
        "History",
        "Local Storage/leveldb/000003.log",
        "IndexedDB/x",
        "Extensions/abc",
        "Service Worker/Database/000003.log",
        "Service Worker/CacheStorage",
        "Code Cache/js",
        "Code Cache/js/../../Cookies",
        "Network/Cookies",
    ] {
        assert!(!cache_path_allowed(&p, &base.join(bad)), "{bad}");
    }
    let ff = Profile {
        browser: browser("firefox").unwrap(),
        name: "x".into(),
        local: base.clone(),
        roaming: base.clone(),
    };
    assert!(cache_path_allowed(&ff, &base.join("cache2").join("index")));
    assert!(cache_path_allowed(&ff, &base.join("cache2/entries/0123")));
    assert!(!cache_path_allowed(&ff, &base.join("cookies.sqlite")));
    assert!(!cache_path_allowed(&ff, &base.join("cache2/doomed/1")));
}

fn exec_opts(dir: &Path) -> ExecOptions {
    ExecOptions {
        op_id: "20261017-000000-b0b0".into(),
        kind: "browser-site".into(),
        owner: 0,
        roots: QuarantineRoots {
            system: Some(dir.join("q")),
            other_volumes: false,
        },
        now: exec::unix_now(),
    }
}

fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            } else {
                out.push((e.path(), std::fs::read(e.path()).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn cleaning_quarantines_the_site_and_restore_puts_it_back() {
    let (root, env, chrome, ff) = machine();
    let before_chrome = snapshot(&chrome);
    let before_ff = snapshot(&ff);
    let p = plan(
        &env,
        &[
            target("chrome", "youtube.com", false),
            target("firefox", "youtube.com", false),
        ],
        1000,
    );
    let gate = SafetyGate::new(&|v: &str| env.resolve(v));
    let op = exec::execute(
        &p.plan,
        &gate,
        &LiveProbe,
        &exec_opts(root.path()),
        &mut |_, _, _| true,
    );
    assert!(
        op.items.iter().all(|i| i.status == ItemStatus::Done),
        "{op:#?}"
    );

    // the site is gone, everything else is byte for byte as it was
    let after = snapshot(&chrome);
    let gone: Vec<&PathBuf> = before_chrome
        .iter()
        .filter(|(p, _)| !after.iter().any(|(a, _)| a == p))
        .map(|(p, _)| p)
        .collect();
    assert!(!gone.is_empty());
    for (path, bytes) in &after {
        let was = before_chrome.iter().find(|(p, _)| p == path).unwrap();
        assert_eq!(&was.1, bytes, "{}", path.display());
    }
    for keep in [
        "Cookies",
        "Login Data",
        "History",
        "Preferences",
        "Cache/Cache_Data/index",
        "Cache/Cache_Data/data_1",
    ] {
        assert!(chrome.join(keep).exists(), "{keep}");
    }
    assert!(chrome.join("Local Storage/leveldb/000003.log").exists());
    assert!(
        chrome
            .join("IndexedDB/https_www.youtube.com_0.indexeddb.leveldb/000003.log")
            .exists()
    );
    assert!(
        !chrome
            .join("Service Worker/CacheStorage/379f1cbab5b08b6fc9e08681e42d8be311441c88")
            .exists()
    );
    assert!(
        !chrome
            .join("Code Cache/js/index-dir/the-real-index")
            .exists()
    );
    let again = scan(&env, &ScanOptions::default());
    let prof = &again.browsers[0].profiles[0];
    let yt = prof.sites.iter().find(|s| s.site == "youtube.com").unwrap();
    assert_eq!((yt.cleanable_bytes, yt.listed_bytes), (0, 5000));
    assert!(
        prof.sites
            .iter()
            .any(|s| s.site == "example.com" && s.cleanable_bytes > 0)
    );
    let fsites = &again.browsers[1].profiles[0].sites;
    assert!(!fsites.iter().any(|s| s.site == "youtube.com"));
    assert!(fsites.iter().any(|s| s.site == "mozilla.org"));
    assert!(
        !ff.join("cache2").join("index").exists(),
        "Firefox rebuilds its index"
    );

    // Restore: every item back where it was
    for i in &op.items {
        let (status, msg) = exec::restore(&i.path, i.stored.as_deref(), i.mode, op.started_at);
        assert_eq!(
            status,
            RestoreStatus::Restored,
            "{} {msg:?}",
            i.path.display()
        );
    }
    assert_eq!(snapshot(&chrome), before_chrome);
    assert_eq!(snapshot(&ff), before_ff);
}
