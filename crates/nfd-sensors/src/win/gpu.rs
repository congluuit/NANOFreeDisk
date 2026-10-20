//! GPUs the way Task Manager reads them: D3DKMT adapter / node performance data
//! (WDDM 2.4+: temperature, fan, clocks), PDH `GPU Engine` / `GPU Adapter Memory`
//! (load per engine, VRAM in use), plus NVML for NVIDIA power and exact clocks.
//!
//! A hybrid laptop's discrete GPU sleeps (D3) when idle. Querying the driver would
//! wake it and cost battery, so its device power state is checked first (SetupAPI,
//! passive) and only the passive PDH counters are read while it sleeps.

use std::ffi::c_void;

use windows_sys::Wdk::Graphics::Direct3D::{
    D3DKMT_ADAPTER_PERFDATA, D3DKMT_ADAPTER_PERFDATACAPS, D3DKMT_ADAPTERINFO,
    D3DKMT_ADAPTERREGISTRYINFO, D3DKMT_CLOSEADAPTER, D3DKMT_ENUMADAPTERS2, D3DKMT_NODE_PERFDATA,
    D3DKMT_QUERYADAPTERINFO, D3DKMT_SEGMENTSIZEINFO, D3DKMTCloseAdapter, D3DKMTEnumAdapters2,
    D3DKMTQueryAdapterInfo, KMTQAITYPE_ADAPTERPERFDATA, KMTQAITYPE_ADAPTERPERFDATA_CAPS,
    KMTQAITYPE_ADAPTERREGISTRYINFO, KMTQAITYPE_ADAPTERTYPE, KMTQAITYPE_GETSEGMENTSIZE,
    KMTQAITYPE_NODEPERFDATA,
};
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_PRESENT, GUID_DEVCLASS_DISPLAY, HDEVINFO, SP_DEVINFO_DATA, SPDRP_HARDWAREID,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceRegistryPropertyW,
};

use super::nvml::{NvDevice, Nvml};
use super::pdh::{Counter, Query};
use crate::{Device, DeviceKind, Discovery, Kind, Note, SensorInfo, SensorProvider};

/// `SPDRP_DEVICE_POWER_DATA` (CM_POWER_DATA).
const SPDRP_DEVICE_POWER_DATA: u32 = 0x1E;

fn query<T>(adapter: u32, kind: i32, data: &mut T) -> bool {
    let mut q = D3DKMT_QUERYADAPTERINFO {
        hAdapter: adapter,
        Type: kind,
        pPrivateDriverData: (data as *mut T).cast::<c_void>(),
        PrivateDriverDataSize: size_of::<T>() as u32,
    };
    // SAFETY: the private buffer is a T of the size passed.
    unsafe { D3DKMTQueryAdapterInfo(&mut q) >= 0 }
}

fn wstr(b: &[u16]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..end]).trim().to_owned()
}

/// PCI vendor guessed from the adapter name.
fn vendor_of(name: &str) -> Option<&'static str> {
    let l = name.to_ascii_lowercase();
    if l.contains("nvidia") || l.contains("geforce") || l.contains("quadro") {
        Some("10de")
    } else if l.contains("amd") || l.contains("radeon") {
        Some("1002")
    } else if l.contains("intel") {
        Some("8086")
    } else {
        None
    }
}

/// PDH's spelling of an adapter LUID.
pub fn luid_key(high: i32, low: u32) -> String {
    format!("luid_0x{:08x}_0x{:08x}", high as u32, low)
}

/// Engine group of a PDH `engtype_…` suffix.
pub fn engine_group(engtype: &str) -> Option<usize> {
    let e = engtype.to_ascii_lowercase();
    Some(if e.starts_with("3d") || e.starts_with("graphics") {
        0
    } else if e.starts_with("copy") {
        1
    } else if e.starts_with("videodecode") {
        2
    } else if e.starts_with("videoencode") {
        3
    } else if e.starts_with("compute") || e.starts_with("cuda") {
        4
    } else {
        return None;
    })
}

