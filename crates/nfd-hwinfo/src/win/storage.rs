//! Physical disks through `IOCTL_STORAGE_QUERY_PROPERTY` on `\\.\PhysicalDriveN`,
//! opened with no access rights (enough for these queries, no admin needed).

use std::ffi::c_void;

use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery,
    STORAGE_PROPERTY_QUERY, StorageDeviceProperty, StorageDeviceSeekPenaltyProperty,
    StorageDeviceTemperatureProperty,
};

use super::{Handle, wide};
use crate::Disk;

/// Open `\\.\PhysicalDrive{index}` for queries only.
pub fn open(index: u32) -> Option<Handle> {
    let path = wide(&format!(r"\\.\PhysicalDrive{index}"));
    // SAFETY: valid path; zero access rights.
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    Handle::valid(h)
}

fn ioctl(h: &Handle, code: u32, input: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut got = 0u32;
    // SAFETY: buffers valid for their lengths; synchronous call.
    let ok = unsafe {
        DeviceIoControl(
            h.0,
            code,
            input.as_ptr() as *const c_void,
            input.len() as u32,
            out.as_mut_ptr().cast(),
            out.len() as u32,
            &mut got,
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(got as usize)
}

fn query(h: &Handle, property: i32, out: &mut [u8]) -> Option<usize> {
    let q = STORAGE_PROPERTY_QUERY {
        PropertyId: property,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    // SAFETY: plain-old-data viewed as bytes.
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (&q as *const STORAGE_PROPERTY_QUERY).cast::<u8>(),
            size_of::<STORAGE_PROPERTY_QUERY>(),
        )
    };
    ioctl(h, IOCTL_STORAGE_QUERY_PROPERTY, bytes, out)
}

fn rd32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4)
        .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
        .unwrap_or(0)
}

fn cstr_at(b: &[u8], off: u32) -> String {
    let off = off as usize;
    if off == 0 || off >= b.len() {
        return String::new();
    }
    let end = b[off..]
        .iter()
        .position(|&c| c == 0)
        .map_or(b.len(), |e| off + e);
    String::from_utf8_lossy(&b[off..end]).trim().to_owned()
}

/// Bus name for `STORAGE_BUS_TYPE`.
pub fn bus_name(bus: u32) -> &'static str {
    match bus {
        1 => "SCSI",
        2 => "ATAPI",
        3 => "ATA",
        4 => "IEEE 1394",
        5 => "SSA",
        6 => "Fibre Channel",
        7 => "USB",
        8 => "RAID",
        9 => "iSCSI",
        10 => "SAS",
        11 => "SATA",
        12 => "SD",
        13 => "MMC",
        14 | 15 => "Virtual",
        16 => "Storage Spaces",
        17 => "NVMe",
        18 => "SCM",
        19 => "UFS",
        _ => "Unknown",
    }
}

fn disk(index: u32, h: &Handle) -> Option<Disk> {
    let mut buf = vec![0u8; 1024];
    let got = query(h, StorageDeviceProperty, &mut buf)?;
    let d = &buf[..got.min(buf.len())];
    // STORAGE_DEVICE_DESCRIPTOR: Version, Size, DeviceType, Modifier, Removable, Queueing,
    // VendorIdOffset@12, ProductIdOffset@16, RevisionOffset@20, SerialOffset@24, BusType@28
    let removable = d.get(10).copied().unwrap_or(0) != 0;
    let vendor = cstr_at(d, rd32(d, 12));
    let product = cstr_at(d, rd32(d, 16));
    let firmware = cstr_at(d, rd32(d, 20));
    let bus = rd32(d, 28);
    let model = if vendor.is_empty() || product.starts_with(&vendor) {
        product
    } else {
        format!("{vendor} {product}")
    };
    let mut sp = [0u8; 12];
    let ssd = query(h, StorageDeviceSeekPenaltyProperty, &mut sp)
        .filter(|&n| n >= 9)
        .map(|_| sp[8] == 0);
    let mut geo = [0u8; 256];
    let size = ioctl(h, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, &[], &mut geo)
        .filter(|&n| n >= 32)
        .map(|_| u64::from_le_bytes(geo[24..32].try_into().unwrap_or([0; 8])));
    Some(Disk {
        index,
        model,
        firmware: (!firmware.is_empty()).then_some(firmware),
        bus: bus_name(bus).to_owned(),
        size,
        ssd: if bus == 17 { Some(true) } else { ssd },
        removable,
        temperature_c: temperature_of(h),
    })
}

pub fn temperature_of(h: &Handle) -> Option<f32> {
    let mut buf = [0u8; 256];
    let got = query(h, StorageDeviceTemperatureProperty, &mut buf)?;
    // header: Version, Size, Critical i16, Warning i16, InfoCount u16, Reserved, Reserved1[2]
    // then STORAGE_TEMPERATURE_INFO { Index u16, Temperature i16, … } at offset 24
    if got < 28 || u16::from_le_bytes([buf[12], buf[13]]) == 0 {
        return None;
    }
    let t = i16::from_le_bytes([buf[26], buf[27]]);
    // drivers report 0 or nonsense when the device has no sensor
    (t > 0 && t < 150).then_some(f32::from(t))
}

/// Current temperature of disk `index`, °C.
pub fn temperature(index: u32) -> Option<f32> {
    temperature_of(&open(index)?)
}

pub fn native() -> Vec<Disk> {
    let mut out = Vec::new();
    let mut misses = 0;
    for i in 0..64 {
        match open(i) {
            Some(h) => {
                misses = 0;
                if let Some(d) = disk(i, &h) {
                    out.push(d);
                }
            }
            None => {
                misses += 1;
                if misses > 8 {
                    break;
                }
            }
        }
    }
    out
}
