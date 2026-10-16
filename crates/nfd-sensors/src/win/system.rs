//! CPU load and estimated clock, memory, ACPI thermal zones — all user-mode.

use windows_sys::Wdk::System::SystemInformation::{
    NtQuerySystemInformation, SystemProcessorPerformanceInformation,
};
use windows_sys::Win32::System::Power::{
    CallNtPowerInformation, PROCESSOR_POWER_INFORMATION, ProcessorInformation,
};
use windows_sys::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::WindowsProgramming::SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION;

use super::pdh::{Counter, Query};
use crate::{Device, DeviceKind, Discovery, Kind, Note, SensorInfo, SensorProvider};

const SPPI: usize = size_of::<SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION>();

/// (idle, total) 100-ns ticks per logical processor.
fn cpu_times(n: usize) -> Vec<(i64, i64)> {
    let mut buf = vec![SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION::default(); n.max(1)];
    let mut got = 0u32;
    // SAFETY: buffer of n entries.
    let st = unsafe {
        NtQuerySystemInformation(
            SystemProcessorPerformanceInformation,
            buf.as_mut_ptr().cast(),
            (buf.len() * SPPI) as u32,
            &mut got,
        )
    };
    if st < 0 {
        return Vec::new();
    }
    buf.truncate(got as usize / SPPI);
    // KernelTime includes IdleTime
    buf.iter()
        .map(|p| (p.IdleTime, p.KernelTime + p.UserTime))
        .collect()
}

