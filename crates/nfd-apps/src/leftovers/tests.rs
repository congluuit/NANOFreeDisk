//! Leftover scan on a fixture: a fake machine laid out in a temporary folder
//! (Program Files, ProgramData, AppData, Start Menu, Desktop) and an in-memory
//! registry.

use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::safety::SafetyGate;

use super::*;
use crate::command::Command;
use crate::inventory::Program;
use crate::registry::{Hive, MemRegistry, Value};

struct Machine {
    dir: tempfile::TempDir,
    roots: Roots,
    gate: SafetyGate,
    reg: MemRegistry,
}

impl Machine {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tmp");
        let b = dir.path().to_path_buf();
        for d in [
            "pf",
            "pf86",
            "pd",
            "win",
            "home/AppData/Roaming",
            "home/AppData/Local/Programs",
            "home/AppData/Local/Packages",
            "home/AppData/LocalLow",
            "home/Desktop",
            "home/AppData/Roaming/Microsoft/Windows/Start Menu/Programs",
            "pd/Microsoft/Windows/Start Menu/Programs",
            "public/Desktop",
        ] {
            std::fs::create_dir_all(b.join(d)).expect("mkdir");
        }
        let resolve = |v: &str| -> Option<String> {
            let p = |s: &str| Some(b.join(s).to_string_lossy().into_owned());
            match v {
                "PROGRAMFILES" => p("pf"),
                "PROGRAMFILES_X86" => p("pf86"),
                "PROGRAMDATA" => p("pd"),
                "SYSTEMROOT" => p("win"),
                "APPDATA" => p("home/AppData/Roaming"),
                "LOCALAPPDATA" => p("home/AppData/Local"),
                "HOME" | "USERPROFILE" => p("home"),
                _ => None,
            }
        };
        let env =
            |v: &str| (v == "PUBLIC").then(|| b.join("public").to_string_lossy().into_owned());
        let roots = Roots::detect(&resolve, &env);
        let gate = SafetyGate::new(&resolve);
        Self {
            dir,
            roots,
            gate,
            reg: MemRegistry::new(),
        }
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn file(&self, rel: &str, bytes: usize) -> PathBuf {
        let p = self.p(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, vec![7u8; bytes]).expect("write");
        p
    }

    fn mkdir(&self, rel: &str) -> PathBuf {
        let p = self.p(rel);
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }

    /// A `.lnk` pointing at `target` (only a LinkInfo local path).
    fn link(&self, rel: &str, target: &Path) -> PathBuf {
        let mut data = vec![0u8; 0x4C];
        data[0..4].copy_from_slice(&0x4Cu32.to_le_bytes());
        data[4..20].copy_from_slice(&[
            0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x46,
        ]);
        // HasLinkInfo
        data[0x14..0x18].copy_from_slice(&2u32.to_le_bytes());
        let path = target.to_string_lossy();
        let mut base: Vec<u8> = path.as_bytes().to_vec();
        base.push(0);
        // LinkInfo: size, header size 0x1C, flags VolumeIDAndLocalBasePath,
        // VolumeID offset, LocalBasePath offset, net offset 0, suffix offset
        let volume_id: Vec<u8> = {
            let mut v = vec![0u8; 0x10];
            v[0..4].copy_from_slice(&0x11u32.to_le_bytes());
            v[12..16].copy_from_slice(&0x10u32.to_le_bytes());
            v.push(0);
            v
        };
        let header = 0x1Cu32;
        let vol_off = header;
        let base_off = vol_off + volume_id.len() as u32;
        let suffix_off = base_off + base.len() as u32;
        let size = suffix_off + 1;
        let mut info = Vec::new();
        for n in [size, header, 1, vol_off, base_off, 0, suffix_off] {
            info.extend(n.to_le_bytes());
        }
        info.extend(&volume_id);
        info.extend(&base);
        info.push(0);
        data.extend(info);
        let p = self.p(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, data).expect("write");
        p
    }

    fn scan(&self, program: &Program, others: &[Program], mode: Mode) -> Report {
        let system_root = self.p("win");
        let snap = snapshot(program, &self.roots, &system_root, 0);
        self.scan_snap(&snap, others, mode)
    }

