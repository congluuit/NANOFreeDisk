//! Windows plumbing for the setup: known folders, elevation, shortcuts, WebView2,
//! processes. Registry access goes through `winreg` in [`crate::install`].

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, IPersistFile,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcessToken, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{
    FOLDERID_CommonPrograms, FOLDERID_Desktop, FOLDERID_LocalAppData, FOLDERID_ProgramFiles,
    FOLDERID_Programs, FOLDERID_PublicDesktop, IShellLinkW, KF_FLAG_DEFAULT,
    SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, SHGetKnownFolderPath, ShellExecuteExW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{GUID, Interface, PCWSTR};

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

fn known(id: &GUID) -> Option<PathBuf> {
    // SAFETY: valid GUID pointer; the returned buffer is freed with CoTaskMemFree.
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

/// `%LOCALAPPDATA%\Programs\NANOFreeDisk` or `%ProgramFiles%\NANOFreeDisk`.
#[must_use]
pub fn default_dir(all_users: bool) -> PathBuf {
    let base = if all_users {
        known(&FOLDERID_ProgramFiles).unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
    } else {
        known(&FOLDERID_LocalAppData)
            .map(|d| d.join("Programs"))
            .unwrap_or_else(|| PathBuf::from(r"C:\NANOFreeDisk"))
    };
    base.join(crate::PRODUCT)
}

/// Start menu "Programs" folder for this user or for all users.
#[must_use]
pub fn programs_dir(all_users: bool) -> Option<PathBuf> {
    known(if all_users {
        &FOLDERID_CommonPrograms
    } else {
        &FOLDERID_Programs
    })
}

/// Desktop folder for this user or the public desktop.
#[must_use]
pub fn desktop_dir(all_users: bool) -> Option<PathBuf> {
    known(if all_users {
        &FOLDERID_PublicDesktop
    } else {
        &FOLDERID_Desktop
    })
}

/// The process runs with an elevated (administrator) token.
#[must_use]
pub fn is_elevated() -> bool {
    // SAFETY: plain token query on our own process; the handle is closed.
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut info = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut info).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && info.TokenIsElevated != 0
    }
}

/// Quote one argument for the Windows command line (CommandLineToArgvW rules).
#[must_use]
pub fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_owned();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// Start `exe` with `params` through the shell (`verb` = `runas` asks for UAC) and,
/// when `wait`, wait for it and return its exit code.
///
/// # Errors
/// The user refused UAC (`ERROR_CANCELLED`) or the program could not start.
pub fn shell_execute(exe: &Path, params: &str, verb: &str, wait: bool) -> io::Result<u32> {
    let (exe_w, params_w, verb_w) = (wide(exe), wide(params), wide(verb));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: HWND::default(),
        lpVerb: PCWSTR(verb_w.as_ptr()),
        lpFile: PCWSTR(exe_w.as_ptr()),
        lpParameters: PCWSTR(params_w.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: every string outlives the call; the process handle is closed below.
    unsafe {
        ShellExecuteExW(&mut info)
            .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xffff))?;
        let mut code = 0u32;
        if !info.hProcess.is_invalid() {
            if wait {
                WaitForSingleObject(info.hProcess, INFINITE);
                let _ = GetExitCodeProcess(info.hProcess, &mut code);
            }
            let _ = CloseHandle(info.hProcess);
        }
        Ok(code)
    }
}

/// Create (or replace) a `.lnk` at `link` pointing to `target`.
///
/// # Errors
/// COM or file-system failure.
pub fn create_shortcut(link: &Path, target: &Path, description: &str) -> io::Result<()> {
    let to_io = |e: windows::core::Error| io::Error::other(e.message());
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // SAFETY: COM is initialised for this thread; every string outlives its call.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let sl: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(to_io)?;
        let target_w = wide(target);
        sl.SetPath(PCWSTR(target_w.as_ptr())).map_err(to_io)?;
        if let Some(dir) = target.parent() {
            let dir_w = wide(dir);
            sl.SetWorkingDirectory(PCWSTR(dir_w.as_ptr()))
                .map_err(to_io)?;
        }
        sl.SetIconLocation(PCWSTR(target_w.as_ptr()), 0)
            .map_err(to_io)?;
        let desc_w = wide(description);
        sl.SetDescription(PCWSTR(desc_w.as_ptr())).map_err(to_io)?;
        let pf: IPersistFile = sl.cast().map_err(to_io)?;
        let link_w = wide(link);
        pf.Save(PCWSTR(link_w.as_ptr()), true).map_err(to_io)?;
    }
    Ok(())
}

