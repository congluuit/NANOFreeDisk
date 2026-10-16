//! Windows reads. Everything in the first half works for a standard user (handles
//! opened with no access rights, `FILE_ANY_ACCESS` IOCTLs); [`ata_smart`],
//! [`sat_smart`] and [`volume_dirty`] need administrator rights and only send the
//! read commands of [`crate::cmd`].

use std::ffi::c_void;

use nfd_hwinfo::win::Handle;
use nfd_hwinfo::win::storage;
use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceW, OPEN_EXISTING,
};
use windows_sys::Win32::Storage::IscsiDisc::{
    IOCTL_SCSI_PASS_THROUGH, SCSI_IOCTL_DATA_IN, SCSI_PASS_THROUGH,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    FSCTL_IS_VOLUME_DIRTY, IOCTL_STORAGE_PREDICT_FAILURE, IOCTL_STORAGE_QUERY_PROPERTY,
    PropertyStandardQuery, SMART_RCV_DRIVE_DATA, StorageAdapterProtocolSpecificProperty,
    StorageDeviceProperty, StorageDeviceProtocolSpecificProperty, StorageDeviceTrimProperty,
    VOLUME_IS_DIRTY,
};

use crate::cmd::{self, SmartRead};
use crate::{RawDrive, Source, Volume, ata, nvme};