    fn scan_snap(&self, snap: &Snapshot, others: &[Program], mode: Mode) -> Report {
        let system_root = self.p("win");
        let o = Others::new(others, &snap.program.id, &self.roots, &system_root);
        let ctx = Ctx {
            reg: &self.reg,
            roots: &self.roots,
            gate: &self.gate,
            others: &o,
            running: &[],
            max_entries: 100_000,
            exists: &|p: &Path| p.exists(),
            env: &|_| None,
            system_root,
        };
        scan(snap, &ctx, mode)
    }
}

fn program(name: &str, publisher: Option<&str>, install: Option<PathBuf>) -> Program {
    Program {
        id: format!("hklm:{name}"),
        name: name.to_owned(),
        publisher: publisher.map(str::to_owned),
        version: None,
        source: crate::Source::Win32,
        scope: crate::Scope::Machine,
        arch: None,
        uninstall: install.as_ref().map(|i| Command {
            exe: i.join("uninst.exe"),
            args: String::new(),
            raw: String::new(),
        }),
        install_location: install,
        quiet_uninstall: None,
        icon: None,
        installed_at: None,
        size: 0,
        size_measured: false,
        product_code: None,
        key: Some(format!(
            r"HKEY_LOCAL_MACHINE\{}\{name}",
            crate::inventory::UNINSTALL
        )),
        package: None,
        url: None,
        caution: None,
        blocked: None,
        broken: false,
    }
}

fn find<'a>(r: &'a Report, path: &Path) -> Option<&'a Leftover> {
    let n = crate::inventory::norm(path);
    r.items
        .iter()
        .find(|i| crate::inventory::norm(Path::new(&i.path)) == n)
}

#[test]
fn the_install_folder_left_behind_is_safe() {
    let m = Machine::new();
    let install = m.mkdir("pf/Acme Tool");
    m.file("pf/Acme Tool/settings.ini", 100);
    m.file("pf/Acme Tool/acme.exe", 1000);
    let prog = program(
        "Acme Tool 2.1",
        Some("Acme Software, Inc."),
        Some(install.clone()),
    );
    let snap = snapshot(&prog, &m.roots, &m.p("win"), 0);
    assert!(snap.keys.contains(&"acme tool".to_owned()));
    assert!(snap.exes.iter().any(|e| e.ends_with("acme.exe")));
    // the uninstaller removed the exe but left the settings
    std::fs::remove_file(install.join("acme.exe")).expect("rm");
    let r = m.scan_snap(&snap, &[], Mode::AfterUninstall);
    let it = find(&r, &install).expect("install folder");
    assert_eq!(it.reason, "install-dir");
    assert_eq!(it.level, RiskLevel::Safe);
    assert_eq!(it.scope, crate::Scope::Machine);
    assert!(it.selected && it.blocked.is_none(), "{it:?}");
    assert_eq!(it.files, 1);
    assert_eq!(
        r.items.iter().filter(|i| i.kind == Kind::Folder).count(),
        1,
        "same-name match merged"
    );
}

#[test]
fn data_folders_named_like_the_program() {
    let m = Machine::new();
    m.file("home/AppData/Roaming/Acme Tool/config.json", 10);
    m.file("home/AppData/Local/AcmeTool/cache.bin", 10);
    m.file("home/AppData/LocalLow/Acme/Acme Tool/x", 10);
    m.file("pd/Acme Tool Beta/x", 10);
    m.file("home/AppData/Roaming/Acme/other.txt", 10);
    let prog = program("Acme Tool", Some("Acme Ltd"), None);
    let r = m.scan(&prog, &[], Mode::AfterUninstall);
    let roaming = find(&r, &m.p("home/AppData/Roaming/Acme Tool")).expect("roaming");
    assert!(roaming.selected && roaming.scope == crate::Scope::User);
    assert!(
        find(&r, &m.p("home/AppData/Local/AcmeTool"))
            .expect("compact")
            .selected
    );
    assert!(
        find(&r, &m.p("home/AppData/LocalLow/Acme/Acme Tool"))
            .expect("vendor/product")
            .selected
    );
    // the LocalLow vendor folder holds only the product: offered, never pre-selected
    let vendor = find(&r, &m.p("home/AppData/LocalLow/Acme")).expect("vendor folder");
    assert_eq!(
        (vendor.reason.as_str(), vendor.level, vendor.selected),
        ("vendor-folder", RiskLevel::Caution, false)
    );
    // the Roaming vendor folder holds something else: not offered
    assert!(find(&r, &m.p("home/AppData/Roaming/Acme")).is_none());
    // a longer name is only a weak match
    let weak = find(&r, &m.p("pd/Acme Tool Beta")).expect("weak");
    assert_eq!((weak.reason.as_str(), weak.selected), ("weak-name", false));
}