const GROUPS: [&str; 5] = [
    "gpu-load-3d",
    "gpu-load-copy",
    "gpu-load-decode",
    "gpu-load-encode",
    "gpu-load-compute",
];

/// Per-adapter load: for each engine (`…_eng_N_engtype_X`) the sum over processes,
/// then the busiest engine per group and overall — Task Manager's numbers.
pub fn engine_loads(items: &[(String, f64)], key: &str) -> ([Option<f64>; 5], Option<f64>) {
    let mut engines: Vec<(String, usize, f64)> = Vec::new();
    for (name, v) in items {
        let lower = name.to_ascii_lowercase();
        if !lower.contains(key) {
            continue;
        }
        let Some(eng) = lower.split("_eng_").nth(1) else {
            continue;
        };
        let Some(group) = eng.split("_engtype_").nth(1).and_then(engine_group) else {
            continue;
        };
        let engine = eng.to_owned();
        match engines.iter_mut().find(|e| e.0 == engine) {
            Some(e) => e.2 += v,
            None => engines.push((engine, group, *v)),
        }
    }
    let mut groups = [None; 5];
    let mut all: Option<f64> = None;
    for (_, g, v) in &engines {
        let v = v.min(100.0);
        groups[*g] = Some(groups[*g].map_or(v, |x: f64| x.max(v)));
        all = Some(all.map_or(v, |x| x.max(v)));
    }
    (groups, all)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Src {
    Load,
    Group(usize),
    MemUsed,
    MemShared,
    MemTotal,
    MemLoad,
    KmtTemp,
    KmtFan,
    KmtMemClock,
    KmtCoreClock,
    NvTemp,
    NvPower,
    NvPowerLimit,
    NvCoreClock,
    NvMemClock,
    NvFan,
}

struct Gpu {
    id: String,
    name: String,
    handle: u32,
    key: String,
    vram: u64,
    /// Discrete GPU whose PnP power state gates the driver queries.
    power_dev: Option<SP_DEVINFO_DATA>,
    /// NVIDIA with nvml.dll installed: NVML sensors, matched by name once loaded.
    nvidia: bool,
    nv: Option<NvDevice>,
    asleep: bool,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("asleep", &self.asleep)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct GpuProvider {
    gpus: Vec<Gpu>,
    sensors: Vec<(usize, Src)>,
    pdh: Option<(Query, Counter, Option<Counter>, Option<Counter>)>,
    nvml: Option<Nvml>,
    nvml_tried: bool,
    devinfo: HDEVINFO,
}

// SAFETY: the device-info set and D3DKMT handles are process-wide; one owner.
unsafe impl Send for GpuProvider {}

impl Default for GpuProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuProvider {
    pub fn new() -> Self {
        Self {
            gpus: Vec::new(),
            sensors: Vec::new(),
            pdh: None,
            nvml: None,
            nvml_tried: false,
            devinfo: -1,
        }
    }

    /// Present display devices with their hardware id (lowercase).
    fn display_devices(&mut self) -> Vec<(SP_DEVINFO_DATA, String)> {
        // SAFETY: enumeration; destroyed in Drop.
        self.devinfo = unsafe {
            SetupDiGetClassDevsW(
                &GUID_DEVCLASS_DISPLAY,
                std::ptr::null(),
                std::ptr::null_mut(),
                DIGCF_PRESENT,
            )
        };
        let mut out = Vec::new();
        if self.devinfo == -1 || self.devinfo == 0 {
            return out;
        }
        for i in 0..32 {
            let mut dev = SP_DEVINFO_DATA {
                cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            // SAFETY: cbSize set.
            if unsafe { SetupDiEnumDeviceInfo(self.devinfo, i, &mut dev) } == 0 {
                break;
            }
            let mut buf = [0u16; 512];
            // SAFETY: buffer size in bytes.
            let ok = unsafe {
                SetupDiGetDeviceRegistryPropertyW(
                    self.devinfo,
                    &dev,
                    SPDRP_HARDWAREID,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast(),
                    (buf.len() * 2) as u32,
                    std::ptr::null_mut(),
                )
            } != 0;
            if ok {
                out.push((dev, wstr(&buf).to_ascii_lowercase()));
            }
        }
        out
    }

    fn ensure_nvml(&mut self) {
        if !self.nvml_tried {
            self.nvml_tried = true;
            self.nvml = Nvml::load();
        }
    }

    fn nv_device(&self, name: &str) -> Option<NvDevice> {
        let nvml = self.nvml.as_ref()?;
        nvml.devices
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .or(nvml.devices.first())
            .map(|(_, d)| *d)
    }

    /// D0 = awake. Unknown states count as awake.
    fn awake(&self, dev: &SP_DEVINFO_DATA) -> bool {
        let mut data = [0u32; 16];
        // SAFETY: buffer size in bytes.
        let ok = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                self.devinfo,
                dev,
                SPDRP_DEVICE_POWER_DATA,
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                (data.len() * 4) as u32,
                std::ptr::null_mut(),
            )
        } != 0;
        // CM_POWER_DATA { PD_Size, PD_MostRecentPowerState, … }; PowerDeviceD0 = 1
        !ok || data[1] <= 1
    }
}

impl Drop for GpuProvider {
    fn drop(&mut self) {
        for g in &self.gpus {
            let c = D3DKMT_CLOSEADAPTER { hAdapter: g.handle };
            // SAFETY: opened by D3DKMTEnumAdapters2.
            unsafe { D3DKMTCloseAdapter(&c) };
        }
        if self.devinfo != -1 && self.devinfo != 0 {
            // SAFETY: created in display_devices.
            unsafe { SetupDiDestroyDeviceInfoList(self.devinfo) };
        }
    }
}

impl SensorProvider for GpuProvider {
    fn id(&self) -> &'static str {
        "gpu"
    }

    fn warmup(&self) -> std::time::Duration {
        crate::RATE_WARMUP
    }

    fn discover(&mut self) -> Discovery {
        let mut d = Discovery::default();
        let mut e = D3DKMT_ENUMADAPTERS2::default();
        // SAFETY: count query with a null array.
        if unsafe { D3DKMTEnumAdapters2(&mut e) } < 0 || e.NumAdapters == 0 {
            return d;
        }
        let mut infos = vec![
            D3DKMT_ADAPTERINFO {
                hAdapter: 0,
                AdapterLuid: Default::default(),
                NumOfSources: 0,
                bPrecisePresentRegionsPreferred: 0,
            };
            e.NumAdapters as usize
        ];
        e.pAdapters = infos.as_mut_ptr();
        // SAFETY: array of NumAdapters entries.
        if unsafe { D3DKMTEnumAdapters2(&mut e) } < 0 {
            return d;
        }
        infos.truncate(e.NumAdapters as usize);
        let devices = self.display_devices();
        // NVML is loaded only while the NVIDIA GPU is awake (initializing it would
        // otherwise power the discrete GPU up)
        let nvml_dll = std::env::var_os("SystemRoot").is_some_and(|r| {
            std::path::Path::new(&r)
                .join("System32")
                .join("nvml.dll")
                .is_file()
        });
        let mut pdh = Query::new();
        let counters = pdh.as_mut().map(|q| {
            (
                q.add(r"\GPU Engine(*)\Utilization Percentage"),
                q.add(r"\GPU Adapter Memory(*)\Dedicated Usage"),
                q.add(r"\GPU Adapter Memory(*)\Shared Usage"),
            )
        });
        if let (Some(mut q), Some((Some(eng), ded, sh))) = (pdh, counters) {
            q.collect();
            self.pdh = Some((q, eng, ded, sh));
        }
        let mut seen_names: Vec<String> = Vec::new();
        for info in &infos {
            let h = info.hAdapter;
            let mut ty = 0u32;
            let typed = query(h, KMTQAITYPE_ADAPTERTYPE, &mut ty);
            // RenderSupported (bit 0); skip software (bit 2) and indirect display (bit 6)
            if !typed || ty & 1 == 0 || ty & (1 << 2) != 0 || ty & (1 << 6) != 0 {
                let c = D3DKMT_CLOSEADAPTER { hAdapter: h };
                // SAFETY: handle from the enumeration.
                unsafe { D3DKMTCloseAdapter(&c) };
                continue;
            }
            let discrete = ty & (1 << 4) != 0;
            let mut reg = D3DKMT_ADAPTERREGISTRYINFO::default();
            let name = if query(h, KMTQAITYPE_ADAPTERREGISTRYINFO, &mut reg) {
                wstr(&reg.AdapterString)
            } else {
                String::new()
            };
            let name = if name.is_empty() {
                "GPU".to_owned()
            } else {
                name
            };
            if seen_names.contains(&name)
                && self
                    .gpus
                    .iter()
                    .any(|g| g.key == luid_key(info.AdapterLuid.HighPart, info.AdapterLuid.LowPart))
            {
                continue;
            }
            seen_names.push(name.clone());
            let gi = self.gpus.len();
            let id = format!("gpu{gi}");
            let mut seg = D3DKMT_SEGMENTSIZEINFO::default();
            let vram = if query(h, KMTQAITYPE_GETSEGMENTSIZE, &mut seg) {
                seg.DedicatedVideoMemorySize
            } else {
                0
            };
            let vendor = vendor_of(&name);
            let power_dev = if discrete {
                vendor.and_then(|v| {
                    devices
                        .iter()
                        .find(|(_, hw)| hw.contains(&format!("ven_{v}")))
                        .map(|(d, _)| *d)
                })
            } else {
                None
            };
            let asleep = power_dev.as_ref().is_some_and(|p| !self.awake(p));
            let nvidia = nvml_dll && vendor == Some("10de");
            if nvidia && !asleep {
                self.ensure_nvml();
            }
            let nv = self.nv_device(&name);
            self.gpus.push(Gpu {
                id: id.clone(),
                name: name.clone(),
                handle: h,
                key: luid_key(info.AdapterLuid.HighPart, info.AdapterLuid.LowPart),
                vram,
                power_dev,
                nvidia,
                nv,
                asleep,
            });
            d.devices.push(Device {
                id: id.clone(),
                kind: DeviceKind::Gpu,
                name: name.clone(),
            });
            let mut add = |s: SensorInfo, src: Src| {
                d.sensors.push(s);
                self.sensors.push((gi, src));
            };
            // temperature / fan / clocks: NVML when present, else D3DKMT if it reports them
            let mut perf = D3DKMT_ADAPTER_PERFDATA::default();
            let mut caps = D3DKMT_ADAPTER_PERFDATACAPS::default();
            let mut node = D3DKMT_NODE_PERFDATA::default();
            let (has_perf, has_caps, has_node) = if asleep {
                (discrete, false, false)
            } else {
                (
                    query(h, KMTQAITYPE_ADAPTERPERFDATA, &mut perf),
                    query(h, KMTQAITYPE_ADAPTERPERFDATA_CAPS, &mut caps),
                    query(h, KMTQAITYPE_NODEPERFDATA, &mut node),
                )
            };
            if nvidia {
                add(
                    SensorInfo::new(&id, Kind::Temperature, "gpu-temp", "NVML").primary(),
                    Src::NvTemp,
                );
            } else if has_perf && (perf.Temperature > 0 || discrete) {
                add(
                    SensorInfo::new(&id, Kind::Temperature, "gpu-temp", "D3DKMT").primary(),
                    Src::KmtTemp,
                );
            }
            if nvidia {
                add(
                    SensorInfo::new(&id, Kind::Clock, "gpu-clock-core", "NVML").primary(),
                    Src::NvCoreClock,
                );
                add(
                    SensorInfo::new(&id, Kind::Clock, "gpu-clock-mem", "NVML"),
                    Src::NvMemClock,
                );
                add(
                    SensorInfo::new(&id, Kind::Power, "gpu-power", "NVML").primary(),
                    Src::NvPower,
                );
                add(
                    SensorInfo::new(&id, Kind::Power, "gpu-power-limit", "NVML"),
                    Src::NvPowerLimit,
                );
                add(
                    SensorInfo::new(&id, Kind::Control, "gpu-fan-duty", "NVML"),
                    Src::NvFan,
                );
            } else {
                if has_node && node.Frequency > 0 {
                    add(
                        SensorInfo::new(&id, Kind::Clock, "gpu-clock-core", "D3DKMT").primary(),
                        Src::KmtCoreClock,
                    );
                }
                if has_perf && perf.MemoryFrequency > 0 {
                    add(
                        SensorInfo::new(&id, Kind::Clock, "gpu-clock-mem", "D3DKMT"),
                        Src::KmtMemClock,
                    );
                }
            }
            if has_caps && caps.MaxFanRPM > 0 {
                add(
                    SensorInfo::new(&id, Kind::Fan, "gpu-fan", "D3DKMT"),
                    Src::KmtFan,
                );
            }
            if self.pdh.is_some() {
                add(
                    SensorInfo::new(&id, Kind::Load, "gpu-load", "PDH GPU Engine").primary(),
                    Src::Load,
                );
                for (g, label) in GROUPS.iter().enumerate() {
                    add(
                        SensorInfo::new(&id, Kind::Load, label, "PDH GPU Engine"),
                        Src::Group(g),
                    );
                }
                add(
                    SensorInfo::new(&id, Kind::Data, "gpu-mem-used", "PDH GPU Adapter Memory")
                        .primary(),
                    Src::MemUsed,
                );
                add(
                    SensorInfo::new(&id, Kind::Data, "gpu-mem-shared", "PDH GPU Adapter Memory"),
                    Src::MemShared,
                );
                if vram > 0 {
                    add(
                        SensorInfo::new(&id, Kind::Load, "gpu-mem-load", "PDH GPU Adapter Memory"),
                        Src::MemLoad,
                    );
                }
            }
            if vram > 0 {
                add(
                    SensorInfo::new(&id, Kind::Data, "gpu-mem-total", "D3DKMT"),
                    Src::MemTotal,
                );
            }
            if !nvidia && !has_perf {
                d.notes.push(Note {
                    device: id.clone(),
                    kind: Kind::Temperature,
                    reason: "not-reported".into(),
                });
            }
        }
        d
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        let mut engines = Vec::new();
        let mut ded = Vec::new();
        let mut shared = Vec::new();
        if let Some((q, eng, dc, sc)) = &mut self.pdh {
            if q.collect() {
                engines = q.values(*eng);
                if let Some(c) = dc {
                    ded = q.values(*c);
                }
                if let Some(c) = sc {
                    shared = q.values(*c);
                }
            }
        }
        let mem = |items: &[(String, f64)], key: &str| -> Option<f64> {
            let vals: Vec<f64> = items
                .iter()
                .filter(|(n, _)| n.to_ascii_lowercase().contains(key))
                .map(|(_, v)| *v)
                .collect();
            (!vals.is_empty()).then(|| vals.iter().sum())
        };
        for gi in 0..self.gpus.len() {
            let asleep = self.gpus[gi]
                .power_dev
                .as_ref()
                .is_some_and(|p| !self.awake(p));
            self.gpus[gi].asleep = asleep;
            if !asleep && self.gpus[gi].nvidia && self.gpus[gi].nv.is_none() {
                self.ensure_nvml();
                let name = self.gpus[gi].name.clone();
                self.gpus[gi].nv = self.nv_device(&name);
            }
        }
        let mut cache: Vec<Option<(D3DKMT_ADAPTER_PERFDATA, D3DKMT_NODE_PERFDATA)>> =
            vec![None; self.gpus.len()];
        let per_gpu: Vec<_> = self
            .gpus
            .iter()
            .map(|g| {
                (
                    engine_loads(&engines, &g.key),
                    mem(&ded, &g.key),
                    mem(&shared, &g.key),
                )
            })
            .collect();
        for (local, &(gi, src)) in self.sensors.iter().enumerate() {
            let g = &self.gpus[gi];
            let ((groups, all), used, shared_used) = per_gpu[gi];
            let driver_ok = !g.asleep;
            let kmt = |cache: &mut Vec<Option<(D3DKMT_ADAPTER_PERFDATA, D3DKMT_NODE_PERFDATA)>>| {
                if cache[gi].is_none() {
                    let mut p = D3DKMT_ADAPTER_PERFDATA::default();
                    let mut n = D3DKMT_NODE_PERFDATA::default();
                    query(g.handle, KMTQAITYPE_ADAPTERPERFDATA, &mut p);
                    query(g.handle, KMTQAITYPE_NODEPERFDATA, &mut n);
                    cache[gi] = Some((p, n));
                }
                cache[gi]
            };
            let nv = |f: &dyn Fn(&Nvml, NvDevice) -> Option<f64>| -> Option<f64> {
                if !driver_ok {
                    return None;
                }
                f(self.nvml.as_ref()?, g.nv?)
            };
            let v = match src {
                Src::Load => all.or(Some(0.0)),
                Src::Group(i) => groups[i].or(Some(0.0)),
                Src::MemUsed => used,
                Src::MemShared => shared_used,
                Src::MemTotal => (g.vram > 0).then_some(g.vram as f64),
                Src::MemLoad => used
                    .filter(|_| g.vram > 0)
                    .map(|u| u * 100.0 / g.vram as f64),
                Src::KmtTemp if driver_ok => kmt(&mut cache)
                    .map(|(p, _)| f64::from(p.Temperature) / 10.0)
                    .filter(|&t| t > 0.0),
                Src::KmtFan if driver_ok => kmt(&mut cache).map(|(p, _)| f64::from(p.FanRPM)),
                Src::KmtMemClock if driver_ok => kmt(&mut cache)
                    .map(|(p, _)| p.MemoryFrequency as f64 / 1e6)
                    .filter(|&f| f > 0.0),
                Src::KmtCoreClock if driver_ok => kmt(&mut cache)
                    .map(|(_, n)| n.Frequency as f64 / 1e6)
                    .filter(|&f| f > 0.0),
                Src::NvTemp => nv(&|n, d| n.temperature(d)),
                Src::NvPower => nv(&|n, d| n.power(d)),
                Src::NvPowerLimit => nv(&|n, d| n.power_limit(d)),
                Src::NvCoreClock => nv(&|n, d| n.clock(d, 0)),
                Src::NvMemClock => nv(&|n, d| n.clock(d, 2)),
                Src::NvFan => nv(&|n, d| n.fan(d)),
                _ => None,
            };
            if let Some(v) = v {
                out.push((local, v));
            }
        }
    }

    fn live_notes(&self) -> Vec<Note> {
        self.gpus
            .iter()
            .filter(|g| g.asleep)
            .map(|g| Note {
                device: g.id.clone(),
                kind: Kind::Temperature,
                reason: "gpu-asleep".into(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_per_engine_like_task_manager() {
        let key = luid_key(0, 0xd1f2);
        let items = vec![
            (format!("pid_10_{key}_phys_0_eng_0_engtype_3D"), 30.0),
            (format!("pid_11_{key}_phys_0_eng_0_engtype_3D"), 25.0),
            (
                format!("pid_11_{key}_phys_0_eng_3_engtype_VideoDecode"),
                12.0,
            ),
            (format!("pid_12_{key}_phys_0_eng_5_engtype_Copy"), 2.0),
            (
                "pid_12_luid_0x00000000_0x0000aaaa_phys_0_eng_0_engtype_3D".to_owned(),
                99.0,
            ),
        ];
        let (groups, all) = engine_loads(&items, &key);
        assert_eq!(groups[0], Some(55.0));
        assert_eq!(groups[1], Some(2.0));
        assert_eq!(groups[2], Some(12.0));
        assert_eq!(groups[3], None);
        assert_eq!(all, Some(55.0));
    }

    #[test]
    fn luid_spelling() {
        assert_eq!(luid_key(0, 0xD1F2), "luid_0x00000000_0x0000d1f2");
        assert_eq!(engine_group("Compute_1"), Some(4));
        assert_eq!(engine_group("Security"), None);
    }
}
