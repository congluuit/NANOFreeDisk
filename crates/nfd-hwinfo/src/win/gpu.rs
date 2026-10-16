//! Display adapters: present devices of the display class (SetupAPI) and their
//! driver keys — name, vendor, VRAM, driver version and date. No COM, no DXGI
//! factory; live GPU data (load, temperature) comes from D3DKMT in `nfd-sensors`.

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DICS_FLAG_GLOBAL, DIGCF_PRESENT, DIREG_DRV, GUID_DEVCLASS_DISPLAY, SP_DEVINFO_DATA,
    SPDRP_LOCATION_INFORMATION, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW, SetupDiOpenDevRegKey,
};
use windows_sys::Win32::System::Registry::{HKEY, KEY_READ, RegCloseKey};

use super::{from_wide, reg_string, reg_u64};
use crate::Gpu;

/// Iterate the present devices of a setup class, handing each one's driver key.
pub(crate) fn for_each_driver_key(
    class: &windows_sys::core::GUID,
    mut f: impl FnMut(HKEY, Option<String>),
) {
    // SAFETY: plain enumeration; the list is destroyed below.
    let set = unsafe {
        SetupDiGetClassDevsW(class, std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT)
    };
    if set == -1 || set == 0 {
        return;
    }
    for i in 0..64 {
        let mut dev = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: cbSize set.
        if unsafe { SetupDiEnumDeviceInfo(set, i, &mut dev) } == 0 {
            break;
        }
        let mut loc = [0u16; 256];
        // SAFETY: buffer size in bytes.
        let has_loc = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set,
                &dev,
                SPDRP_LOCATION_INFORMATION,
                std::ptr::null_mut(),
                loc.as_mut_ptr().cast(),
                (loc.len() * 2) as u32,
                std::ptr::null_mut(),
            )
        } != 0;
        // SAFETY: read-only open of the driver key.
        let key =
            unsafe { SetupDiOpenDevRegKey(set, &dev, DICS_FLAG_GLOBAL, 0, DIREG_DRV, KEY_READ) };
        if key.is_null() || key as isize == -1 {
            continue;
        }
        f(
            key,
            has_loc.then(|| from_wide(&loc)).filter(|s| !s.is_empty()),
        );
        // SAFETY: opened above.
        unsafe { RegCloseKey(key) };
    }
    // SAFETY: created above.
    unsafe { SetupDiDestroyDeviceInfoList(set) };
}

pub fn native() -> Vec<Gpu> {
    let mut out = Vec::new();
    for_each_driver_key(&GUID_DEVCLASS_DISPLAY, |key, location| {
        let Some(name) = reg_string(key, "", "DriverDesc") else {
            return;
        };
        let device_id = reg_string(key, "", "MatchingDeviceId").unwrap_or_default();
        let vram = reg_u64(key, "", "HardwareInformation.qwMemorySize")
            .or_else(|| reg_u64(key, "", "HardwareInformation.MemorySize").map(|v| v & 0xFFFF_FFFF))
            .filter(|&v| v > 0);
        out.push(Gpu {
            vendor: pci_vendor(&device_id)
                .map(str::to_owned)
                .or_else(|| reg_string(key, "", "ProviderName")),
            name,
            vram,
            driver_version: reg_string(key, "", "DriverVersion"),
            driver_date: reg_string(key, "", "DriverDate"),
            location,
        });
    });
    out
}

/// Vendor from a hardware id such as `pci\ven_10de&dev_2820`.
pub fn pci_vendor(id: &str) -> Option<&'static str> {
    let lower = id.to_ascii_lowercase();
    let ven = lower.split("ven_").nth(1)?.get(..4)?;
    Some(match ven {
        "10de" => "NVIDIA",
        "1002" | "1022" => "AMD",
        "8086" => "Intel",
        "1414" => "Microsoft",
        "5143" => "Qualcomm",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn vendors() {
        assert_eq!(super::pci_vendor("pci\\ven_10de&dev_2820"), Some("NVIDIA"));
        assert_eq!(super::pci_vendor("PCI\\VEN_8086&DEV_A788"), Some("Intel"));
        assert_eq!(super::pci_vendor("root\\basicdisplay"), None);
    }
}