/// `CTL_CODE(IOCTL_VOLUME_BASE, 0, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x0056_0000;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn open_path(path: &str, access: u32) -> Option<Handle> {
    let p = wide(path);
    // SAFETY: valid NUL-terminated path; no security attributes or template.
    let h = unsafe {
        CreateFileW(
            p.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    Handle::valid(h)
}

fn ioctl(h: HANDLE, code: u32, input: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut got = 0u32;
    // SAFETY: buffers valid for their lengths; synchronous call.
    let ok = unsafe {
        DeviceIoControl(
            h,
            code,
            input.as_ptr().cast::<c_void>(),
            input.len() as u32,
            out.as_mut_ptr().cast(),
            out.len() as u32,
            &mut got,
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(got as usize)
}

fn rd32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4)
        .map_or(0, |x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

fn query_header(property: i32) -> [u8; 12] {
    let mut q = [0u8; 12];
    q[0..4].copy_from_slice(&property.to_le_bytes());
    q[4..8].copy_from_slice(&PropertyStandardQuery.to_le_bytes());
    q
}

/// Serial number from `STORAGE_DEVICE_DESCRIPTOR` (SerialNumberOffset at 24).
fn serial(h: &Handle) -> Option<String> {
    let mut buf = vec![0u8; 1024];
    let got = ioctl(
        h.0,
        IOCTL_STORAGE_QUERY_PROPERTY,
        &query_header(StorageDeviceProperty),
        &mut buf,
    )?;
    let d = &buf[..got.min(buf.len())];
    let off = rd32(d, 24) as usize;
    if off == 0 || off >= d.len() {
        return None;
    }
    let end = d[off..]
        .iter()
        .position(|&c| c == 0)
        .map_or(d.len(), |e| off + e);
    let s = String::from_utf8_lossy(&d[off..end]).trim().to_owned();
    // some USB bridges report a run of spaces / dots; keep only printable text
    (!s.is_empty() && s.chars().any(|c| c.is_ascii_alphanumeric())).then_some(s)
}

/// `DEVICE_TRIM_DESCRIPTOR.TrimEnabled`.
fn trim(h: &Handle) -> Option<bool> {
    let mut buf = [0u8; 12];
    let got = ioctl(
        h.0,
        IOCTL_STORAGE_QUERY_PROPERTY,
        &query_header(StorageDeviceTrimProperty),
        &mut buf,
    )?;
    (got >= 9).then_some(buf[8] != 0)
}

/// One NVMe read through `StorageDeviceProtocolSpecificProperty` (a Get Log Page or
/// Identify the storage driver issues itself; nothing is passed through raw).
fn nvme_query(
    h: &Handle,
    property: i32,
    data_type: u32,
    value: u32,
    sub: u32,
    len: usize,
) -> Option<Vec<u8>> {
    // STORAGE_PROPERTY_QUERY header (8) + STORAGE_PROTOCOL_SPECIFIC_DATA (40) + data
    const SPECIFIC: usize = 8;
    const DATA: usize = SPECIFIC + 40;
    let mut buf = vec![0u8; DATA + len];
    buf[0..4].copy_from_slice(&property.to_le_bytes());
    buf[4..8].copy_from_slice(&PropertyStandardQuery.to_le_bytes());
    let s = SPECIFIC;
    buf[s..s + 4].copy_from_slice(&3u32.to_le_bytes()); // ProtocolTypeNvme
    buf[s + 4..s + 8].copy_from_slice(&data_type.to_le_bytes());
    buf[s + 8..s + 12].copy_from_slice(&value.to_le_bytes());
    buf[s + 12..s + 16].copy_from_slice(&sub.to_le_bytes());
    buf[s + 16..s + 20].copy_from_slice(&40u32.to_le_bytes()); // ProtocolDataOffset
    buf[s + 20..s + 24].copy_from_slice(&(len as u32).to_le_bytes());
    let input = buf.clone();
    let got = ioctl(h.0, IOCTL_STORAGE_QUERY_PROPERTY, &input, &mut buf)?;
    // answer: STORAGE_PROTOCOL_DATA_DESCRIPTOR { Version, Size, ProtocolSpecific }
    let offset = rd32(&buf, s + 16) as usize;
    let start = SPECIFIC + offset;
    (offset >= 40 && got >= start + len && rd32(&buf, s + 20) as usize >= len)
        .then(|| buf[start..start + len].to_vec())
}

/// NVMe log page 02h, controller-wide first; some drivers only answer one NSID.
fn nvme_log(h: &Handle) -> Option<Vec<u8>> {
    const LOG_PAGE: u32 = 2; // NVMeDataTypeLogPage
    [0u32, 0xFFFF_FFFF].into_iter().find_map(|nsid| {
        nvme_query(
            h,
            StorageDeviceProtocolSpecificProperty,
            LOG_PAGE,
            2,
            nsid,
            nvme::LOG_LEN,
        )
        .filter(|log| nvme::NvmeLog::parse(log).is_some())
    })
}

/// NVMe Identify Controller (the real serial number; Windows' descriptor reports
/// the EUI-64 for many NVMe drives).
fn nvme_identify(h: &Handle) -> Option<nvme::Identify> {
    const IDENTIFY: u32 = 1; // NVMeDataTypeIdentify
    // controller data comes from the adapter-level property
    nvme_query(
        h,
        StorageAdapterProtocolSpecificProperty,
        IDENTIFY,
        1,
        0,
        nvme::IDENTIFY_LEN,
    )
    .as_deref()
    .and_then(nvme::Identify::parse)
}

/// `IOCTL_STORAGE_PREDICT_FAILURE`: the drive's failure flag plus, for ATA drives,
/// the SMART attribute sector.
fn predict_failure(h: &Handle) -> Option<(bool, Vec<u8>)> {
    let mut buf = vec![0u8; 4 + ata::SECTOR];
    let got = ioctl(h.0, IOCTL_STORAGE_PREDICT_FAILURE, &[], &mut buf)?;
    if got < 4 {
        return None;
    }
    let flag = rd32(&buf, 0) != 0;
    let data = buf[4..].to_vec();
    Some((flag, data))
}

/// Every physical drive with what a standard user can read.
#[must_use]
pub fn raw_drives() -> Vec<RawDrive> {
    storage::native()
        .into_iter()
        .map(|d| {
            let mut raw = RawDrive {
                index: d.index,
                model: d.model,
                firmware: d.firmware,
                bus: d.bus,
                size: d.size,
                ssd: d.ssd,
                removable: d.removable,
                temperature: d.temperature_c,
                ..RawDrive::default()
            };
            if let Some(h) = storage::open(d.index) {
                raw.serial = serial(&h);
                raw.trim = trim(&h);
                let (warning, critical) = storage::temperature_limits_of(&h);
                raw.temp_warning = warning;
                raw.temp_critical = critical;
                if raw.bus == "NVMe" {
                    raw.nvme_log = nvme_log(&h);
                    if let Some(id) = nvme_identify(&h) {
                        raw.serial = Some(id.serial);
                        raw.temp_warning = raw.temp_warning.or(id.warning_temp.map(|t| t as f32));
                        raw.temp_critical =
                            raw.temp_critical.or(id.critical_temp.map(|t| t as f32));
                    }
                }
                if raw.nvme_log.is_none() {
                    if let Some((flag, data)) = predict_failure(&h) {
                        raw.predict_failure = Some(flag);
                        if !ata::parse(&data, None).is_empty() {
                            raw.smart = Some(data);
                            raw.smart_via = Some(Source::AtaDriver);
                        }
                    }
                }
            }
            raw
        })
        .collect()
}

/// Disk number and partition offset of a volume (`C:\`).
fn extent(root: &str) -> Option<(u32, u64)> {
    let letter = root.trim_end_matches('\\');
    let h = open_path(&format!(r"\\.\{letter}"), 0)?;
    let mut buf = [0u8; 64];
    let got = ioctl(h.0, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, &[], &mut buf)?;
    // VOLUME_DISK_EXTENTS { Count u32, pad, DISK_EXTENT { DiskNumber u32, pad, Start i64, Length i64 } }
    if got < 32 || rd32(&buf, 0) == 0 {
        return None;
    }
    let disk = rd32(&buf, 8);
    let start = u64::from_le_bytes(buf[16..24].try_into().ok()?);
    Some((disk, start))
}

fn cluster(root: &str) -> Option<u32> {
    let r = wide(root);
    let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    // SAFETY: valid root path and out pointers.
    let ok = unsafe { GetDiskFreeSpaceW(r.as_ptr(), &mut spc, &mut bps, &mut free, &mut total) };
    (ok != 0 && spc > 0 && bps > 0).then(|| spc.saturating_mul(bps))
}

fn reg_dword(key: &str, value: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
    };
    let (k, v) = (wide(key), wide(value));
    let mut out = 0u32;
    let mut len = 4u32;
    // SAFETY: 4-byte out buffer with its length.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut out as *mut u32).cast(),
            &mut len,
        )
    };
    (rc == 0).then_some(out)
}