#[test]
fn generic_names_never_match() {
    let m = Machine::new();
    m.file("home/AppData/Roaming/Common/x", 10);
    m.file("home/AppData/Local/Update/x", 10);
    m.file("pd/Data/x", 10);
    let prog = program("Update", Some("Common"), None);
    let r = m.scan(&prog, &[], Mode::AfterUninstall);
    assert!(r.items.is_empty(), "{:?}", r.items);
}

#[test]
fn folders_of_other_programs_are_blocked() {
    let m = Machine::new();
    let suite = m.mkdir("pf/Acme");
    m.file("pf/Acme/Tool/x.dll", 10);
    m.file("pf/Acme/Viewer/viewer.exe", 10);
    let tool = program("Acme Tool", Some("Acme"), Some(suite.join("Tool")));
    // a second program registered the whole vendor folder as its home
    let viewer = program("Acme Viewer", Some("Acme"), Some(suite.join("Viewer")));
    let mut wide = program("Acme Suite", Some("Acme"), Some(suite.clone()));
    wide.id = "hklm:suite".into();
    let r = m.scan(&tool, &[viewer, wide], Mode::AfterUninstall);
    let it = find(&r, &suite.join("Tool")).expect("tool folder");
    assert_eq!(it.blocked.as_deref(), Some("other-program"));
    assert!(!it.selected);
    // the vendor folder is never offered: other programs share the publisher
    assert!(find(&r, &suite).is_none());
}

#[test]
fn a_name_another_program_shares_is_never_preselected() {
    let m = Machine::new();
    m.file("home/AppData/Roaming/Studio/x", 10);
    let a = program("Studio", Some("Acme"), None);
    let b = program("Studio", Some("Other Corp"), None);
    let mut b = b;
    b.id = "hklm:other-studio".into();
    let r = m.scan(&a, &[b], Mode::AfterUninstall);
    let it = find(&r, &m.p("home/AppData/Roaming/Studio")).expect("studio");
    assert_eq!(it.level, RiskLevel::Caution);
    assert!(!it.selected);
    assert_eq!(it.also.as_deref(), Some("Studio"));
}

#[test]
fn windows_folders_and_roots_are_never_offered() {
    let m = Machine::new();
    m.file("pf/Common Files/Acme Tool/x", 10);
    m.file("pd/Microsoft/Acme Tool/x", 10);
    // a registration naming the whole Program Files folder
    let prog = program("Acme Tool", None, Some(m.p("pf")));
    let r = m.scan(&prog, &[], Mode::AfterUninstall);
    assert!(find(&r, &m.p("pf")).is_none());
    assert!(r.items.iter().all(|i| !i.selected || i.blocked.is_none()));
    assert!(
        find(&r, &m.p("pf/Common Files/Acme Tool")).is_none(),
        "only one level below a root"
    );
}

#[test]
fn conversation_history_inside_blocks_the_folder() {
    let m = Machine::new();
    m.file(
        "home/AppData/Roaming/Acme Tool/.codex/sessions/rollout.jsonl",
        10,
    );
    let prog = program("Acme Tool", None, None);
    let r = m.scan(&prog, &[], Mode::AfterUninstall);
    let it = find(&r, &m.p("home/AppData/Roaming/Acme Tool")).expect("folder");
    assert_eq!(it.blocked.as_deref(), Some("contains-history"));
    assert!(!it.selected);
}

#[test]
fn dead_shortcuts_into_the_program() {
    let m = Machine::new();
    let install = m.p("pf/Acme Tool");
    let exe = install.join("acme.exe");
    let prog = program("Acme Tool", None, Some(install.clone()));
    let sm = "home/AppData/Roaming/Microsoft/Windows/Start Menu/Programs";
    m.link(&format!("{sm}/Acme Tool/Acme Tool.lnk"), &exe);
    m.link(
        &format!("{sm}/Acme Tool/Uninstall.lnk"),
        &install.join("uninst.exe"),
    );
    m.link("home/Desktop/Acme.lnk", &exe);
    // a shortcut to something else that is still there
    let notes = m.file("pf/Notes/notes.exe", 10);
    m.link("home/Desktop/Notes.lnk", &notes);
    let r = m.scan(&prog, &[], Mode::AfterUninstall);
    let folder = find(&r, &m.p(&format!("{sm}/Acme Tool"))).expect("start menu folder");
    assert_eq!(folder.reason, "start-menu-folder");
    assert!(folder.selected);
    let desk = find(&r, &m.p("home/Desktop/Acme.lnk")).expect("desktop link");
    assert_eq!(desk.kind, Kind::Shortcut);
    assert!(desk.selected);
    assert!(find(&r, &m.p("home/Desktop/Notes.lnk")).is_none());
}

