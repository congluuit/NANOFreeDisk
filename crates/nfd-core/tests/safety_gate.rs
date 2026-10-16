//! SafetyGate test suite (PHASE3_CLEAN §3): one test per case so a regression names
//! the exact trick that got through. Paths are written Windows-style; on other
//! platforms the drive letter `C:` is dropped and backslashes become slashes, and the
//! cases that only make sense on Windows (other drives, UNC, `\\?\`, streams, device
//! names, trailing dots) are compiled on Windows only.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::plan::{self, CleanPlan, DeleteMode, PlanItem, PlanOptions};
use nfd_core::safety::gate::{Expected, Facts};
use nfd_core::safety::{Blocked, Origin, Probe, SafetyGate, Verdict, Warning};

fn resolver(var: &str) -> Option<String> {
    let base = if cfg!(windows) { "C:" } else { "" };
    Some(match var {
        "SYSTEMROOT" => format!("{base}/Windows"),
        "SYSTEMDRIVE" => base.to_owned(),
        "HOME" | "USERPROFILE" => format!("{base}/Users/nano"),
        "APPDATA" => format!("{base}/Users/nano/AppData/Roaming"),
        "LOCALAPPDATA" => format!("{base}/Users/nano/AppData/Local"),
        "TEMP" => format!("{base}/Users/nano/AppData/Local/Temp"),
        "PROGRAMDATA" => format!("{base}/ProgramData"),
        "PROGRAMFILES" => format!("{base}/Program Files"),
        "PROGRAMFILES_X86" => format!("{base}/Program Files (x86)"),
        _ => return None,
    })
}

fn gate() -> SafetyGate {
    SafetyGate::new(&resolver)
}

/// Windows-style path → this platform's form.
fn p(path: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(path)
    } else {
        PathBuf::from(path.trim_start_matches("C:").replace('\\', "/"))
    }
}

const M: Origin = Origin::Manual;
const SAFE: Origin = Origin::Rule {
    level: RiskLevel::Safe,
};
const CAUTION: Origin = Origin::Rule {
    level: RiskLevel::Caution,
};
const ADVANCED: Origin = Origin::Rule {
    level: RiskLevel::Advanced,
};
const PROTECTED: Origin = Origin::Rule {
    level: RiskLevel::Protected,
};

/// `"ok"` or the blocked code.
fn code(origin: Origin, path: &str) -> &'static str {
    match gate().check_path(&p(path), origin) {
        Ok(()) => "ok",
        Err(b) => b.code(),
    }
}

