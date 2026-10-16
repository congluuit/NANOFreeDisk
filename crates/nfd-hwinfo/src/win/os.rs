//! Windows edition, build, memory totals, uptime.

use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows_sys::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};

use super::{reg_string, reg_u64};

const CV: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

#[derive(Debug, Clone, Default)]
pub struct OsInfo {
    pub name: String,
    pub display_version: Option<String>,
    pub build: Option<String>,
    pub edition: Option<String>,
    pub computer: Option<String>,
    pub uptime_secs: u64,
}

pub fn native() -> OsInfo {
    let build = reg_string(HKEY_LOCAL_MACHINE, CV, "CurrentBuild");
    let ubr = reg_u64(HKEY_LOCAL_MACHINE, CV, "UBR");
    let mut name =
        reg_string(HKEY_LOCAL_MACHINE, CV, "ProductName").unwrap_or_else(|| "Windows".into());
    // ProductName still says "Windows 10" on Windows 11
    if build
        .as_deref()
        .and_then(|b| b.parse::<u32>().ok())
        .is_some_and(|b| b >= 22000)
    {
        name = name.replace("Windows 10", "Windows 11");
    }
    OsInfo {
        name,
        display_version: reg_string(HKEY_LOCAL_MACHINE, CV, "DisplayVersion")
            .or_else(|| reg_string(HKEY_LOCAL_MACHINE, CV, "ReleaseId")),
        build: build.map(|b| match ubr {
            Some(u) => format!("{b}.{u}"),
            None => b,
        }),
        edition: reg_string(HKEY_LOCAL_MACHINE, CV, "EditionID"),
        computer: std::env::var("COMPUTERNAME").ok(),
        // SAFETY: no arguments.
        uptime_secs: unsafe { GetTickCount64() } / 1000,
    }
}

/// (total, available) physical memory in bytes.
pub fn memory() -> (u64, u64) {
    let mut m = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: dwLength set as required.
    if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
        return (0, 0);
    }
    (m.ullTotalPhys, m.ullAvailPhys)
}