#[test]
fn registry_leftovers() {
    let mut m = Machine::new();
    let install = m.p("pf/Acme Tool");
    let prog = program("Acme Tool", Some("Acme Ltd"), Some(install.clone()));
    let exe = install.join("acme.exe");
    let cu = Hive::CurrentUser;
    let lm = Hive::LocalMachine;
    m.reg
        .sz(cu, r"Software\Acme\Acme Tool", "Theme", "dark")
        .sz(lm, r"SOFTWARE\WOW6432Node\AcmeTool", "Path", "x")
        .sz(lm, r"SOFTWARE\Microsoft\Acme Tool", "x", "y")
        .sz(cu, r"Software\Classes\Acme Tool", "x", "y")
        .sz(
            lm,
            &format!(r"{}\Acme Tool", crate::inventory::UNINSTALL),
            "DisplayName",
            "Acme Tool",
        )
        .sz(
            lm,
            &format!(r"{APP_PATHS}\acme.exe"),
            "",
            &exe.to_string_lossy(),
        )
        .sz(
            cu,
            RUN,
            "AcmeTray",
            &format!("\"{}\" --tray", exe.display()),
        )
        .sz(cu, RUN, "Other", r#""C:\Windows\notepad.exe""#)
        .set(
            cu,
            STARTUP_APPROVED,
            "AcmeTray",
            Value::Binary(vec![2, 0, 0, 0]),
        )
        .sz(
            lm,
            r"SYSTEM\CurrentControlSet\Services\AcmeSvc",
            "ImagePath",
            &format!("\"{}\"", install.join("svc.exe").display()),
        );
    // the snapshot is taken while the exe exists
    m.file("pf/Acme Tool/acme.exe", 10);
    let snap = snapshot(&prog, &m.roots, &m.p("win"), 0);
    std::fs::remove_dir_all(&install).expect("uninstalled");
    let r = m.scan_snap(&snap, &[], Mode::AfterUninstall);
    let ids: Vec<(&str, bool)> = r
        .items
        .iter()
        .filter(|i| matches!(i.kind, Kind::RegistryKey | Kind::RegistryValue))
        .map(|i| (i.id.as_str(), i.selected))
        .collect();
    let has = |id: &str| {
        ids.iter()
            .find(|(i, _)| i.eq_ignore_ascii_case(id))
            .map(|(_, s)| *s)
    };
    assert_eq!(
        has(r"HKEY_CURRENT_USER\Software\Acme\Acme Tool"),
        Some(true)
    );
    assert_eq!(
        has(r"HKEY_CURRENT_USER\Software\Acme"),
        Some(false),
        "vendor key: caution"
    );
    assert_eq!(
        has(r"HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\AcmeTool"),
        Some(true)
    );
    assert_eq!(
        has(&format!(
            r"HKEY_LOCAL_MACHINE\{}\Acme Tool",
            crate::inventory::UNINSTALL
        )),
        Some(true)
    );
    assert_eq!(
        has(&format!(r"HKEY_LOCAL_MACHINE\{APP_PATHS}\acme.exe")),
        Some(true)
    );
    assert_eq!(
        has(&format!(r"HKEY_CURRENT_USER\{RUN}|AcmeTray")),
        Some(true)
    );
    assert_eq!(
        has(&format!(r"HKEY_CURRENT_USER\{STARTUP_APPROVED}|AcmeTray")),
        Some(true)
    );
    assert_eq!(has(&format!(r"HKEY_CURRENT_USER\{RUN}|Other")), None);
    assert!(has(r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Acme Tool").is_none());
    assert!(has(r"HKEY_CURRENT_USER\Software\Classes\Acme Tool").is_none());
    let svc = r
        .items
        .iter()
        .find(|i| i.kind == Kind::Service)
        .expect("service");
    assert_eq!(svc.detail.as_deref(), Some("AcmeSvc"));
    assert_eq!(svc.blocked.as_deref(), Some("info"));
    assert!(!svc.selected);
    let hklm = r
        .items
        .iter()
        .find(|i| i.id.starts_with("HKEY_LOCAL_MACHINE"))
        .expect("hklm");
    assert_eq!(hklm.scope, crate::Scope::Machine);
}

#[test]
fn forced_scans_preselect_nothing_and_keep_the_entry() {
    let mut m = Machine::new();
    let install = m.mkdir("pf/Acme Tool");
    m.file("pf/Acme Tool/acme.exe", 10);
    let prog = program("Acme Tool", None, Some(install));
    m.reg.sz(
        Hive::LocalMachine,
        &format!(r"{}\Acme Tool", crate::inventory::UNINSTALL),
        "DisplayName",
        "Acme Tool",
    );
    let r = m.scan(&prog, &[], Mode::Forced);
    assert!(!r.items.is_empty());
    assert!(
        r.items
            .iter()
            .all(|i| !i.selected && i.level == RiskLevel::Caution)
    );
    assert!(r.items.iter().all(|i| i.reason != "arp-entry"));
}

#[test]
fn registry_refusals() {
    let lm = Hive::LocalMachine;
    assert_eq!(
        registry_refusal(lm, "SOFTWARE", false),
        Some("registry-protected")
    );
    assert_eq!(
        registry_refusal(lm, r"SOFTWARE\Microsoft", false),
        Some("registry-protected")
    );
    assert_eq!(
        registry_refusal(
            lm,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            false
        ),
        Some("registry-protected")
    );
    assert_eq!(
        registry_refusal(
            lm,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Acme",
            false
        ),
        None
    );
    assert_eq!(
        registry_refusal(
            lm,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Acme\Sub",
            false
        ),
        Some("registry-protected")
    );
    assert_eq!(
        registry_refusal(
            lm,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths\a.exe",
            false
        ),
        None
    );
    assert_eq!(
        registry_refusal(lm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", true),
        None
    );
    assert_eq!(
        registry_refusal(lm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", false),
        Some("registry-protected")
    );
    assert_eq!(
        registry_refusal(
            lm,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
            true
        ),
        Some("registry-protected")
    );
    assert_eq!(
        registry_refusal(lm, r"SOFTWARE\Classes\Acme", false),
        Some("registry-protected")
    );
    assert_eq!(
        registry_refusal(lm, r"SOFTWARE\Policies\Acme", false),
        Some("registry-protected")
    );
    assert_eq!(
        registry_refusal(lm, r"SOFTWARE\WOW6432Node", false),
        Some("registry-protected")
    );
    assert_eq!(
        registry_refusal(lm, r"SYSTEM\CurrentControlSet\Services\Acme", false),
        Some("registry-protected")
    );
    assert_eq!(registry_refusal(lm, r"SOFTWARE\Acme\Tool", false), None);
    assert_eq!(
        registry_refusal(lm, r"SOFTWARE\Acme", true),
        Some("registry-protected")
    );
}

#[test]
fn store_apps_leave_their_data_folder() {
    let m = Machine::new();
    m.file(
        "home/AppData/Local/Packages/Acme.Notes_abc/LocalState/db",
        10,
    );
    let mut prog = program("Notes", Some("Acme"), None);
    prog.source = crate::Source::Store;
    prog.key = None;
    prog.package = Some(crate::inventory::Package {
        full_name: "Acme.Notes_1.0.0.0_x64__abc".into(),
        family: "Acme.Notes_abc".into(),
    });
    let r = m.scan(&prog, &[], Mode::AfterUninstall);
    let it = find(&r, &m.p("home/AppData/Local/Packages/Acme.Notes_abc")).expect("data");
    assert_eq!(it.reason, "store-data");
    assert!(it.selected);
}

#[test]
fn a_product_named_like_its_vendor() {
    let m = Machine::new();
    m.file("home/AppData/Roaming/discord/Cache/x", 10);
    let prog = program("Discord", Some("Discord Inc."), None);
    let r = m.scan(&prog, &[], Mode::AfterUninstall);
    assert!(
        find(&r, &m.p("home/AppData/Roaming/discord"))
            .expect("discord")
            .selected
    );
    // another program of the vendor is installed: listed, never pre-selected
    let mut ptb = program("Discord PTB", Some("Discord Inc."), None);
    ptb.id = "hkcu:ptb".into();
    let r = m.scan(&prog, &[ptb], Mode::AfterUninstall);
    let it = find(&r, &m.p("home/AppData/Roaming/discord")).expect("discord");
    assert_eq!((it.level, it.selected), (RiskLevel::Caution, false));
}