/// Windows sends TRIM / UNMAP for this file system (`fsutil behavior query
/// DisableDeleteNotify`, read from the registry).
fn trim_enabled(fs: &str) -> Option<bool> {
    let value = match fs {
        "NTFS" => "DisableDeleteNotify",
        "ReFS" => "RefsDisableDeleteNotify",
        _ => return None,
    };
    // absent = default = enabled
    Some(reg_dword(r"SYSTEM\CurrentControlSet\Control\FileSystem", value).is_none_or(|v| v == 0))
}

/// `System.Volume.BitLockerProtection` from the shell (no administrator rights needed).
fn bitlocker(root: &str) -> Option<String> {
    use windows_sys::Win32::Foundation::PROPERTYKEY;
    use windows_sys::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
    use windows_sys::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    use windows_sys::Win32::UI::Shell::PropertiesSystem::{
        GPS_DEFAULT, PSGetPropertyKeyFromName, SHGetPropertyStoreFromParsingName,
    };
    use windows_sys::core::{GUID, HRESULT};

    #[repr(C)]
    struct StoreVtbl {
        query_interface: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_count: usize,
        get_at: usize,
        get_value:
            unsafe extern "system" fn(*mut c_void, *const PROPERTYKEY, *mut PROPVARIANT) -> HRESULT,
    }
    const IID_IPROPERTYSTORE: GUID = GUID::from_u128(0x886d8eeb_8cf2_4446_8d02_cdba1dbdcf99);

    // SAFETY: no reserved pointer; balanced by CoUninitialize below when it succeeded.
    let init = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
    let value = (|| {
        let name = wide("System.Volume.BitLockerProtection");
        // SAFETY: zeroed POD key filled by the call.
        let mut key: PROPERTYKEY = unsafe { std::mem::zeroed() };
        // SAFETY: valid name and out pointer.
        if unsafe { PSGetPropertyKeyFromName(name.as_ptr(), &mut key) } < 0 {
            return None;
        }
        let path = wide(root);
        let mut store: *mut c_void = std::ptr::null_mut();
        // SAFETY: valid path, IID and out pointer.
        let hr = unsafe {
            SHGetPropertyStoreFromParsingName(
                path.as_ptr(),
                std::ptr::null_mut(),
                GPS_DEFAULT,
                &IID_IPROPERTYSTORE,
                &mut store,
            )
        };
        if hr < 0 || store.is_null() {
            return None;
        }
        // SAFETY: `store` is an IPropertyStore; its first field is the vtable pointer.
        let vtbl = unsafe { &**store.cast::<*const StoreVtbl>() };
        // SAFETY: zeroed PROPVARIANT is VT_EMPTY.
        let mut pv: PROPVARIANT = unsafe { std::mem::zeroed() };
        // SAFETY: valid object, key and out PROPVARIANT.
        let hr = unsafe { (vtbl.get_value)(store, &key, &mut pv) };
        // SAFETY: PROPVARIANT starts with the VARTYPE; the 32-bit value sits at offset 8.
        let (vt, v) = unsafe {
            let p = (&pv as *const PROPVARIANT).cast::<u8>();
            (
                std::ptr::read_unaligned(p.cast::<u16>()),
                std::ptr::read_unaligned(p.add(8).cast::<i32>()),
            )
        };
        // SAFETY: clears what GetValue put there; releases our reference.
        unsafe {
            PropVariantClear(&mut pv);
            (vtbl.release)(store);
        }
        // VT_I4 = 3, VT_UI4 = 19
        (hr >= 0 && (vt == 3 || vt == 19)).then_some(v)
    })();
    if init >= 0 {
        // SAFETY: paired with the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
    Some(
        match value? {
            1 => "on",
            2 => "off",
            3 => "encrypting",
            4 => "decrypting",
            5 => "suspended",
            6 => "locked",
            8 => "waiting",
            _ => return None,
        }
        .to_owned(),
    )
}

/// Fixed and removable volumes with media.
#[must_use]
pub fn volumes() -> Vec<Volume> {
    use nfd_platform::volumes::{self, VolumeKind};
    volumes::list()
        .into_iter()
        .filter(|v| v.ready && matches!(v.kind, VolumeKind::Fixed | VolumeKind::Removable))
        .map(|v| {
            let root = v.root.to_string_lossy().into_owned();
            let ext = extent(&root);
            Volume {
                label: v.label,
                trim_enabled: trim_enabled(&v.fs),
                fs: v.fs,
                total: v.total,
                free: v.free,
                system: v.system,
                removable: v.kind == VolumeKind::Removable,
                disk: ext.map(|e| e.0),
                offset: ext.map(|e| e.1),
                aligned: ext.map(|e| e.1 % 4096 == 0),
                cluster: cluster(&root),
                bitlocker: bitlocker(&root),
                dirty: None,
                days_to_full: None,
                has_trend: false,
                root,
            }
        })
        .collect()
}

/// This process runs with an elevated token.
#[must_use]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: pseudo handle of this process; out pointer.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let Some(token) = Handle::valid(token) else {
        return false;
    };
    let mut e = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut len = 0u32;
    // SAFETY: buffer of the struct's size.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            (&mut e as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    };
    ok != 0 && e.TokenIsElevated != 0
}