/// Lower-case names of running executables whose image lives in `dir`.
#[must_use]
pub fn running_in(dir: &Path) -> Vec<String> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    let dir = dir.to_string_lossy().to_lowercase();
    let dir = dir.trim_end_matches('\\').to_owned() + "\\";
    let me = std::process::id();
    let mut out = Vec::new();
    // SAFETY: snapshot iteration with properly sized structs; handles are closed.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut e = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut more = Process32FirstW(snap, &mut e).is_ok();
        while more {
            if e.th32ProcessID != me {
                if let Ok(h) =
                    OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, e.th32ProcessID)
                {
                    let mut buf = [0u16; 1024];
                    let mut len = buf.len() as u32;
                    if QueryFullProcessImageNameW(
                        h,
                        PROCESS_NAME_WIN32,
                        windows::core::PWSTR(buf.as_mut_ptr()),
                        &mut len,
                    )
                    .is_ok()
                    {
                        let path = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
                        if let Some(name) = path.strip_prefix(&dir) {
                            if !name.contains('\\') {
                                out.push(name.to_owned());
                            }
                        }
                    }
                    let _ = CloseHandle(h);
                }
            }
            more = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// WebView2 Evergreen runtime client id (EdgeUpdate).
const WEBVIEW2_CLIENT: &str =
    r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
/// Microsoft's Evergreen bootstrapper.
pub const WEBVIEW2_BOOTSTRAPPER: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

/// Installed WebView2 runtime version, if any.
#[must_use]
pub fn webview2_version() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY};
    let candidates = [
        (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY),
        (HKEY_LOCAL_MACHINE, KEY_READ),
        (HKEY_CURRENT_USER, KEY_READ),
    ];
    for (root, flags) in candidates {
        let key = RegKey::predef(root)
            .open_subkey_with_flags(format!(r"SOFTWARE\{WEBVIEW2_CLIENT}"), flags)
            .ok();
        let v: Option<String> = key.and_then(|k| k.get_value("pv").ok());
        if let Some(v) = v.filter(|v| !v.is_empty() && v != "0.0.0.0") {
            return Some(v);
        }
    }
    None
}

/// Download Microsoft's WebView2 bootstrapper, check its Authenticode signature
/// (valid chain, signed by Microsoft Corporation) and run it silently.
///
/// # Errors
/// No network, a bad signature, or the bootstrapper failed (its exit code).
pub fn install_webview2() -> io::Result<()> {
    use windows::Win32::System::Com::Urlmon::URLDownloadToFileW;
    let dir = std::env::temp_dir().join(format!("nfd-webview2-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("MicrosoftEdgeWebview2Setup.exe");
    let (url_w, file_w) = (wide(WEBVIEW2_BOOTSTRAPPER), wide(&file));
    // SAFETY: both strings outlive the call; no callback.
    let res = unsafe {
        URLDownloadToFileW(
            None,
            PCWSTR(url_w.as_ptr()),
            PCWSTR(file_w.as_ptr()),
            0,
            None,
        )
    };
    let result = match res {
        Err(e) => Err(io::Error::other(format!("download: {}", e.message()))),
        Ok(()) => match signer(&file) {
            Some(s) if s == "Microsoft Corporation" => {
                let code = run_hidden(&file, &["/silent", "/install"])?;
                if code == 0 {
                    Ok(())
                } else {
                    Err(io::Error::other(format!("bootstrapper exit code {code}")))
                }
            }
            Some(s) => Err(io::Error::other(format!("unexpected signer: {s}"))),
            None => Err(io::Error::other("invalid signature")),
        },
    };
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Run `exe args…` without a console window and wait for it.
///
/// # Errors
/// The program could not start.
pub fn run_hidden(exe: &Path, args: &[&str]) -> io::Result<u32> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = std::process::Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    Ok(status.code().map_or(1, |c| c as u32))
}

/// The signer's display name when `file` has a valid Authenticode signature.
#[must_use]
pub fn signer(file: &Path) -> Option<String> {
    use windows::Win32::Security::Cryptography::{
        CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW,
    };
    use windows::Win32::Security::WinTrust::{
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
        WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WTD_UI_NONE, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
    };
    let path_w = wide(file);
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(path_w.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: structures are fully initialised and live across both calls; the
    // state is released with WTD_STATEACTION_CLOSE.
    unsafe {
        let status = WinVerifyTrust(HWND(-1isize as _), &mut action, (&raw mut data).cast());
        let mut name = None;
        if status == 0 {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if !prov.is_null() {
                let sgnr = WTHelperGetProvSignerFromChain(prov, 0, false, 0);
                if !sgnr.is_null() && (*sgnr).csCertChain > 0 && !(*sgnr).pasCertChain.is_null() {
                    let cert = (*(*sgnr).pasCertChain).pCert;
                    let mut buf = [0u16; 256];
                    let n = CertGetNameStringW(
                        cert,
                        CERT_NAME_SIMPLE_DISPLAY_TYPE,
                        0,
                        None,
                        Some(&mut buf),
                    );
                    if n > 1 {
                        name = Some(String::from_utf16_lossy(&buf[..n as usize - 1]));
                    }
                }
            }
        }
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(HWND(-1isize as _), &mut action, (&raw mut data).cast());
        name
    }
}