/// Base clock in MHz of logical processor 0 (what `% Processor Performance` scales).
fn base_mhz(threads: usize) -> Option<f64> {
    let mut info = vec![PROCESSOR_POWER_INFORMATION::default(); threads.max(1)];
    let size = (info.len() * size_of::<PROCESSOR_POWER_INFORMATION>()) as u32;
    // SAFETY: output buffer of `size` bytes.
    let st = unsafe {
        CallNtPowerInformation(
            ProcessorInformation,
            std::ptr::null(),
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    (st == 0 && info[0].MaxMhz > 0).then(|| f64::from(info[0].MaxMhz))
}

/// Load from two tick samples (0–100).
pub fn load(prev: (i64, i64), now: (i64, i64)) -> Option<f64> {
    let idle = now.0 - prev.0;
    let total = now.1 - prev.1;
    (total > 0).then(|| (100.0 * (1.0 - idle as f64 / total as f64)).clamp(0.0, 100.0))
}

#[derive(Debug)]
pub struct CpuProvider {
    threads: usize,
    prev: Vec<(i64, i64)>,
    pdh: Option<(Query, Counter)>,
    base: Option<f64>,
}

impl Default for CpuProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuProvider {
    pub fn new() -> Self {
        Self {
            threads: 0,
            prev: Vec::new(),
            pdh: None,
            base: None,
        }
    }
}

impl SensorProvider for CpuProvider {
    fn id(&self) -> &'static str {
        "cpu"
    }

    fn discover(&mut self) -> Discovery {
        let topo = nfd_hwinfo::win::topology::native();
        let id = nfd_hwinfo::cpu::native();
        self.threads = (topo.threads as usize).clamp(1, 64);
        self.prev = cpu_times(self.threads);
        self.base = base_mhz(self.threads);
        let mut pdh = Query::new();
        let perf = pdh
            .as_mut()
            .and_then(|q| q.add(r"\Processor Information(_Total)\% Processor Performance"));
        if let (Some(mut q), Some(c)) = (pdh, perf) {
            q.collect();
            self.pdh = Some((q, c));
        }
        let hybrid = topo.p_cores.is_some();
        let mut sensors = vec![
            SensorInfo::new("cpu", Kind::Load, "cpu-total", "NtQuerySystemInformation").primary(),
            SensorInfo::new(
                "cpu",
                Kind::Load,
                "cpu-thread-max",
                "NtQuerySystemInformation",
            ),
        ];
        for t in 0..self.threads {
            let mut s =
                SensorInfo::new("cpu", Kind::Load, "cpu-thread", "NtQuerySystemInformation")
                    .index(t as u32 + 1);
            if hybrid {
                let max_class = topo.thread_class.iter().copied().max().unwrap_or(0);
                let class = topo.thread_class.get(t).copied().unwrap_or(0);
                s.detail = Some(if class == max_class { "P" } else { "E" }.into());
            }
            sensors.push(s);
        }
        if self.pdh.is_some() && self.base.is_some() {
            sensors.push(
                SensorInfo::new(
                    "cpu",
                    Kind::Clock,
                    "cpu-clock-avg",
                    "PDH % Processor Performance",
                )
                .primary()
                .estimated(),
            );
        }
        let pawn = |kind| Note {
            device: "cpu".into(),
            kind,
            reason: "needs-pawnio".into(),
        };
        Discovery {
            devices: vec![Device {
                id: "cpu".into(),
                kind: DeviceKind::Cpu,
                name: if id.brand.is_empty() {
                    "CPU".into()
                } else {
                    id.brand
                },
            }],
            sensors,
            notes: vec![
                pawn(Kind::Temperature),
                pawn(Kind::Power),
                pawn(Kind::Voltage),
            ],
        }
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        let now = cpu_times(self.threads);
        if now.len() == self.prev.len() && !now.is_empty() {
            let mut idle = 0;
            let mut total = 0;
            let mut max: f64 = 0.0;
            for (t, (p, n)) in self.prev.iter().zip(&now).enumerate() {
                idle += n.0 - p.0;
                total += n.1 - p.1;
                if let Some(l) = load(*p, *n) {
                    max = max.max(l);
                    out.push((2 + t, l));
                }
            }
            if let Some(l) = load((0, 0), (idle, total)) {
                out.push((0, l));
                out.push((1, max));
            }
        }
        self.prev = now;
        if let (Some((q, c)), Some(base)) = (&mut self.pdh, self.base) {
            if q.collect() {
                if let Some(pct) = q.value(*c) {
                    out.push((2 + self.threads, base * pct / 100.0));
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct MemoryProvider;

impl SensorProvider for MemoryProvider {
    fn id(&self) -> &'static str {
        "memory"
    }

    fn discover(&mut self) -> Discovery {
        let src = "GlobalMemoryStatusEx";
        Discovery {
            devices: vec![Device {
                id: "memory".into(),
                kind: DeviceKind::Memory,
                name: "Memory".into(),
            }],
            sensors: vec![
                SensorInfo::new("memory", Kind::Load, "mem-load", src).primary(),
                SensorInfo::new("memory", Kind::Data, "mem-used", src).primary(),
                SensorInfo::new("memory", Kind::Data, "mem-available", src),
                SensorInfo::new(
                    "memory",
                    Kind::Load,
                    "mem-commit-load",
                    "GetPerformanceInfo",
                ),
                SensorInfo::new(
                    "memory",
                    Kind::Data,
                    "mem-commit-used",
                    "GetPerformanceInfo",
                ),
                SensorInfo::new(
                    "memory",
                    Kind::Data,
                    "mem-commit-limit",
                    "GetPerformanceInfo",
                ),
            ],
            notes: Vec::new(),
        }
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        let mut m = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: dwLength set.
        if unsafe { GlobalMemoryStatusEx(&mut m) } != 0 && m.ullTotalPhys > 0 {
            let used = m.ullTotalPhys - m.ullAvailPhys;
            out.push((0, used as f64 * 100.0 / m.ullTotalPhys as f64));
            out.push((1, used as f64));
            out.push((2, m.ullAvailPhys as f64));
        }
        let mut p = PERFORMANCE_INFORMATION::default();
        let cb = size_of::<PERFORMANCE_INFORMATION>() as u32;
        // SAFETY: cb is the struct size.
        if unsafe { GetPerformanceInfo(&mut p, cb) } != 0 && p.CommitLimit > 0 {
            let page = p.PageSize as f64;
            out.push((3, p.CommitTotal as f64 * 100.0 / p.CommitLimit as f64));
            out.push((4, p.CommitTotal as f64 * page));
            out.push((5, p.CommitLimit as f64 * page));
        }
    }
}

/// ACPI thermal zones (`\Thermal Zone Information(*)`): coarse, firmware-defined,
/// often a fixed value — shown under "Mainboard" with its zone name, never as the
/// CPU temperature.
#[derive(Debug)]
pub struct AcpiProvider {
    pdh: Option<(Query, Counter)>,
    zones: Vec<String>,
    high_precision: bool,
}

impl Default for AcpiProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AcpiProvider {
    pub fn new() -> Self {
        Self {
            pdh: None,
            zones: Vec::new(),
            high_precision: true,
        }
    }

    fn kelvin(&self, raw: f64) -> f64 {
        let k = if self.high_precision { raw / 10.0 } else { raw };
        ((k - 273.15) * 10.0).round() / 10.0
    }
}

/// "\_TZ.TZ00" → "TZ00".
pub fn zone_name(instance: &str) -> String {
    instance
        .rsplit(['.', '\\'])
        .next()
        .unwrap_or(instance)
        .to_ascii_uppercase()
}

impl SensorProvider for AcpiProvider {
    fn id(&self) -> &'static str {
        "acpi"
    }

    fn interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(2)
    }

    fn discover(&mut self) -> Discovery {
        let mut d = Discovery::default();
        for (path, hp) in [
            (
                r"\Thermal Zone Information(*)\High Precision Temperature",
                true,
            ),
            (r"\Thermal Zone Information(*)\Temperature", false),
        ] {
            let Some(mut q) = Query::new() else { return d };
            let Some(c) = q.add(path) else { continue };
            q.collect();
            let vals = q.values(c);
            if vals.iter().any(|(_, v)| *v > 0.0) {
                self.high_precision = hp;
                self.zones = vals.iter().map(|(n, _)| n.clone()).collect();
                self.pdh = Some((q, c));
                break;
            }
        }
        if self.zones.is_empty() {
            return d;
        }
        d.devices.push(Device {
            id: "acpi".into(),
            kind: DeviceKind::Board,
            name: "ACPI".into(),
        });
        for z in &self.zones {
            d.sensors.push(
                SensorInfo::new("acpi", Kind::Temperature, "acpi-zone", "ACPI thermal zone")
                    .detail(zone_name(z)),
            );
        }
        d
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        let Some((q, c)) = &mut self.pdh else { return };
        if !q.collect() {
            return;
        }
        let vals = q.values(*c);
        for (name, raw) in vals {
            if let Some(i) = self.zones.iter().position(|z| *z == name) {
                if raw > 0.0 {
                    out.push((i, self.kelvin(raw)));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_from_ticks() {
        assert_eq!(load((0, 0), (25, 100)), Some(75.0));
        assert_eq!(load((10, 10), (10, 10)), None);
        assert_eq!(load((0, 0), (200, 100)), Some(0.0), "clamped");
    }

    #[test]
    fn zone_names() {
        assert_eq!(zone_name(r"\_TZ.TZ00"), "TZ00");
        assert_eq!(zone_name("tz01"), "TZ01");
    }

    #[test]
    fn kelvin_conversion() {
        let mut p = AcpiProvider::new();
        assert_eq!(p.kelvin(3231.0), 50.0);
        p.high_precision = false;
        assert_eq!(p.kelvin(323.15), 50.0);
    }
}
