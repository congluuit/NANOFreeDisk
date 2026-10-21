//! Software junk on a fixture AppData.

use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::plan::DeleteMode;
use nfd_core::safety::SafetyGate;

use super::*;
use crate::inventory::Program;

struct Fixture {
    dir: tempfile::TempDir,
    roots: JunkRoots,
    gate: SafetyGate,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tmp");
        let b = dir.path().to_path_buf();
        for d in ["Roaming", "Local/Programs", "LocalLow", "win"] {
            std::fs::create_dir_all(b.join(d)).expect("mkdir");
        }
        let resolve = |v: &str| -> Option<String> {
            let p = |s: &str| Some(b.join(s).to_string_lossy().into_owned());
            match v {
                "APPDATA" => p("Roaming"),
                "LOCALAPPDATA" => p("Local"),
                "SYSTEMROOT" => p("win"),
                _ => None,
            }
        };
        let mut roots = JunkRoots::detect(&resolve, &[b.join("Local/io.nanofreedisk")]);
        roots.app_data.push(b.join("LocalLow"));
        let gate = SafetyGate::new(&resolve);
        Self { dir, roots, gate }
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn file(&self, rel: &str, bytes: usize) {
        let p = self.p(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(p, vec![1u8; bytes]).expect("write");
    }

    fn scan(&self, programs: &[Program], running: &[PathBuf]) -> JunkReport {
        let ctx = JunkCtx {
            gate: &self.gate,
            covered: &|p: &Path| p.to_string_lossy().contains("CoveredApp"),
            running,
            max_entries: 100_000,
            depth: 5,
            cancel: None,
        };
        scan(programs, &self.roots, &ctx)
    }
}

fn discord(f: &Fixture) -> Program {
    Program {
        id: "hkcu:Discord".into(),
        name: "Discord".into(),
        publisher: Some("Discord Inc.".into()),
        version: None,
        source: crate::Source::Win32,
        scope: crate::Scope::User,
        arch: None,
        install_location: Some(f.p("Local/Discord")),
        uninstall: None,
        quiet_uninstall: None,
        icon: Some(f.p("Local/Discord/app-1.0.9036/Discord.exe")),
        icon_index: 0,
        logo: None,
        installed_at: None,
        size: 0,
        size_measured: false,
        product_code: None,
        key: None,
        package: None,
        url: None,
        caution: None,
        blocked: None,
        broken: false,
    }
}

fn item<'a>(r: &'a JunkReport, path: &Path) -> Option<&'a JunkItem> {
    r.groups
        .iter()
        .flat_map(|g| &g.items)
        .find(|i| crate::inventory::same(&i.path, path))
}

fn electron(f: &Fixture, base: &str) {
    f.file(&format!("{base}/Preferences"), 10);
    f.file(&format!("{base}/Cache/Cache_Data/f_000001"), 4000);
    f.file(&format!("{base}/Code Cache/js/index"), 3000);
    f.file(&format!("{base}/GPUCache/data_0"), 2000);
    f.file(&format!("{base}/Local Storage/leveldb/000003.log"), 100);
    f.file(&format!("{base}/IndexedDB/https_x/1.ldb"), 100);
    f.file(&format!("{base}/logs/main.log"), 500);
    f.file(&format!("{base}/Crashpad/reports/a.dmp"), 700);
    f.file(&format!("{base}/Crashpad/settings.dat"), 10);
    f.file(&format!("{base}/Service Worker/CacheStorage/x"), 300);
    f.file(&format!("{base}/Service Worker/Database/x"), 30);
}

#[test]
fn electron_caches_crashes_and_logs() {
    let f = Fixture::new();
    electron(&f, "Roaming/discord");
    let r = f.scan(&[discord(&f)], &[]);
    let g = r
        .groups
        .iter()
        .find(|g| g.program.as_deref() == Some("hkcu:Discord"))
        .expect("group");
    assert_eq!(g.name, "Discord");
    assert_eq!(g.requires_closed, vec!["discord.exe".to_owned()]);
    assert!(!g.running);
    for (rel, what, level) in [
        ("Roaming/discord/Cache", "chromium-cache", RiskLevel::Safe),
        (
            "Roaming/discord/Code Cache",
            "chromium-cache",
            RiskLevel::Safe,
        ),
        (
            "Roaming/discord/GPUCache",
            "chromium-cache",
            RiskLevel::Safe,
        ),
        ("Roaming/discord/logs", "electron-logs", RiskLevel::Safe),
        (
            "Roaming/discord/Crashpad/reports",
            "crashpad",
            RiskLevel::Safe,
        ),
        (
            "Roaming/discord/Service Worker/CacheStorage",
            "service-worker",
            RiskLevel::Caution,
        ),
    ] {
        let it = item(&r, &f.p(rel)).unwrap_or_else(|| panic!("{rel} missing: {r:#?}"));
        assert_eq!((it.what.as_str(), it.level), (what, level), "{rel}");
        assert_eq!(it.selected, level == RiskLevel::Safe, "{rel}");
    }
    for never in [
        "Roaming/discord/Local Storage",
        "Roaming/discord/IndexedDB",
        "Roaming/discord/Service Worker/Database",
        "Roaming/discord/Crashpad",
        "Roaming/discord",
    ] {
        assert!(item(&r, &f.p(never)).is_none(), "{never} offered");
    }
}

#[test]
fn a_cache_folder_outside_chromium_is_not_junk() {
    let f = Fixture::new();
    f.file("Roaming/Photos/Cache/thumbs.db", 100);
    f.file("Roaming/Photos/library.db", 100);
    let r = f.scan(&[], &[]);
    assert!(item(&r, &f.p("Roaming/Photos/Cache")).is_none());
}