fn open_rw(index: u32) -> Option<Handle> {
    open_path(
        &format!(r"\\.\PhysicalDrive{index}"),
        GENERIC_READ | GENERIC_WRITE,
    )
}

fn smart_rcv(h: &Handle, read: SmartRead) -> Option<Vec<u8>> {
    // SENDCMDINPARAMS (packed): cBufferSize, IDEREGS, bDriveNumber, 3 + 16 reserved, bBuffer[1]
    let mut input = [0u8; 33];
    input[0..4].copy_from_slice(&(ata::SECTOR as u32).to_le_bytes());
    input[4..12].copy_from_slice(&cmd::ide_regs(read, 0));
    // SENDCMDOUTPARAMS: cBufferSize, DRIVERSTATUS (12), bBuffer[512]
    let mut out = vec![0u8; 16 + ata::SECTOR];
    let got = ioctl(h.0, SMART_RCV_DRIVE_DATA, &input, &mut out)?;
    let driver_error = out[4];
    (got >= 16 + ata::SECTOR && driver_error == 0).then(|| out[16..].to_vec())
}

/// SMART READ DATA + READ THRESHOLDS through `SMART_RCV_DRIVE_DATA` (administrator).
#[must_use]
pub fn ata_smart(index: u32) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
    let h = open_rw(index)?;
    let data = smart_rcv(&h, SmartRead::Data).filter(|d| !ata::parse(d, None).is_empty())?;
    let thresholds = smart_rcv(&h, SmartRead::Thresholds);
    Some((data, thresholds))
}

