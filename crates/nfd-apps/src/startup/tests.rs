use super::*;

#[test]
fn approved_flags_like_task_manager() {
    // never touched by Task Manager, or turned back on
    assert_eq!(
        parse_approved(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        Some((true, None))
    );
    assert_eq!(
        parse_approved(&[6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        Some((true, None))
    );
    // turned off on 2020-01-01T00:00:00Z
    let ft: u64 = (1_577_836_800 + 11_644_473_600) * 10_000_000;
    let mut off = vec![3, 0, 0, 0];
    off.extend(ft.to_le_bytes());
    assert_eq!(parse_approved(&off), Some((false, Some(1_577_836_800))));
    off[0] = 7;
    assert_eq!(parse_approved(&off), Some((false, Some(1_577_836_800))));
    // off without a time
    assert_eq!(parse_approved(&[3, 0, 0, 0]), Some((false, None)));
    // too short to mean anything: counts as on
    assert_eq!(parse_approved(&[3]), None);
    assert_eq!(parse_approved(&[]), None);
}

#[test]
fn written_flags_read_back() {
    let on = approved_bytes(true, 1_700_000_000);
    assert_eq!(on, vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(parse_approved(&on), Some((true, None)));
    let off = approved_bytes(false, 1_700_000_000);
    assert_eq!(off.len(), 12);
    assert_eq!(off[0], 3);
    assert_eq!(parse_approved(&off), Some((false, Some(1_700_000_000))));
}

/// A shell link with LinkInfo (ANSI or Unicode base path), arguments and,
/// optionally, an environment-variable target block.
fn lnk(target: &str, unicode_info: bool, args: Option<&str>, env_target: Option<&str>) -> Vec<u8> {
    let mut b = vec![0u8; 0x4C];
    b[0] = 0x4C;
    let mut flags: u32 = 0x2 | 0x80;
    if args.is_some() {
        flags |= 0x20;
    }
    if env_target.is_some() {
        flags |= 0x200;
    }
    b[20..24].copy_from_slice(&flags.to_le_bytes());
    // LinkInfo
    let header: u32 = if unicode_info { 0x24 } else { 0x1C };
    let volume_at = header;
    let volume = {
        let mut v = vec![0u8; 0x10];
        v[0..4].copy_from_slice(&0x10u32.to_le_bytes());
        v
    };
    let base_at = volume_at + volume.len() as u32;
    let base_ansi: Vec<u8> = target.bytes().chain([0]).collect();
    let suffix_at = base_at + base_ansi.len() as u32;
    let mut body = Vec::new();
    body.extend(&volume);
    body.extend(&base_ansi);
    body.push(0); // empty suffix
    let mut uni_base_at = 0;
    let mut uni_suffix_at = 0;
    if unicode_info {
        uni_base_at = header + body.len() as u32;
        body.extend(target.encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
        uni_suffix_at = header + body.len() as u32;
        body.extend([0, 0]);
    }
    let size = header + body.len() as u32;
    let mut li = Vec::new();
    li.extend(size.to_le_bytes());
    li.extend(header.to_le_bytes());
    li.extend(1u32.to_le_bytes()); // VolumeIDAndLocalBasePath
    li.extend(volume_at.to_le_bytes());
    li.extend(base_at.to_le_bytes());
    li.extend(0u32.to_le_bytes()); // no network part
    li.extend(suffix_at.to_le_bytes());
    if unicode_info {
        li.extend(uni_base_at.to_le_bytes());
        li.extend(uni_suffix_at.to_le_bytes());
    }
    li.extend(body);
    b.extend(li);
    if let Some(a) = args {
        let units: Vec<u16> = a.encode_utf16().collect();
        b.extend((units.len() as u16).to_le_bytes());
        b.extend(units.iter().flat_map(|u| u.to_le_bytes()));
    }
    if let Some(t) = env_target {
        let mut block = vec![0u8; 0x314];
        block[0..4].copy_from_slice(&0x314u32.to_le_bytes());
        block[4..8].copy_from_slice(&0xA000_0001u32.to_le_bytes());
        for (i, c) in t.bytes().enumerate() {
            block[8 + i] = c;
        }
        for (i, u) in t.encode_utf16().enumerate() {
            block[8 + 260 + i * 2..8 + 260 + i * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
        b.extend(block);
    }
    b.extend(0u32.to_le_bytes()); // terminal block
    b
}

#[test]
fn shortcuts_name_their_target_and_arguments() {
    let s = parse_lnk(&lnk(r"C:\Tools\app.exe", false, Some("--tray"), None)).expect("lnk");
    assert_eq!(s.target.as_deref(), Some(r"C:\Tools\app.exe"));
    assert_eq!(s.args.as_deref(), Some("--tray"));
    assert_eq!(s.env_target, None);

    let s = parse_lnk(&lnk(r"C:\Đồ dùng\ứng dụng.exe", true, None, None)).expect("unicode");
    assert_eq!(s.target.as_deref(), Some(r"C:\Đồ dùng\ứng dụng.exe"));
    assert_eq!(s.args, None);

    let s = parse_lnk(&lnk(
        r"C:\Program Files\Acme\acme.exe",
        false,
        None,
        Some(r"%ProgramFiles%\Acme\acme.exe"),
    ))
    .expect("env block");
    assert_eq!(
        s.env_target.as_deref(),
        Some(r"%ProgramFiles%\Acme\acme.exe")
    );

    assert_eq!(parse_lnk(b"not a link"), None);
    assert_eq!(parse_lnk(&[]), None);
    // cut short: no panic, no answer
    let full = lnk(r"C:\Tools\app.exe", true, Some("-x"), None);
    for n in 0..full.len() {
        let _ = parse_lnk(&full[..n]);
    }
}

#[test]
fn origins_map_to_task_managers_keys() {
    assert_eq!(Origin::UserRun.approved_key(), format!(r"{APPROVED}\Run"));
    assert_eq!(
        Origin::MachineRun32.approved_key(),
        format!(r"{APPROVED}\Run32")
    );
    assert_eq!(
        Origin::CommonFolder.approved_key(),
        format!(r"{APPROVED}\StartupFolder")
    );
    assert_eq!(Origin::UserFolder.hive(), Hive::CurrentUser);
    assert_eq!(Origin::MachineRun.hive(), Hive::LocalMachine);
    assert_eq!(Origin::MachineRun32.run_key(), Some(RUN32));
    assert_eq!(Origin::UserFolder.run_key(), None);
}

// Windows paths: like the Programs view, these run on Windows only.
#[cfg(windows)]
mod windows {
    use super::*;
    use crate::registry::MemRegistry;

    const SYS: &str = r"C:\Windows";

    fn var(v: &str) -> Option<String> {
        match v.to_ascii_lowercase().as_str() {
            "programfiles" => Some(r"C:\Program Files".into()),
            "systemroot" | "windir" => Some(SYS.into()),
            "localappdata" => Some(r"C:\Users\nano\AppData\Local".into()),
            _ => None,
        }
    }

    fn on_disk(p: &Path) -> bool {
        let s = p.to_string_lossy().to_ascii_lowercase();
        [
            r"c:\program files\acme tool\acme.exe",
            r"c:\windows\system32\rundll32.exe",
            r"c:\windows\system32\securityhealthsystray.exe",
            r"c:\users\nano\appdata\local\discord\update.exe",
            r"c:\tools\app.exe",
        ]
        .contains(&s.as_str())
    }

    fn exe(raw: &str) -> Option<String> {
        resolve_exe(raw, &var, &on_disk, Path::new(SYS)).map(|p| p.to_string_lossy().into_owned())
    }

    #[test]
    fn command_lines_resolve_to_the_program() {
        // quoted, with arguments
        assert_eq!(
            exe(r#""C:\Program Files\Acme Tool\acme.exe" --minimized"#).as_deref(),
            Some(r"C:\Program Files\Acme Tool\acme.exe")
        );
        // unquoted path with spaces
        assert_eq!(
            exe(r"C:\Program Files\Acme Tool\acme.exe /background").as_deref(),
            Some(r"C:\Program Files\Acme Tool\acme.exe")
        );
        // variables
        assert_eq!(
            exe(r#""%LOCALAPPDATA%\Discord\Update.exe" --processStart Discord.exe"#).as_deref(),
            Some(r"C:\Users\nano\AppData\Local\Discord\Update.exe")
        );
        // rundll32 runs a DLL: that DLL is the program
        assert_eq!(
            exe(r#"rundll32.exe "C:\Program Files\Acme Tool\hook.dll",Start"#).as_deref(),
            Some(r"C:\Program Files\Acme Tool\hook.dll")
        );
        assert_eq!(
            exe(r"%SystemRoot%\system32\RunDll32.exe %ProgramFiles%\Acme\x.dll,Run /q").as_deref(),
            Some(r"C:\Program Files\Acme\x.dll")
        );
        assert_eq!(
            exe("rundll32 shell32.dll,Control_RunDLL").as_deref(),
            Some(r"C:\Windows\System32\shell32.dll")
        );
        assert_eq!(exe("   "), None);
    }

    #[test]
    fn windows_and_security_programs_need_care() {
        let root = Path::new(SYS);
        assert_eq!(
            caution(
                Path::new(r"C:\Windows\System32\SecurityHealthSystray.exe"),
                root
            ),
            Some(Caution::Security)
        );
        assert_eq!(
            caution(
                Path::new(r"C:\ProgramData\Microsoft\Windows Defender\Platform\4.18\MSASCuiL.exe"),
                root
            ),
            Some(Caution::Security)
        );
        assert_eq!(
            caution(
                Path::new(r"C:\Program Files\Bitdefender\Agent\agent.exe"),
                root
            ),
            Some(Caution::Security)
        );
        assert_eq!(
            caution(Path::new(r"c:\windows\system32\ctfmon.exe"), root),
            Some(Caution::Windows)
        );
        // `C:\WindowsApps` is not under `C:\Windows`
        assert_eq!(caution(Path::new(r"C:\WindowsApps\x.exe"), root), None);
        assert_eq!(
            caution(Path::new(r"C:\Program Files\Acme Tool\acme.exe"), root),
            None
        );
    }

    fn approved(enabled: bool) -> Value {
        Value::Binary(approved_bytes(enabled, 1_700_000_000))
    }

    #[test]
    fn lists_every_source_with_its_state() {
        let mut reg = MemRegistry::new();
        reg.sz(
            Hive::CurrentUser,
            RUN,
            "Discord",
            r#""%LOCALAPPDATA%\Discord\Update.exe" --processStart Discord.exe"#,
        )
        .sz(Hive::CurrentUser, RUN, "Gone", r"C:\Gone\gone.exe")
        .sz(Hive::CurrentUser, RUN, "", "ignored default value")
        .sz(
            Hive::LocalMachine,
            RUN,
            "SecurityHealth",
            r"%windir%\system32\SecurityHealthSystray.exe",
        )
        .sz(
            Hive::LocalMachine,
            RUN32,
            "Acme",
            r#""C:\Program Files\Acme Tool\acme.exe" /tray"#,
        )
        .set(
            Hive::CurrentUser,
            &Origin::UserRun.approved_key(),
            "Discord",
            approved(false),
        )
        .set(
            Hive::LocalMachine,
            &Origin::MachineRun32.approved_key(),
            "Acme",
            approved(true),
        )
        .set(
            Hive::CurrentUser,
            &Origin::UserFolder.approved_key(),
            "Tool.lnk",
            approved(false),
        );
        let user_folder = PathBuf::from(
            r"C:\Users\nano\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup",
        );
        let common_folder =
            PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Startup");
        let uf = user_folder.clone();
        let list_dir = move |d: &Path| -> Vec<PathBuf> {
            if d == uf {
                vec![
                    uf.join("desktop.ini"),
                    uf.join("Tool.lnk"),
                    uf.join("notes.bat"),
                ]
            } else {
                Vec::new()
            }
        };
        let link = lnk(r"C:\Tools\app.exe", true, Some("--tray"), None);
        let read =
            move |p: &Path| -> Option<Vec<u8>> { p.ends_with("Tool.lnk").then(|| link.clone()) };
        let env = Env {
            reg: &reg,
            var: &var,
            exists: &on_disk,
            list_dir: &list_dir,
            read: &read,
            system_root: PathBuf::from(SYS),
            user_folder: Some(user_folder.clone()),
            common_folder: Some(common_folder),
        };
        let all = list(&env);
        let by = |id: &str| {
            all.iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("{id}: {all:#?}"))
        };
        assert_eq!(all.len(), 6, "{all:#?}");

        let discord = by("user-run|Discord");
        assert!(!discord.enabled);
        assert_eq!(discord.disabled_at, Some(1_700_000_000));
        assert_eq!(discord.scope, Scope::User);
        assert_eq!(discord.location, format!(r"HKEY_CURRENT_USER\{RUN}"));
        assert!(!discord.missing);

        let gone = by("user-run|Gone");
        assert!(gone.enabled && gone.missing);

        let sec = by("machine-run|SecurityHealth");
        assert!(sec.enabled);
        assert_eq!(sec.caution, Some(Caution::Security));
        assert_eq!(sec.scope, Scope::Machine);

        let acme = by("machine-run32|Acme");
        assert!(acme.enabled);
        assert_eq!(
            acme.exe.as_deref(),
            Some(Path::new(r"C:\Program Files\Acme Tool\acme.exe"))
        );

        let tool = by("user-folder|Tool.lnk");
        assert_eq!(tool.name, "Tool");
        assert!(!tool.enabled);
        assert_eq!(tool.exe.as_deref(), Some(Path::new(r"C:\Tools\app.exe")));
        assert_eq!(tool.command, r#""C:\Tools\app.exe" --tray"#);
        assert_eq!(tool.approved_name, "Tool.lnk");

        let bat = by("user-folder|notes.bat");
        assert!(bat.enabled && bat.missing);

        // this user's entries are written to HKCU; all users' are refused
        let (hive, key, name, value) = flag_write(discord, true, 0).expect("user entry");
        assert_eq!(
            (hive, key.as_str(), name.as_str()),
            (
                Hive::CurrentUser,
                Origin::UserRun.approved_key().as_str(),
                "Discord"
            )
        );
        assert_eq!(value, Value::Binary(approved_bytes(true, 0)));
        assert_eq!(flag_write(sec, false, 0), Err(SetError::NeedsAdmin));
        assert_eq!(flag_write(acme, false, 0), Err(SetError::NeedsAdmin));

        // turning it on in the registry shows up on the next listing
        reg.set(Hive::CurrentUser, &key, &name, value);
        let env = Env {
            reg: &reg,
            var: &var,
            exists: &on_disk,
            list_dir: &list_dir,
            read: &read,
            system_root: PathBuf::from(SYS),
            user_folder: Some(user_folder),
            common_folder: None,
        };
        assert!(
            list(&env)
                .iter()
                .any(|e| e.id == "user-run|Discord" && e.enabled)
        );
    }
}