#[test]
fn squirrel_keeps_the_newest_version_and_package() {
    let f = Fixture::new();
    f.file("Local/Discord/Update.exe", 10);
    f.file("Local/Discord/app-1.0.9035/Discord.exe", 900);
    f.file("Local/Discord/app-1.0.9036/Discord.exe", 900);
    f.file("Local/Discord/packages/Discord-1.0.9035-full.nupkg", 800);
    f.file("Local/Discord/packages/Discord-1.0.9036-full.nupkg", 800);
    f.file("Local/Discord/packages/Discord-1.0.9036-delta.nupkg", 80);
    f.file("Local/Discord/packages/SquirrelTemp/tmp.bin", 50);
    let r = f.scan(&[discord(&f)], &[]);
    let old = item(&r, &f.p("Local/Discord/app-1.0.9035")).expect("old version");
    assert_eq!(
        (old.what.as_str(), old.level, old.selected),
        ("old-version", RiskLevel::Caution, false)
    );
    assert!(item(&r, &f.p("Local/Discord/app-1.0.9036")).is_none());
    assert!(
        item(
            &r,
            &f.p("Local/Discord/packages/Discord-1.0.9035-full.nupkg")
        )
        .expect("old full")
        .selected
    );
    assert!(
        item(
            &r,
            &f.p("Local/Discord/packages/Discord-1.0.9036-delta.nupkg")
        )
        .expect("delta")
        .selected
    );
    assert!(
        item(
            &r,
            &f.p("Local/Discord/packages/Discord-1.0.9036-full.nupkg")
        )
        .is_none()
    );
    assert!(
        item(&r, &f.p("Local/Discord/packages/SquirrelTemp"))
            .expect("temp")
            .selected
    );
    // the running version is never offered
    let running = vec![f.p("Local/Discord/app-1.0.9035/Discord.exe")];
    let r = f.scan(&[discord(&f)], &running);
    assert_eq!(
        item(&r, &f.p("Local/Discord/app-1.0.9035"))
            .expect("old")
            .blocked
            .as_deref(),
        Some("in-use")
    );
    assert!(r.groups[0].running);
}

#[test]
fn updaters_logs_temp_and_dumps() {
    let f = Fixture::new();
    f.file("Local/acme-updater/pending/acme-setup-2.0.exe", 5000);
    f.file("Roaming/HexChat/logs/freenode.log", 400);
    f.file("Roaming/Foo/crash.dmp", 300);
    f.file("Roaming/Foo/temp/x.tmp", 30);
    let r = f.scan(&[], &[]);
    let up = item(&r, &f.p("Local/acme-updater")).expect("updater");
    assert_eq!((up.what.as_str(), up.selected), ("pending-update", true));
    let logs = item(&r, &f.p("Roaming/HexChat/logs")).expect("logs");
    assert_eq!(
        (logs.what.as_str(), logs.level, logs.selected),
        ("logs", RiskLevel::Caution, false)
    );
    assert!(
        item(&r, &f.p("Roaming/Foo/crash.dmp"))
            .expect("dump")
            .selected
    );
    let tmp = item(&r, &f.p("Roaming/Foo/temp")).expect("temp");
    assert_eq!((tmp.level, tmp.selected), (RiskLevel::Caution, false));
    let foo = r
        .groups
        .iter()
        .find(|g| g.name == "Foo")
        .expect("group by folder");
    assert!(foo.program.is_none() && foo.key.starts_with("folder:"));
}

#[test]
fn covered_history_and_own_data_are_left_out() {
    let f = Fixture::new();
    electron(&f, "Roaming/CoveredApp");
    electron(&f, "Local/io.nanofreedisk/EBWebView/Default");
    f.file("Roaming/Agent/logs/.aider.chat.history.md", 100);
    let r = f.scan(&[], &[]);
    assert!(
        r.groups.iter().all(|g| !g.name.contains("CoveredApp")),
        "covered by a rule pack"
    );
    assert!(
        r.groups.iter().all(|g| g.name != "io.nanofreedisk"),
        "own data"
    );
    let logs = item(&r, &f.p("Roaming/Agent/logs")).expect("logs");
    assert_eq!(logs.blocked.as_deref(), Some("contains-history"));
    assert!(!logs.selected);
}

#[test]
fn plans_quarantine_with_the_apps_to_close() {
    let f = Fixture::new();
    electron(&f, "Roaming/discord");
    let r = f.scan(&[discord(&f)], &[]);
    let picked = r.preselected();
    assert!(!picked.is_empty());
    let p = plan(&r, &picked);
    assert_eq!(p.items.len(), picked.len());
    assert!(
        p.items
            .iter()
            .all(|i| i.mode == DeleteMode::Quarantine && i.selected)
    );
    assert!(
        p.items
            .iter()
            .all(|i| i.requires_closed == vec!["discord.exe".to_owned()])
    );
    assert!(p.items.iter().all(|i| {
        i.rule_id
            .as_deref()
            .is_some_and(|r| r.starts_with("software."))
    }));
    // a blocked or unknown path never gets in
    let none = plan(&r, &[f.p("Roaming/discord/Local Storage")]);
    assert!(none.items.is_empty());
}

#[test]
fn versions_parse() {
    assert_eq!(squirrel_version("app-1.0.9036"), Some(vec![1, 0, 9036]));
    assert_eq!(
        squirrel_version("app-4.41.105-beta"),
        Some(vec![4, 41, 105])
    );
    assert_eq!(squirrel_version("application"), None);
    assert_eq!(
        nupkg_version("Discord-1.0.9036-full.nupkg"),
        Some((vec![1, 0, 9036], true))
    );
    assert_eq!(
        nupkg_version("slack-4.41.105-delta.nupkg"),
        Some((vec![4, 41, 105], false))
    );
    assert_eq!(nupkg_version("RELEASES"), None);
}
