//! NVIDIA Management Library, loaded at run time from System32 only (it ships with
//! the driver; nothing is bundled). Read-only queries: temperature, power, clocks,
//! fan duty, memory. Never calls any `nvmlDeviceSet*`.

use std::ffi::{c_char, c_uint, c_void};

use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};

type Device = *mut c_void;
type Ret = i32;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct Memory {
    pub total: u64,
    pub free: u64,
    pub used: u64,
}

#[derive(Debug)]
pub struct Nvml {
    get_temperature: unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> Ret,
    get_power: unsafe extern "C" fn(Device, *mut c_uint) -> Ret,
    get_power_limit: unsafe extern "C" fn(Device, *mut c_uint) -> Ret,
    get_clock: unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> Ret,
    get_fan: unsafe extern "C" fn(Device, *mut c_uint) -> Ret,
    get_memory: unsafe extern "C" fn(Device, *mut Memory) -> Ret,
    pub devices: Vec<(String, NvDevice)>,
}

/// An NVML device handle (valid for the process lifetime once NVML is initialized).
#[derive(Debug, Clone, Copy)]
pub struct NvDevice(Device);

// SAFETY: NVML is thread-safe; handles are plain identifiers.
unsafe impl Send for NvDevice {}
unsafe impl Send for Nvml {}

macro_rules! sym {
    ($lib:expr, $name:literal) => {{
        // SAFETY: NUL-terminated symbol name; the transmute target matches the
        // documented NVML prototype.
        let p = unsafe { GetProcAddress($lib, concat!($name, "\0").as_ptr()) }?;
        // the target type is the annotated field / binding
        #[allow(clippy::missing_transmute_annotations)]
        unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, _>(p)
        }
    }};
}

impl Nvml {
    /// `None` when there is no NVIDIA driver (or NVML fails to start).
    pub fn load() -> Option<Self> {
        let name: Vec<u16> = "nvml.dll\0".encode_utf16().collect();
        // SAFETY: System32-only search, so a planted nvml.dll elsewhere is never loaded.
        let lib = unsafe {
            LoadLibraryExW(
                name.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if lib.is_null() {
            return None;
        }
        let init: unsafe extern "C" fn() -> Ret = sym!(lib, "nvmlInit_v2");
        let count: unsafe extern "C" fn(*mut c_uint) -> Ret = sym!(lib, "nvmlDeviceGetCount_v2");
        let by_index: unsafe extern "C" fn(c_uint, *mut Device) -> Ret =
            sym!(lib, "nvmlDeviceGetHandleByIndex_v2");
        let get_name: unsafe extern "C" fn(Device, *mut c_char, c_uint) -> Ret =
            sym!(lib, "nvmlDeviceGetName");
        let nv = Self {
            get_temperature: sym!(lib, "nvmlDeviceGetTemperature"),
            get_power: sym!(lib, "nvmlDeviceGetPowerUsage"),
            get_power_limit: sym!(lib, "nvmlDeviceGetEnforcedPowerLimit"),
            get_clock: sym!(lib, "nvmlDeviceGetClockInfo"),
            get_fan: sym!(lib, "nvmlDeviceGetFanSpeed"),
            get_memory: sym!(lib, "nvmlDeviceGetMemoryInfo"),
            devices: Vec::new(),
        };
        // SAFETY: documented NVML calls with valid out-pointers.
        unsafe {
            if init() != 0 {
                return None;
            }
            let mut n = 0;
            if count(&mut n) != 0 {
                return None;
            }
            let mut nv = nv;
            for i in 0..n.min(16) {
                let mut d = std::ptr::null_mut();
                if by_index(i, &mut d) != 0 {
                    continue;
                }
                let mut buf = [0 as c_char; 96];
                if get_name(d, buf.as_mut_ptr(), buf.len() as c_uint) != 0 {
                    continue;
                }
                let bytes: Vec<u8> = buf
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8)
                    .collect();
                nv.devices
                    .push((String::from_utf8_lossy(&bytes).into_owned(), NvDevice(d)));
            }
            Some(nv)
        }
    }

    fn uint(&self, f: impl FnOnce(*mut c_uint) -> Ret) -> Option<u32> {
        let mut v = 0;
        (f(&mut v) == 0).then_some(v)
    }

    /// °C
    pub fn temperature(&self, d: NvDevice) -> Option<f64> {
        // SAFETY: valid handle, out-pointer.
        self.uint(|p| unsafe { (self.get_temperature)(d.0, 0, p) })
            .map(f64::from)
    }

    /// W
    pub fn power(&self, d: NvDevice) -> Option<f64> {
        // SAFETY: as above.
        self.uint(|p| unsafe { (self.get_power)(d.0, p) })
            .map(|mw| f64::from(mw) / 1000.0)
    }

    /// W
    pub fn power_limit(&self, d: NvDevice) -> Option<f64> {
        // SAFETY: as above.
        self.uint(|p| unsafe { (self.get_power_limit)(d.0, p) })
            .map(|mw| f64::from(mw) / 1000.0)
    }

    /// MHz; `kind` 0 = graphics, 2 = memory.
    pub fn clock(&self, d: NvDevice, kind: u32) -> Option<f64> {
        // SAFETY: as above.
        self.uint(|p| unsafe { (self.get_clock)(d.0, kind, p) })
            .map(f64::from)
    }

    /// % of max fan speed.
    pub fn fan(&self, d: NvDevice) -> Option<f64> {
        // SAFETY: as above.
        self.uint(|p| unsafe { (self.get_fan)(d.0, p) })
            .map(f64::from)
    }

    pub fn memory(&self, d: NvDevice) -> Option<Memory> {
        let mut m = Memory::default();
        // SAFETY: as above.
        (unsafe { (self.get_memory)(d.0, &mut m) } == 0).then_some(m)
    }
}
