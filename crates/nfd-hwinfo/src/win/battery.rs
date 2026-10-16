//! Batteries through the battery class driver (`IOCTL_BATTERY_*`): design and
//! full-charge capacity (wear), cycle count, live charge / rate / voltage.

use std::ffi::c_void;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Power::{
    BATTERY_CHARGING, BATTERY_DISCHARGING, BATTERY_INFORMATION, BATTERY_POWER_ON_LINE,
    BATTERY_QUERY_INFORMATION, BATTERY_STATUS, BATTERY_WAIT_STATUS, BatteryDeviceName,
    BatteryInformation, BatteryManufactureName, GUID_DEVICE_BATTERY,
    IOCTL_BATTERY_QUERY_INFORMATION, IOCTL_BATTERY_QUERY_STATUS, IOCTL_BATTERY_QUERY_TAG,
};

use super::{Handle, from_wide};
use crate::Battery;

/// Live battery state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BatteryStatus {
    /// mWh remaining.
    pub capacity_mwh: u32,
    /// mWh at full charge.
    pub full_mwh: u32,
    /// mV.
    pub voltage_mv: Option<u32>,
    /// mW, negative while discharging.
    pub rate_mw: Option<i32>,
    pub on_ac: bool,
    pub charging: bool,
}

fn ioctl<I, O>(h: &Handle, code: u32, input: &I, out: &mut O) -> bool {
    let mut got = 0u32;
    // SAFETY: typed buffers of their own size.
    unsafe {
        DeviceIoControl(
            h.0,
            code,
            (input as *const I).cast::<c_void>(),
            size_of::<I>() as u32,
            (out as *mut O).cast(),
            size_of::<O>() as u32,
            &mut got,
            std::ptr::null_mut(),
        ) != 0
    }
}

/// Device paths of the present batteries.
fn paths() -> Vec<Vec<u16>> {
    let mut out = Vec::new();
    // SAFETY: interface enumeration, destroyed below.
    let set = unsafe {
        SetupDiGetClassDevsW(
            &GUID_DEVICE_BATTERY,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if set == -1 || set == 0 {
        return out;
    }
    for i in 0..8 {
        let mut itf = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: cbSize set.
        if unsafe {
            SetupDiEnumDeviceInterfaces(set, std::ptr::null(), &GUID_DEVICE_BATTERY, i, &mut itf)
        } == 0
        {
            break;
        }
        // SP_DEVICE_INTERFACE_DETAIL_DATA_W: cbSize (8 on x64) + path
        let mut buf = vec![0u32; 260];
        buf[0] = if cfg!(target_pointer_width = "64") {
            8
        } else {
            6
        };
        let mut need = 0u32;
        // SAFETY: buffer of (len*4) bytes, cbSize set as the API requires.
        let ok = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                set,
                &itf,
                buf.as_mut_ptr().cast(),
                (buf.len() * 4) as u32,
                &mut need,
                std::ptr::null_mut(),
            )
        };
        if ok != 0 {
            // SAFETY: reinterpret the u32 buffer as UTF-16 after the 4-byte cbSize.
            let wide = unsafe {
                std::slice::from_raw_parts(buf.as_ptr().cast::<u16>().add(2), buf.len() * 2 - 2)
            };
            let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
            let mut p = wide[..end].to_vec();
            p.push(0);
            out.push(p);
        }
    }
    // SAFETY: created above.
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    out
}

fn open(path: &[u16]) -> Option<(Handle, u32)> {
    // SAFETY: NUL-terminated device path.
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    let h = Handle::valid(h)?;
    let wait = 0u32;
    let mut tag = 0u32;
    if !ioctl(&h, IOCTL_BATTERY_QUERY_TAG, &wait, &mut tag) || tag == 0 {
        return None;
    }
    Some((h, tag))
}

fn query_string(h: &Handle, tag: u32, level: i32) -> Option<String> {
    let q = BATTERY_QUERY_INFORMATION {
        BatteryTag: tag,
        InformationLevel: level,
        AtRate: 0,
    };
    let mut buf = [0u16; 128];
    ioctl(h, IOCTL_BATTERY_QUERY_INFORMATION, &q, &mut buf)
        .then(|| from_wide(&buf))
        .filter(|s| !s.is_empty())
}

fn status_of(h: &Handle, tag: u32, full: u32) -> Option<BatteryStatus> {
    let w = BATTERY_WAIT_STATUS {
        BatteryTag: tag,
        ..Default::default()
    };
    let mut s = BATTERY_STATUS::default();
    if !ioctl(h, IOCTL_BATTERY_QUERY_STATUS, &w, &mut s) {
        return None;
    }
    const UNKNOWN: u32 = 0xFFFF_FFFF;
    Some(BatteryStatus {
        capacity_mwh: if s.Capacity == UNKNOWN { 0 } else { s.Capacity },
        full_mwh: full,
        voltage_mv: (s.Voltage != UNKNOWN && s.Voltage > 0).then_some(s.Voltage),
        rate_mw: (s.Rate != i32::MIN).then_some(s.Rate),
        on_ac: s.PowerState & BATTERY_POWER_ON_LINE != 0,
        charging: s.PowerState & BATTERY_CHARGING != 0 && s.PowerState & BATTERY_DISCHARGING == 0,
    })
}

fn info_of(h: &Handle, tag: u32) -> Option<BATTERY_INFORMATION> {
    let q = BATTERY_QUERY_INFORMATION {
        BatteryTag: tag,
        InformationLevel: BatteryInformation,
        AtRate: 0,
    };
    let mut info = BATTERY_INFORMATION::default();
    ioctl(h, IOCTL_BATTERY_QUERY_INFORMATION, &q, &mut info).then_some(info)
}

pub fn native() -> Vec<Battery> {
    let mut out = Vec::new();
    for path in paths() {
        let Some((h, tag)) = open(&path) else {
            continue;
        };
        let Some(info) = info_of(&h, tag) else {
            continue;
        };
        let chem = String::from_utf8_lossy(&info.Chemistry)
            .trim_matches(char::from(0))
            .trim()
            .to_owned();
        let status = status_of(&h, tag, info.FullChargedCapacity);
        out.push(Battery {
            name: query_string(&h, tag, BatteryDeviceName),
            manufacturer: query_string(&h, tag, BatteryManufactureName),
            chemistry: (!chem.is_empty()).then_some(chem),
            design_mwh: info.DesignedCapacity,
            full_mwh: info.FullChargedCapacity,
            cycles: (info.CycleCount > 0).then_some(info.CycleCount),
            charge_percent: status.filter(|s| s.full_mwh > 0).map(|s| {
                (f64::from(s.capacity_mwh) * 100.0 / f64::from(s.full_mwh)).min(100.0) as f32
            }),
        });
    }
    out
}

/// Live status of every battery (for the sensor tree).
pub fn status() -> Vec<BatteryStatus> {
    paths()
        .iter()
        .filter_map(|p| {
            let (h, tag) = open(p)?;
            let full = info_of(&h, tag)?.FullChargedCapacity;
            status_of(&h, tag, full)
        })
        .collect()
}