#[repr(C)]
struct SptBuffers {
    spt: SCSI_PASS_THROUGH,
    sense: [u8; 32],
    data: [u8; ata::SECTOR],
}

fn sat(h: &Handle, cdb: &[u8]) -> Option<Vec<u8>> {
    // SAFETY: plain-old-data, all zeros is valid.
    let mut b: SptBuffers = unsafe { std::mem::zeroed() };
    b.spt.Length = size_of::<SCSI_PASS_THROUGH>() as u16;
    b.spt.CdbLength = cdb.len() as u8;
    b.spt.SenseInfoLength = b.sense.len() as u8;
    b.spt.DataIn = SCSI_IOCTL_DATA_IN as u8;
    b.spt.DataTransferLength = ata::SECTOR as u32;
    b.spt.TimeOutValue = 5;
    b.spt.DataBufferOffset = std::mem::offset_of!(SptBuffers, data);
    b.spt.SenseInfoOffset = std::mem::offset_of!(SptBuffers, sense) as u32;
    b.spt.Cdb[..cdb.len()].copy_from_slice(cdb);
    let mut got = 0u32;
    let size = size_of::<SptBuffers>() as u32;
    let p = (&mut b as *mut SptBuffers).cast::<c_void>();
    // SAFETY: the same buffer is input and output, valid for `size` bytes.
    let ok = unsafe {
        DeviceIoControl(
            h.0,
            IOCTL_SCSI_PASS_THROUGH,
            p,
            size,
            p,
            size,
            &mut got,
            std::ptr::null_mut(),
        )
    };
    (ok != 0 && b.spt.ScsiStatus == 0 && b.data.iter().any(|&x| x != 0)).then(|| b.data.to_vec())
}

/// SMART through a USB bridge with SCSI / ATA Translation (administrator).
/// `ATA PASS-THROUGH (16)` first, then the 12-byte form.
#[must_use]
pub fn sat_smart(index: u32) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
    let h = open_rw(index)?;
    let (data, sixteen) = match sat(&h, &cmd::sat16(SmartRead::Data)) {
        Some(d) => (d, true),
        None => (sat(&h, &cmd::sat12(SmartRead::Data))?, false),
    };
    if ata::parse(&data, None).is_empty() {
        return None;
    }
    let thresholds = if sixteen {
        sat(&h, &cmd::sat16(SmartRead::Thresholds))
    } else {
        sat(&h, &cmd::sat12(SmartRead::Thresholds))
    };
    Some((data, thresholds))
}

/// `FSCTL_IS_VOLUME_DIRTY` (administrator).
#[must_use]
pub fn volume_dirty(root: &str) -> Option<bool> {
    let letter = root.trim_end_matches('\\');
    let h = open_path(&format!(r"\\.\{letter}"), GENERIC_READ)?;
    let mut out = [0u8; 4];
    ioctl(h.0, FSCTL_IS_VOLUME_DIRTY, &[], &mut out)
        .filter(|&n| n >= 4)
        .map(|_| u32::from_le_bytes(out) & VOLUME_IS_DIRTY != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs on the developer machine and CI: every drive is listed and every NVMe
    /// drive that answers gives a plausible health log.
    #[test]
    fn reads_this_machine() {
        let drives = raw_drives();
        for d in &drives {
            if let Some(log) = d.nvme_log.as_deref() {
                let log = nvme::NvmeLog::parse(log).expect("parsed");
                assert!(log.temperature.is_none_or(|t| (0..120).contains(&t)));
            }
        }
        let vols = volumes();
        assert!(vols.iter().all(|v| v.cluster.is_none_or(|c| c >= 512)));
        if let Some(sys) = vols.iter().find(|v| v.system) {
            assert!(sys.disk.is_some(), "system volume maps to a disk");
        }
    }
}