macro_rules! cases {
    ($( $(#[$m:meta])* $name:ident: $origin:expr, $path:expr => $want:expr; )*) => {
        $(
            $(#[$m])*
            #[test]
            fn $name() {
                assert_eq!(code($origin, $path), $want, "{} ({:?})", $path, $origin);
            }
        )*
    };
}

// ---------------------------------------------------------------- manual: allowed
cases! {
    manual_download_iso: M, r"C:\Users\nano\Downloads\big.iso" => "ok";
    manual_download_folder: M, r"C:\Users\nano\Downloads\setup-files" => "ok";
    manual_document_file: M, r"C:\Users\nano\Documents\old\x.zip" => "ok";
    manual_desktop_file: M, r"C:\Users\nano\Desktop\shot.png" => "ok";
    manual_pictures_raw: M, r"C:\Users\nano\Pictures\raw\IMG_0001.CR2" => "ok";
    manual_videos_file: M, r"C:\Users\nano\Videos\capture.mp4" => "ok";
    manual_music_folder: M, r"C:\Users\nano\Music\old album" => "ok";
    manual_onedrive_file: M, r"C:\Users\nano\OneDrive\big.bin" => "ok";
    manual_temp_item: M, r"C:\Users\nano\AppData\Local\Temp\foo" => "ok";
    manual_localappdata_cache: M, r"C:\Users\nano\AppData\Local\NVIDIA\DXCache" => "ok";
    manual_roaming_app: M, r"C:\Users\nano\AppData\Roaming\OldApp" => "ok";
    manual_cargo_registry: M, r"C:\Users\nano\.cargo\registry" => "ok";
    manual_claude_debug: M, r"C:\Users\nano\.claude\debug" => "ok";
    manual_games_folder: M, r"C:\Games\Old" => "ok";
    manual_sibling_of_documents: M, r"C:\Users\nano\DocumentsBackup" => "ok";
    manual_sibling_of_windows: M, r"C:\WindowsApps-backup" => "ok";
    manual_sibling_of_program_files: M, r"C:\Program Files Old" => "ok";
    manual_sibling_of_ssh: M, r"C:\Users\nano\.ssh-old" => "ok";
    manual_programdata_app: M, r"C:\ProgramData\SomeVendor\cache" => "protected";
    manual_other_user_file: M, r"C:\Users\guest\Downloads\x.zip" => "ok";
    manual_dotdot_back_inside: M, r"C:\Users\nano\Downloads\a\..\b.iso" => "ok";
    manual_dot_segments: M, r"C:\Users\nano\.\Downloads\.\x.iso" => "ok";
    manual_deep_project: M, r"C:\dev\proj\target\debug\build" => "ok";
    manual_node_modules: M, r"C:\dev\web\node_modules" => "ok";
    manual_windows_named_file_elsewhere: M, r"C:\dev\Windows\notes.txt" => "ok";
    manual_system32_named_elsewhere: M, r"C:\dev\System32" => "ok";
    manual_users_named_deeper: M, r"C:\dev\Users" => "ok";
    manual_recycle_named_deeper: M, r"C:\dev\$Recycle.Bin" => "ok";
    manual_npmrc_named_folder: M, r"C:\dev\.npmrc" => "ok";
    manual_cargo_bin_sibling: M, r"C:\Users\nano\.cargo\binaries" => "ok";
    manual_claude_debug_file: M, r"C:\Users\nano\.claude\debug\a.txt" => "ok";
    manual_docker_other_file: M, r"C:\Users\nano\.docker\buildx" => "ok";
    manual_codex_log: M, r"C:\Users\nano\.codex\log\codex-tui.log" => "ok";
}

// ---------------------------------------------------------------- manual: blocked
cases! {
    manual_volume_root: M, r"C:\" => "volume-root";
    manual_windows_dir: M, r"C:\Windows" => "system-dir";
    manual_windows_temp: M, r"C:\Windows\Temp\x.log" => "system-dir";
    manual_system32: M, r"C:\WINDOWS\System32" => "system-dir";
    manual_system32_file: M, r"C:\Windows\System32\drivers\etc\hosts" => "system-dir";
    manual_winsxs: M, r"C:\Windows\WinSxS\x" => "system-dir";
    manual_windows_minidump: M, r"C:\Windows\Minidump\a.dmp" => "system-dir";
    manual_program_files: M, r"C:\Program Files\App" => "protected";
    manual_program_files_itself: M, r"C:\Program Files" => "protected";
    manual_program_files_x86: M, r"C:\Program Files (x86)" => "protected";
    manual_program_files_x86_app: M, r"C:\Program Files (x86)\Steam\steamapps" => "protected";
    manual_programdata_microsoft: M, r"C:\ProgramData\Microsoft\Windows" => "protected";
    manual_programdata_itself: M, r"C:\ProgramData" => "protected";
    manual_pagefile: M, r"C:\pagefile.sys" => "protected";
    manual_hiberfil: M, r"C:\hiberfil.sys" => "protected";
    manual_swapfile: M, r"C:\swapfile.sys" => "protected";
    manual_dumpstack: M, r"C:\DumpStack.log.tmp" => "protected";
    manual_recycle_bin: M, r"C:\$Recycle.Bin" => "protected";
    manual_recycle_bin_sid: M, r"C:\$Recycle.Bin\S-1-5-21\$R123.iso" => "protected";
    manual_svi: M, r"C:\System Volume Information" => "protected";
    manual_recovery: M, r"C:\Recovery\WindowsRE" => "protected";
    manual_efi: M, r"C:\EFI" => "protected";
    manual_boot: M, r"C:\Boot\BCD" => "protected";
    manual_windows_old: M, r"C:\Windows.old" => "protected";
    manual_windows_old_inner: M, r"C:\Windows.old\Users" => "protected";
    manual_ssh_key: M, r"C:\Users\nano\.ssh\id_ed25519" => "protected";
    manual_ssh_dir: M, r"C:\Users\nano\.ssh" => "protected";
    manual_gnupg: M, r"C:\Users\nano\.gnupg\pubring.kbx" => "protected";
    manual_aws: M, r"C:\Users\nano\.aws\credentials" => "protected";
    manual_azure: M, r"C:\Users\nano\.azure" => "protected";
    manual_kube: M, r"C:\Users\nano\.kube\config" => "protected";
    manual_git_credentials: M, r"C:\Users\nano\.git-credentials" => "protected";
    manual_npmrc: M, r"C:\Users\nano\.npmrc" => "protected";
    manual_netrc: M, r"C:\Users\nano\.netrc" => "protected";
    manual_docker_config: M, r"C:\Users\nano\.docker\config.json" => "protected";
    manual_cargo_credentials: M, r"C:\Users\nano\.cargo\credentials.toml" => "protected";
    manual_cargo_config: M, r"C:\Users\nano\.cargo\config.toml" => "protected";
    manual_cargo_bin: M, r"C:\Users\nano\.cargo\bin\cargo.exe" => "protected";
    manual_claude_json: M, r"C:\Users\nano\.claude.json" => "protected";
    manual_claude_settings: M, r"C:\Users\nano\.claude\settings.json" => "protected";
    manual_claude_settings_local: M, r"C:\Users\nano\.claude\settings.local.json" => "protected";
    manual_codex_auth: M, r"C:\Users\nano\.codex\auth.json" => "protected";
    manual_codex_config: M, r"C:\Users\nano\.codex\config.toml" => "protected";
    manual_gemini_creds: M, r"C:\Users\nano\.gemini\oauth_creds.json" => "protected";
    manual_gemini_settings: M, r"C:\Users\nano\.gemini\settings.json" => "protected";
}

// ------------------------------------------------- manual: containers and ancestors
cases! {
    manual_users_root: M, r"C:\Users" => "contains-protected";
    manual_home: M, r"C:\Users\nano" => "contains-protected";
    manual_documents_folder: M, r"C:\Users\nano\Documents" => "contains-protected";
    manual_downloads_trailing_sep: M, r"C:\Users\nano\Downloads\" => "contains-protected";
    manual_desktop_folder: M, r"C:\Users\nano\Desktop" => "contains-protected";
    manual_pictures_folder: M, r"C:\Users\nano\Pictures" => "contains-protected";
    manual_videos_folder: M, r"C:\Users\nano\Videos" => "contains-protected";
    manual_music_folder_itself: M, r"C:\Users\nano\Music" => "contains-protected";
    manual_onedrive_folder: M, r"C:\Users\nano\OneDrive" => "contains-protected";
    manual_appdata: M, r"C:\Users\nano\AppData" => "contains-protected";
    manual_appdata_local: M, r"C:\Users\nano\AppData\Local" => "contains-protected";
    manual_appdata_roaming: M, r"C:\Users\nano\AppData\Roaming" => "contains-protected";
    manual_temp_folder: M, r"C:\Users\nano\AppData\Local\Temp" => "contains-protected";
    manual_claude_dir: M, r"C:\Users\nano\.claude" => "contains-protected";
    manual_cargo_dir: M, r"C:\Users\nano\.cargo" => "contains-protected";
    manual_codex_dir: M, r"C:\Users\nano\.codex" => "contains-protected";
    manual_gemini_dir: M, r"C:\Users\nano\.gemini" => "contains-protected";
    manual_docker_dir: M, r"C:\Users\nano\.docker" => "contains-protected";
}

// ------------------------------------------------------ manual: lexical tricks
cases! {
    manual_dotdot_into_windows: M, r"C:\Games\..\Windows\notepad.exe" => "system-dir";
    manual_dotdot_into_ssh: M, r"C:\Users\nano\Documents\..\.ssh" => "protected";
    manual_dot_documents: M, r"C:\Users\nano\.\Documents" => "contains-protected";
    manual_many_dotdots: M, r"C:\a\b\c\..\..\..\Windows\System32" => "system-dir";
    manual_dotdot_past_root: M, r"C:\..\..\Windows" => "system-dir";
    manual_dotdot_to_root: M, r"C:\Games\.." => "volume-root";
    manual_dotdot_to_home: M, r"C:\Users\nano\Downloads\.." => "contains-protected";
    manual_relative: M, "relative/path" => "unresolvable";
    manual_relative_dotdot: M, r"..\Windows" => "unresolvable";
    manual_empty: M, "" => "unresolvable";
    manual_upper_case_windows: M, r"C:\WINDOWS\SYSTEM32\CONFIG" => "system-dir";
    manual_mixed_case_program_files: M, r"C:\pRoGrAm FiLeS\x" => "protected";
    manual_upper_ssh: M, r"C:\USERS\NANO\.SSH\ID_RSA" => "protected";
    manual_forward_slashes: M, "C:/Windows/System32" => "system-dir";
    manual_mixed_separators: M, r"C:\Users/nano\.ssh/id_rsa" => "protected";
    manual_double_separators: M, r"C:\\Windows\\\System32" => "system-dir";
    manual_double_sep_documents: M, r"C:\Users\\nano\\Documents" => "contains-protected";
}

// ------------------------------------------------------------- manual: Windows forms
cases! {
    #[cfg(windows)] win_verbatim_windows: M, r"\\?\C:\Windows\x" => "system-dir";
    #[cfg(windows)] win_verbatim_ok: M, r"\\?\C:\Users\nano\Downloads\x.iso" => "ok";
    #[cfg(windows)] win_verbatim_ssh: M, r"\\?\C:\Users\nano\.ssh" => "protected";
    #[cfg(windows)] win_verbatim_root: M, r"\\?\C:\" => "volume-root";
    #[cfg(windows)] win_verbatim_unc_root: M, r"\\?\UNC\nas\share\" => "volume-root";
    #[cfg(windows)] win_verbatim_unc_file: M, r"\\?\UNC\nas\share\movies\a.mkv" => "ok";
    #[cfg(windows)] win_unc_root: M, r"\\nas\share" => "volume-root";
    #[cfg(windows)] win_unc_root_sep: M, r"\\nas\share\" => "volume-root";
    #[cfg(windows)] win_unc_file: M, r"\\nas\share\movies\a.mkv" => "ok";
    #[cfg(windows)] win_unc_recycle: M, r"\\nas\share\$Recycle.Bin" => "protected";
    #[cfg(windows)] win_unc_windows: M, r"\\nas\share\Windows\x" => "protected";
    #[cfg(windows)] win_unc_users: M, r"\\nas\share\Users" => "contains-protected";
    #[cfg(windows)] win_unc_svi: M, r"\\nas\share\System Volume Information" => "protected";
    #[cfg(windows)] win_device_physical: M, r"\\.\PhysicalDrive0" => "device";
    #[cfg(windows)] win_device_volume: M, r"\\.\C:\Users\nano\Downloads\x" => "device";
    #[cfg(windows)] win_device_globalroot: M, r"\\?\GLOBALROOT\Device\HarddiskVolume3\x" => "device";
    #[cfg(windows)] win_device_nul: M, r"C:\Users\nano\Downloads\NUL" => "device";
    #[cfg(windows)] win_device_con_ext: M, r"C:\Users\nano\Downloads\con.txt" => "device";
    #[cfg(windows)] win_device_com1: M, r"C:\Users\nano\Downloads\COM1" => "device";
    #[cfg(windows)] win_device_lpt9_ext: M, r"C:\Users\nano\Downloads\lpt9.log" => "device";
    #[cfg(windows)] win_device_aux_folder: M, r"C:\Games\aux\x" => "device";
    #[cfg(windows)] win_device_prn: M, r"C:\Games\PRN" => "device";
    #[cfg(windows)] win_device_nul_space: M, r"C:\Games\NUL .txt" => "device";
    #[cfg(windows)] win_device_conin: M, r"C:\Games\CONIN$" => "device";
    #[cfg(windows)] win_device_com_superscript: M, "C:\\Games\\COM\u{b9}" => "device";
    #[cfg(windows)] win_not_device_console: M, r"C:\Games\console.txt" => "ok";
    #[cfg(windows)] win_not_device_com10: M, r"C:\Games\COM10" => "ok";
    #[cfg(windows)] win_not_device_nullx: M, r"C:\Games\nullable" => "ok";
    #[cfg(windows)] win_stream: M, r"C:\Users\nano\Downloads\file.txt:secret" => "stream";
    #[cfg(windows)] win_stream_data: M, r"C:\Users\nano\Downloads\file.txt::$DATA" => "stream";
    #[cfg(windows)] win_stream_dir: M, r"C:\Games:stream\x" => "stream";
    #[cfg(windows)] win_stream_zone: M, r"C:\Users\nano\Downloads\a.exe:Zone.Identifier" => "stream";
    #[cfg(windows)] win_trailing_dot_windows: M, r"C:\Windows.\System32" => "system-dir";
    #[cfg(windows)] win_trailing_space_windows: M, r"C:\Windows \System32" => "system-dir";
    #[cfg(windows)] win_trailing_dots_program_files: M, r"C:\Program Files...\x" => "protected";
    #[cfg(windows)] win_trailing_dot_documents: M, r"C:\Users\nano\Documents." => "contains-protected";
    #[cfg(windows)] win_trailing_dot_space_documents: M, r"C:\Users\nano\Documents ." => "contains-protected";
    #[cfg(windows)] win_trailing_dot_ssh: M, r"C:\Users\nano\.ssh.\id_rsa" => "protected";
    #[cfg(windows)] win_trailing_space_npmrc: M, r"C:\Users\nano\.npmrc " => "protected";
    #[cfg(windows)] win_lower_drive: M, r"c:\windows\system32" => "system-dir";
    #[cfg(windows)] win_drive_relative: M, r"C:Windows\System32" => "unresolvable";
    #[cfg(windows)] win_rooted_no_drive: M, r"\Windows\System32" => "unresolvable";
    #[cfg(windows)] win_other_drive_root: M, r"D:\" => "volume-root";
    #[cfg(windows)] win_other_drive_recycle: M, r"D:\$Recycle.Bin\S-1-5" => "protected";
    #[cfg(windows)] win_other_drive_svi: M, r"D:\System Volume Information" => "protected";
    #[cfg(windows)] win_other_drive_windows: M, r"D:\Windows\x" => "protected";
    #[cfg(windows)] win_other_drive_program_files: M, r"D:\Program Files\Game" => "protected";
    #[cfg(windows)] win_other_drive_program_files_x86: M, r"D:\Program Files (x86)\x" => "protected";
    #[cfg(windows)] win_other_drive_programdata: M, r"D:\ProgramData\x" => "protected";
    #[cfg(windows)] win_other_drive_pagefile: M, r"D:\pagefile.sys" => "protected";
    #[cfg(windows)] win_other_drive_extend: M, r"D:\$Extend\$UsnJrnl" => "protected";
    #[cfg(windows)] win_other_drive_mft: M, r"D:\$MFT" => "protected";
    #[cfg(windows)] win_other_drive_logfile: M, r"D:\$LogFile" => "protected";
    #[cfg(windows)] win_other_drive_recovery: M, r"D:\Recovery" => "protected";
    #[cfg(windows)] win_other_drive_efi: M, r"D:\EFI\Boot" => "protected";
    #[cfg(windows)] win_other_drive_boot: M, r"D:\boot" => "protected";
    #[cfg(windows)] win_other_drive_users: M, r"D:\Users" => "contains-protected";
    #[cfg(windows)] win_other_drive_users_inner: M, r"D:\Users\old\stuff" => "ok";
    #[cfg(windows)] win_other_drive_projects: M, r"D:\Projects\target" => "ok";
    #[cfg(windows)] win_other_drive_games: M, r"E:\SteamLibrary\steamapps\common\Old" => "ok";
    #[cfg(windows)] win_other_drive_upper: M, r"D:\SYSTEM VOLUME INFORMATION\x" => "protected";
}

// ---------------------------------------------------------------- rules: allowed
cases! {
    rule_temp_item: SAFE, r"C:\Users\nano\AppData\Local\Temp\x" => "ok";
    rule_temp_deep: SAFE, r"C:\Users\nano\AppData\Local\Temp\a\b\c.tmp" => "ok";
    rule_windows_temp: SAFE, r"C:\Windows\Temp\x" => "ok";
    rule_windows_temp_deep: SAFE, r"C:\Windows\Temp\sub\x.log" => "ok";
    rule_cbs_log: SAFE, r"C:\Windows\Logs\CBS\CbsPersist_1.log" => "ok";
    rule_dism_log: SAFE, r"C:\Windows\Logs\DISM\dism.log" => "ok";
    rule_minidump: SAFE, r"C:\Windows\Minidump\a.dmp" => "ok";
    rule_memory_dmp: SAFE, r"C:\Windows\MEMORY.DMP" => "ok";
    rule_live_kernel: SAFE, r"C:\Windows\LiveKernelReports\WATCHDOG" => "ok";
    rule_update_download: ADVANCED, r"C:\Windows\SoftwareDistribution\Download\abc" => "ok";
    rule_thumbcache: SAFE, r"C:\Users\nano\AppData\Local\Microsoft\Windows\Explorer\thumbcache_32.db" => "ok";
    rule_inetcache: SAFE, r"C:\Users\nano\AppData\Local\Microsoft\Windows\INetCache\IE" => "ok";
    rule_crashdumps: SAFE, r"C:\Users\nano\AppData\Local\CrashDumps\app.dmp" => "ok";
    rule_nvidia_dx: SAFE, r"C:\Users\nano\AppData\Local\NVIDIA\DXCache\x.bin" => "ok";
    rule_d3dscache: SAFE, r"C:\Users\nano\AppData\Local\D3DSCache\abc" => "ok";
    rule_programdata_nvidia: SAFE, r"C:\ProgramData\NVIDIA Corporation\Downloader\x" => "ok";
    rule_chrome_cache: SAFE, r"C:\Users\nano\AppData\Local\Google\Chrome\User Data\Default\Cache\Cache_Data" => "ok";
    rule_firefox_cache: SAFE, r"C:\Users\nano\AppData\Local\Mozilla\Firefox\Profiles\x.default\cache2" => "ok";
    rule_claude_debug: SAFE, r"C:\Users\nano\.claude\debug\x.txt" => "ok";
    rule_claude_sessions: CAUTION, r"C:\Users\nano\.claude\projects\p\a.jsonl" => "protected";
    rule_hf_cache: CAUTION, r"C:\Users\nano\.cache\huggingface\hub\models--x" => "ok";
    rule_roaming_cache: SAFE, r"C:\Users\nano\AppData\Roaming\Code\Cache" => "ok";
}

// ---------------------------------------------------------------- rules: blocked
cases! {
    rule_protected_level: PROTECTED, r"C:\Users\nano\AppData\Local\Temp\x" => "protected-level";
    rule_protected_level_anywhere: PROTECTED, r"C:\Games\x" => "protected-level";
    rule_temp_folder_itself: SAFE, r"C:\Users\nano\AppData\Local\Temp" => "contains-protected";
    rule_windows_temp_itself: SAFE, r"C:\Windows\Temp" => "system-dir";
    rule_windows_itself: SAFE, r"C:\Windows" => "system-dir";
    rule_windows_logs_other: SAFE, r"C:\Windows\Logs\x.log" => "system-dir";
    rule_windows_logs_cbs_itself: SAFE, r"C:\Windows\Logs\CBS" => "system-dir";
    rule_windows_update_datastore: SAFE, r"C:\Windows\SoftwareDistribution\DataStore\x" => "system-dir";
    rule_windows_update_itself: ADVANCED, r"C:\Windows\SoftwareDistribution\Download" => "system-dir";
    rule_system32: SAFE, r"C:\Windows\System32\x" => "system-dir";
    rule_syswow64: SAFE, r"C:\Windows\SysWOW64\x.dll" => "system-dir";
    rule_winsxs_temp: SAFE, r"C:\Windows\WinSxS\Temp\x" => "system-dir";
    rule_installer: SAFE, r"C:\Windows\Installer\x.msi" => "system-dir";
    rule_prefetch: SAFE, r"C:\Windows\Prefetch\x.pf" => "system-dir";
    rule_fonts: SAFE, r"C:\Windows\Fonts\arial.ttf" => "system-dir";
    rule_windows_root_file: SAFE, r"C:\Windows\explorer.exe" => "system-dir";
    rule_temp_dotdot_escape: SAFE, r"C:\Windows\Temp\..\System32\x" => "system-dir";
    rule_temp_prefix_trick: SAFE, r"C:\Windows\TempX\x" => "system-dir";
    rule_minidump_prefix_trick: SAFE, r"C:\Windows\Minidump2\a.dmp" => "system-dir";
    rule_memory_dmp_folder_trick: SAFE, r"C:\Windows\MEMORY.DMP\x" => "system-dir";
    rule_programdata_itself: SAFE, r"C:\ProgramData" => "protected";
    rule_programdata_microsoft: SAFE, r"C:\ProgramData\Microsoft\Windows\WER\x" => "protected";
    rule_programdata_defender: SAFE, r"C:\ProgramData\Microsoft\Windows Defender\Scans\x" => "protected";
    rule_program_files: SAFE, r"C:\Program Files\App\cache" => "protected";
    rule_program_files_x86: SAFE, r"C:\Program Files (x86)\App\cache" => "protected";
    rule_documents: SAFE, r"C:\Users\nano\Documents\x" => "protected";
    rule_downloads: SAFE, r"C:\Users\nano\Downloads\x.iso" => "protected";
    rule_desktop: SAFE, r"C:\Users\nano\Desktop\x" => "protected";
    rule_pictures: SAFE, r"C:\Users\nano\Pictures\cache" => "protected";
    rule_videos: SAFE, r"C:\Users\nano\Videos\x" => "protected";
    rule_music: SAFE, r"C:\Users\nano\Music\x" => "protected";
    rule_onedrive: SAFE, r"C:\Users\nano\OneDrive\cache" => "protected";
    rule_documents_folder: SAFE, r"C:\Users\nano\Documents" => "protected";
    rule_ssh: SAFE, r"C:\Users\nano\.ssh\known_hosts" => "protected";
    rule_cargo_bin: SAFE, r"C:\Users\nano\.cargo\bin" => "protected";
    rule_claude_settings: SAFE, r"C:\Users\nano\.claude\settings.json" => "protected";
    rule_claude_dir: SAFE, r"C:\Users\nano\.claude" => "contains-protected";
    rule_home: SAFE, r"C:\Users\nano" => "contains-protected";
    rule_appdata_local: SAFE, r"C:\Users\nano\AppData\Local" => "contains-protected";
    rule_users: SAFE, r"C:\Users" => "contains-protected";
    rule_volume_root: SAFE, r"C:\" => "volume-root";
    rule_pagefile: SAFE, r"C:\pagefile.sys" => "protected";
    rule_windows_old: SAFE, r"C:\Windows.old\x" => "protected";
    rule_recycle: SAFE, r"C:\$Recycle.Bin\x" => "protected";
    rule_svi: SAFE, r"C:\System Volume Information\x" => "protected";
    rule_dotdot_into_documents: SAFE, r"C:\Users\nano\AppData\Local\Temp\..\..\..\Documents\x" => "protected";
    rule_case_documents: SAFE, r"C:\USERS\NANO\DOCUMENTS\X" => "protected";
    rule_relative: SAFE, r"Temp\x" => "unresolvable";
    #[cfg(windows)] rule_other_drive_windows_temp: SAFE, r"D:\Windows\Temp\x" => "protected";
    #[cfg(windows)] rule_other_drive_programdata: SAFE, r"D:\ProgramData\x" => "protected";
    #[cfg(windows)] rule_verbatim_system32: SAFE, r"\\?\C:\Windows\System32\x" => "system-dir";
    #[cfg(windows)] rule_trailing_dot_system32: SAFE, r"C:\Windows\System32.\x" => "system-dir";
    #[cfg(windows)] rule_trailing_dot_temp_ok: SAFE, r"C:\Windows\Temp.\x" => "ok";
    #[cfg(windows)] rule_stream: SAFE, r"C:\Users\nano\AppData\Local\Temp\x:ads" => "stream";
    #[cfg(windows)] rule_device: SAFE, r"C:\Users\nano\AppData\Local\Temp\nul" => "device";
    #[cfg(windows)] rule_unc_device: SAFE, r"\\.\pipe\x" => "device";
}

// ------------------------------------------------------- live checks (fake probe)

#[derive(Default)]
struct Fake {
    facts: HashMap<PathBuf, Facts>,
    finals: HashMap<PathBuf, PathBuf>,
    roots: HashMap<PathBuf, u64>,
    lockers: HashMap<PathBuf, Vec<String>>,
    running: Vec<String>,
}

impl Fake {
    fn file(mut self, path: &str, size: u64, mtime: i64) -> Self {
        self.facts.insert(
            p(path),
            Facts {
                size,
                mtime,
                volume_id: 1,
                ..Facts::default()
            },
        );
        self
    }
    fn dir(mut self, path: &str) -> Self {
        self.facts.insert(
            p(path),
            Facts {
                is_dir: true,
                volume_id: 1,
                ..Facts::default()
            },
        );
        self
    }
    fn with(mut self, path: &str, f: impl FnOnce(&mut Facts)) -> Self {
        if let Some(facts) = self.facts.get_mut(&p(path)) {
            f(facts);
        }
        self
    }
    fn real(mut self, path: &str, real: &str) -> Self {
        self.finals.insert(p(path), p(real));
        self
    }
    fn root(mut self, root: &str, id: u64) -> Self {
        self.roots.insert(p(root), id);
        self
    }
    fn locked(mut self, path: &str, by: &str) -> Self {
        self.lockers.insert(p(path), vec![by.to_owned()]);
        self
    }
}

impl Probe for Fake {
    fn facts(&self, path: &Path) -> io::Result<Facts> {
        self.facts
            .get(path)
            .copied()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
    fn final_path(&self, path: &Path) -> io::Result<PathBuf> {
        if !self.facts.contains_key(path) && !self.finals.contains_key(path) {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        Ok(self
            .finals
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_path_buf()))
    }
    fn root_volume(&self, root: &Path) -> Option<u64> {
        self.roots.get(root).copied()
    }
    fn lockers(&self, path: &Path, _is_dir: bool) -> Vec<String> {
        self.lockers.get(path).cloned().unwrap_or_default()
    }
    fn running(&self) -> Vec<String> {
        self.running.clone()
    }
}

fn live(
    fake: &Fake,
    origin: Origin,
    path: &str,
    expected: Option<Expected>,
) -> Result<Vec<Warning>, Blocked> {
    gate().check_live(&p(path), origin, expected, fake)
}

fn live_code(fake: &Fake, origin: Origin, path: &str) -> &'static str {
    match live(fake, origin, path, None) {
        Ok(w) if w.is_empty() => "ok",
        Ok(_) => "warn",
        Err(b) => b.code(),
    }
}

const DL: &str = r"C:\Users\nano\Downloads\x.iso";
const TMP: &str = r"C:\Users\nano\AppData\Local\Temp\x.tmp";
const WTMP: &str = r"C:\Windows\Temp\x.tmp";

#[test]
fn live_plain_file_is_allowed() {
    let fake = Fake::default().file(DL, 10, 5);
    assert_eq!(live_code(&fake, M, DL), "ok");
}

#[test]
fn live_missing_is_unresolvable() {
    assert_eq!(live_code(&Fake::default(), M, DL), "unresolvable");
}

#[test]
fn live_link_is_refused() {
    let fake = Fake::default().file(DL, 0, 0).with(DL, |f| f.link = true);
    assert_eq!(live_code(&fake, M, DL), "link");
}

#[test]
fn live_link_refused_for_rules() {
    let fake = Fake::default().dir(TMP).with(TMP, |f| f.link = true);
    assert_eq!(live_code(&fake, SAFE, TMP), "link");
}

#[test]
fn live_junction_parent_into_windows() {
    let path = r"C:\Games\link\notepad.exe";
    let fake = Fake::default()
        .file(path, 1, 1)
        .real(path, r"C:\Windows\System32\notepad.exe");
    assert_eq!(live_code(&fake, M, path), "system-dir");
}

#[test]
fn live_junction_parent_into_documents_for_rule() {
    let path = r"C:\Users\nano\AppData\Local\Temp\j\cv.docx";
    let fake = Fake::default()
        .file(path, 1, 1)
        .real(path, r"C:\Users\nano\Documents\cv.docx");
    assert_eq!(live_code(&fake, SAFE, path), "protected");
}

#[test]
fn live_junction_parent_into_ssh() {
    let path = r"C:\Games\keys\id_rsa";
    let fake = Fake::default()
        .file(path, 1, 1)
        .real(path, r"C:\Users\nano\.ssh\id_rsa");
    assert_eq!(live_code(&fake, M, path), "protected");
}

#[test]
fn live_short_name_program_files() {
    let path = r"C:\PROGRA~1\App\cache";
    let fake = Fake::default()
        .dir(path)
        .real(path, r"C:\Program Files\App\cache");
    assert_eq!(code(M, path), "ok", "lexically it looks harmless");
    assert_eq!(live_code(&fake, M, path), "protected");
}

#[test]
fn live_short_name_program_files_x86() {
    let path = r"C:\PROGRA~2\App";
    let fake = Fake::default()
        .dir(path)
        .real(path, r"C:\Program Files (x86)\App");
    assert_eq!(live_code(&fake, M, path), "protected");
}

#[test]
fn live_short_name_documents() {
    let path = r"C:\Users\nano\DOCUME~1";
    let fake = Fake::default()
        .dir(path)
        .real(path, r"C:\Users\nano\Documents");
    assert_eq!(live_code(&fake, M, path), "contains-protected");
}

#[test]
fn live_short_name_windows_for_rule() {
    let path = r"C:\WINDOW~1\System32\x.dll";
    let fake = Fake::default()
        .file(path, 1, 1)
        .real(path, r"C:\Windows\System32\x.dll");
    assert_eq!(live_code(&fake, SAFE, path), "system-dir");
}

#[test]
fn live_final_path_with_case_change_is_fine() {
    let fake = Fake::default()
        .file(DL, 1, 1)
        .real(DL, r"C:\USERS\NANO\DOWNLOADS\X.ISO");
    assert_eq!(live_code(&fake, M, DL), "ok");
}

#[cfg(windows)]
#[test]
fn live_final_path_verbatim_is_fine() {
    let fake = Fake::default()
        .file(DL, 1, 1)
        .real(DL, r"\\?\C:\Users\nano\Downloads\x.iso");
    assert_eq!(live_code(&fake, M, DL), "ok");
}

#[cfg(windows)]
#[test]
fn live_final_path_verbatim_into_windows() {
    let path = r"C:\Games\j\x";
    let fake = Fake::default()
        .file(path, 1, 1)
        .real(path, r"\\?\C:\Windows\x");
    assert_eq!(live_code(&fake, M, path), "system-dir");
}

#[cfg(windows)]
#[test]
fn live_symlink_to_unc_root() {
    let path = r"C:\Games\share";
    let fake = Fake::default().dir(path).real(path, r"\\?\UNC\nas\share");
    assert_eq!(live_code(&fake, M, path), "volume-root");
}

#[cfg(windows)]
#[test]
fn live_mount_point_to_other_volume() {
    let path = r"C:\Games\mnt\data";
    let fake = Fake::default()
        .dir(path)
        .with(path, |f| f.volume_id = 2)
        .root(r"C:\", 1);
    assert_eq!(live_code(&fake, M, path), "cross-volume");
}

#[cfg(windows)]
#[test]
fn live_same_volume_passes() {
    let fake = Fake::default().file(DL, 1, 1).root(r"C:\", 1);
    assert_eq!(live_code(&fake, M, DL), "ok");
}

#[cfg(windows)]
#[test]
fn live_cross_volume_for_rules() {
    let fake = Fake::default()
        .dir(TMP)
        .with(TMP, |f| f.volume_id = 9)
        .root(r"C:\", 1);
    assert_eq!(live_code(&fake, SAFE, TMP), "cross-volume");
}

#[cfg(windows)]
#[test]
fn live_onedrive_placeholder_folder_via_final_path() {
    let path = r"C:\Games\od";
    let fake = Fake::default()
        .dir(path)
        .real(path, r"C:\Users\nano\OneDrive")
        .with(path, |f| f.cloud = true);
    assert_eq!(live_code(&fake, M, path), "contains-protected");
}

#[test]
fn live_system_attr_refused_for_rules() {
    let fake = Fake::default()
        .file(TMP, 1, 1)
        .with(TMP, |f| f.system = true);
    assert_eq!(live_code(&fake, SAFE, TMP), "system-file");
}

#[test]
fn live_system_attr_in_windows_temp() {
    let fake = Fake::default()
        .file(WTMP, 1, 1)
        .with(WTMP, |f| f.system = true);
    assert_eq!(live_code(&fake, SAFE, WTMP), "system-file");
}

#[test]
fn live_system_attr_manual_outside_system_dirs_is_allowed() {
    let path = r"C:\Users\nano\Downloads\desktop.ini";
    let fake = Fake::default()
        .file(path, 1, 1)
        .with(path, |f| f.system = true);
    assert_eq!(live_code(&fake, M, path), "ok");
}

#[test]
fn live_system_binary_dll_in_windows_temp() {
    let path = r"C:\Windows\Temp\x.dll";
    let fake = Fake::default().file(path, 1, 1);
    assert_eq!(live_code(&fake, SAFE, path), "system-binary");
}

#[test]
fn live_system_binary_upper_case() {
    let path = r"C:\Windows\Temp\SETUP.EXE";
    let fake = Fake::default().file(path, 1, 1);
    assert_eq!(live_code(&fake, SAFE, path), "system-binary");
}

#[test]
fn live_system_binary_sys_driver() {
    let path = r"C:\Windows\Temp\drv\x.sys";
    let fake = Fake::default().file(path, 1, 1);
    assert_eq!(live_code(&fake, SAFE, path), "system-binary");
}

#[test]
fn live_system_binary_mui() {
    let path = r"C:\Windows\Temp\en-US\x.dll.mui";
    let fake = Fake::default().file(path, 1, 1);
    assert_eq!(live_code(&fake, SAFE, path), "system-binary");
}

#[test]
fn live_windows_temp_log_is_fine() {
    let fake = Fake::default().file(WTMP, 1, 1);
    assert_eq!(live_code(&fake, SAFE, WTMP), "ok");
}

#[test]
fn live_windows_temp_folder_is_fine() {
    let path = r"C:\Windows\Temp\msdt";
    let fake = Fake::default().dir(path);
    assert_eq!(live_code(&fake, SAFE, path), "ok");
}

#[test]
fn live_exe_outside_windows_is_fine() {
    let path = r"C:\Users\nano\AppData\Local\Temp\setup.exe";
    let fake = Fake::default().file(path, 1, 1);
    assert_eq!(live_code(&fake, SAFE, path), "ok");
}

#[test]
fn live_cloud_placeholder_refused_for_rules() {
    let fake = Fake::default()
        .file(TMP, 1, 1)
        .with(TMP, |f| f.cloud = true);
    assert_eq!(live_code(&fake, SAFE, TMP), "cloud-file");
}

#[test]
fn live_cloud_placeholder_warns_for_manual() {
    let fake = Fake::default().file(DL, 1, 1).with(DL, |f| f.cloud = true);
    assert_eq!(
        live(&fake, M, DL, None),
        Ok(vec![Warning::CloudPlaceholder])
    );
}

#[test]
fn live_cloud_placeholder_refused_for_duplicates() {
    let fake = Fake::default().file(DL, 1, 1).with(DL, |f| f.cloud = true);
    assert_eq!(
        live_code(&fake, Origin::Duplicate { group: 3 }, DL),
        "cloud-file"
    );
}

#[test]
fn duplicates_follow_the_hand_picked_rules() {
    let dupe = Origin::Duplicate { group: 1 };
    assert_eq!(code(dupe, r"C:\Users\nano\Downloads\x.jpg"), "ok");
    assert_eq!(code(dupe, r"C:\Users\nano\Documents\x.jpg"), "ok");
    assert_eq!(code(dupe, r"C:\Windows\Web\x.jpg"), "system-dir");
    assert_eq!(code(dupe, r"C:\Program Files\App\x.dll"), "protected");
    assert_eq!(
        code(dupe, r"C:\Users\nano\.claude\projects\p\s.jsonl"),
        "protected"
    );
    assert_eq!(code(dupe, r"C:\Users\nano\Documents"), "contains-protected");
}

fn exp(is_dir: bool, size: u64, mtime: i64) -> Option<Expected> {
    Some(Expected {
        is_dir,
        size,
        mtime,
    })
}

#[test]
fn live_unchanged_file_passes() {
    let fake = Fake::default().file(DL, 10, 5);
    assert_eq!(live(&fake, M, DL, exp(false, 10, 5)), Ok(vec![]));
}

#[test]
fn live_size_changed() {
    let fake = Fake::default().file(DL, 11, 5);
    assert_eq!(live(&fake, M, DL, exp(false, 10, 5)), Err(Blocked::Changed));
}

#[test]
fn live_mtime_changed() {
    let fake = Fake::default().file(DL, 10, 6);
    assert_eq!(live(&fake, M, DL, exp(false, 10, 5)), Err(Blocked::Changed));
}

#[test]
fn live_file_became_dir() {
    let fake = Fake::default().dir(DL);
    assert_eq!(live(&fake, M, DL, exp(false, 10, 5)), Err(Blocked::Changed));
}

#[test]
fn live_dir_became_file() {
    let fake = Fake::default().file(TMP, 1, 1);
    assert_eq!(
        live(&fake, SAFE, TMP, exp(true, 0, 0)),
        Err(Blocked::Changed)
    );
}

#[test]
fn live_dir_contents_changing_is_fine() {
    let fake = Fake::default().dir(TMP).with(TMP, |f| f.mtime = 99);
    assert_eq!(live(&fake, SAFE, TMP, exp(true, 0, 1)), Ok(vec![]));
}

#[test]
fn live_locked_file_warns() {
    let fake = Fake::default().file(TMP, 1, 1).locked(TMP, "Google Chrome");
    assert_eq!(
        live(&fake, SAFE, TMP, None),
        Ok(vec![Warning::InUse(vec!["Google Chrome".into()])])
    );
}

#[test]
fn live_caution_warns() {
    let fake = Fake::default().file(TMP, 1, 1);
    assert_eq!(live(&fake, CAUTION, TMP, None), Ok(vec![Warning::Caution]));
}

#[test]
fn live_advanced_warns() {
    let fake = Fake::default().file(TMP, 1, 1);
    assert_eq!(
        live(&fake, ADVANCED, TMP, None),
        Ok(vec![Warning::Advanced])
    );
}

#[test]
fn live_protected_level_never_reaches_disk() {
    assert_eq!(
        live_code(&Fake::default(), PROTECTED, TMP),
        "protected-level"
    );
}

#[test]
fn live_lexical_block_wins_before_probe() {
    // Not even probed: a protected path is refused without touching the disk.
    assert_eq!(
        live_code(&Fake::default(), M, r"C:\Windows\System32\x"),
        "system-dir"
    );
}

#[test]
fn live_final_path_unreachable() {
    #[derive(Default)]
    struct Broken;
    impl Probe for Broken {
        fn facts(&self, _: &Path) -> io::Result<Facts> {
            Ok(Facts::default())
        }
        fn final_path(&self, _: &Path) -> io::Result<PathBuf> {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        }
        fn root_volume(&self, _: &Path) -> Option<u64> {
            None
        }
        fn lockers(&self, _: &Path, _: bool) -> Vec<String> {
            Vec::new()
        }
        fn running(&self) -> Vec<String> {
            Vec::new()
        }
    }
    assert!(matches!(
        gate().check_live(&p(DL), M, None, &Broken),
        Err(Blocked::Unresolvable(_))
    ));
}

// --------------------------------------------------------------- whole-plan checks

fn item(path: &str, origin: Origin, bytes: u64) -> PlanItem {
    PlanItem {
        path: p(path),
        origin,
        rule_id: None,
        is_dir: false,
        bytes,
        files: 1,
        size: bytes,
        mtime: 1,
        requires_closed: Vec::new(),
        mode: DeleteMode::Quarantine,
        selected: true,
        keeper: None,
    }
}

#[test]
fn plan_nested_items_are_refused() {
    let a = r"C:\Users\nano\AppData\Local\Temp\a";
    let b = r"C:\Users\nano\AppData\Local\Temp\a\b.tmp";
    let fake = Fake::default().dir(a).file(b, 3, 1);
    let mut outer = item(a, SAFE, 10);
    outer.is_dir = true;
    outer.size = 0;
    let plan = CleanPlan {
        items: vec![outer, item(b, SAFE, 3)],
    };
    let report = gate().check(&plan, &fake);
    assert_eq!(report.verdicts[0], Verdict::Allowed);
    assert_eq!(report.verdicts[1], Verdict::Blocked(Blocked::Nested));
    assert_eq!(report.allowed_bytes, 10);
}

#[test]
fn plan_duplicates_keep_the_first() {
    let fake = Fake::default().file(TMP, 3, 1);
    let plan = CleanPlan {
        items: vec![item(TMP, SAFE, 3), item(TMP, SAFE, 3)],
    };
    let report = gate().check(&plan, &fake);
    assert_eq!(report.verdicts[0], Verdict::Allowed);
    assert_eq!(report.verdicts[1], Verdict::Blocked(Blocked::Nested));
}

#[test]
fn plan_requires_closed_running_app_warns() {
    let mut fake = Fake::default().file(TMP, 3, 1);
    fake.running = vec!["chrome.exe".into()];
    let mut it = item(TMP, SAFE, 3);
    it.requires_closed = vec!["chrome.exe".into(), "msedge.exe".into()];
    let report = gate().check(&CleanPlan { items: vec![it] }, &fake);
    assert_eq!(
        report.verdicts[0],
        Verdict::Warn(vec![Warning::AppRunning(vec!["chrome.exe".into()])])
    );
    assert!(report.verdicts[0].is_allowed());
}

#[test]
fn plan_requires_closed_not_running_is_allowed() {
    let fake = Fake::default().file(TMP, 3, 1);
    let mut it = item(TMP, SAFE, 3);
    it.requires_closed = vec!["chrome.exe".into()];
    let report = gate().check(&CleanPlan { items: vec![it] }, &fake);
    assert_eq!(report.verdicts[0], Verdict::Allowed);
}

#[test]
fn plan_changed_item_is_skipped_others_go_on() {
    let fake = Fake::default().file(TMP, 4, 1).file(DL, 5, 1);
    let plan = CleanPlan {
        items: vec![item(TMP, SAFE, 3), item(DL, M, 5)],
    };
    let report = gate().check(&plan, &fake);
    assert_eq!(report.verdicts[0], Verdict::Blocked(Blocked::Changed));
    assert_eq!(report.verdicts[1], Verdict::Allowed);
    assert_eq!(report.allowed_bytes, 5);
}

#[test]
fn plan_protected_rule_item_is_blocked() {
    let fake = Fake::default().file(TMP, 3, 1);
    let report = gate().check(
        &CleanPlan {
            items: vec![item(TMP, PROTECTED, 3)],
        },
        &fake,
    );
    assert_eq!(
        report.verdicts[0],
        Verdict::Blocked(Blocked::ProtectedLevel)
    );
    assert_eq!(report.allowed_bytes, 0);
}

#[test]
fn planner_output_passes_the_nesting_check() {
    let paths = [
        r"C:\Users\nano\AppData\Local\Temp\a",
        r"C:\Users\nano\AppData\Local\Temp\a\b",
        r"C:\Users\nano\AppData\Local\Temp\a b",
        r"C:\Users\nano\AppData\Local\Temp\A\c",
    ];
    let mut fake = Fake::default();
    for path in paths {
        fake = fake.dir(path);
    }
    let candidates = paths
        .iter()
        .map(|path| {
            let mut i = item(path, SAFE, 1);
            i.is_dir = true;
            i.size = 0;
            i
        })
        .collect();
    let built = plan::build(candidates, PlanOptions::default());
    assert_eq!(built.items.len(), 2);
    let report = gate().check(&built, &fake);
    assert!(
        report.verdicts.iter().all(Verdict::is_allowed),
        "{report:?}"
    );
}

// ------------------------------------------------------- the real machine (smoke)

#[test]
fn real_machine_refuses_its_own_system_folders() {
    use nfd_platform::Platform;
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    let real = SafetyGate::new(&resolve);
    for var in ["SYSTEMROOT", "PROGRAMFILES", "HOME", "LOCALAPPDATA", "TEMP"] {
        let Some(path) = platform.known_folder(var) else {
            continue;
        };
        assert!(
            real.check_path(&path, M).is_err(),
            "{var} = {} must be refused",
            path.display()
        );
        assert!(
            real.check_path(&path, SAFE).is_err(),
            "{var} = {} must be refused for rules",
            path.display()
        );
    }
}

#[cfg(windows)]
#[test]
fn real_machine_live_probe_on_a_temp_file() {
    use nfd_core::safety::LiveProbe;
    use nfd_platform::Platform;
    let platform = nfd_platform::Native;
    let resolve = platform.resolver();
    let real = SafetyGate::new(&resolve);
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("probe.bin");
    std::fs::write(&file, b"abc").expect("write");
    let warnings = real
        .check_live(&file, SAFE, None, &LiveProbe)
        .expect("a fresh temp file is fine to clean");
    assert!(warnings.is_empty(), "{warnings:?}");
    // a junction to the Windows folder is refused as a link…
    let junction = dir.path().join("win");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(std::env::var_os("SystemRoot").expect("SystemRoot"))
        .output()
        .expect("mklink");
    if status.status.success() {
        assert_eq!(
            real.check_live(&junction, SAFE, None, &LiveProbe),
            Err(Blocked::Link)
        );
        // …and anything reached through it is judged by its real location.
        let through = junction.join("Temp");
        assert!(real.check_live(&through, M, None, &LiveProbe).is_err());
        let notepad = junction.join("notepad.exe");
        assert!(real.check_live(&notepad, SAFE, None, &LiveProbe).is_err());
        std::fs::remove_dir(&junction).expect("remove junction");
    }
}
