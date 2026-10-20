//! AMD Zen (family 17h–1Ah) sensors through PawnIO's AMDFamily17 module, elevated
//! helper only: Tctl / Tdie, per-CCD temperatures, package and per-core power
//! (RAPL), per-core VID and effective clocks. Registers and SMN addresses follow
//! Linux k10temp and LibreHardwareMonitor.
//!
//! Not yet verified on AMD hardware (docs/hwmon-validation.md): every sensor is
//! marked as coming from this provider so a wrong value can be traced.

use std::time::Instant;

use windows_sys::Win32::System::SystemInformation::GROUP_AFFINITY;
use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadGroupAffinity};

use super::pawnio::PawnIo;
use crate::{Discovery, Kind, SensorInfo, SensorProvider};

const MSR_APERF_RO: u32 = 0xC000_00E8;
const MSR_HARDWARE_PSTATE_STATUS: u32 = 0xC001_0293;
const MSR_PWR_UNIT: u32 = 0xC001_0299;
const MSR_CORE_ENERGY_STAT: u32 = 0xC001_029A;
const MSR_PKG_ENERGY_STAT: u32 = 0xC001_029B;

const THM_TCON_CUR_TMP: u32 = 0x0005_9800;
const CCD_TEMP_M70H: u32 = 0x0005_9954; // Zen 2 / Zen 3
const CCD_TEMP_M61H: u32 = 0x0005_9B08; // Zen 4 (Raphael), Zen 5 (Granite Ridge)
const CCD_TEMP_F1AH_M00H: u32 = 0x0005_99F0; // Zen 5 Turin / Shimada Peak
const RANGE_SEL: u32 = 0x8_0000;
const TJ_SEL: u32 = 0x3_0000;
const CCD_VALID: u32 = 0x800;

/// Tctl in °C from THM_TCON_CUR_TMP (bits 31:21, 1/8 °C; −49 °C when the range
/// select / TJ select flags say so).
pub fn tctl(raw: u32) -> f64 {
    let t = f64::from(raw >> 21) * 0.125;
    if raw & RANGE_SEL != 0 || raw & TJ_SEL == TJ_SEL {
        t - 49.0
    } else {
        t
    }
}

/// Tdie = Tctl + offset for the parts that report an offset Tctl (k10temp table).
pub fn tdie_offset(brand: &str) -> f64 {
    if ["1600X", "1700X", "1800X"]
        .iter()
        .any(|m| brand.contains(m))
    {
        -20.0
    } else if brand.contains("Threadripper 19") || brand.contains("Threadripper 29") {
        -27.0
    } else if brand.contains("2700X") {
        -10.0
    } else {
        0.0
    }
}

/// CCD temperature (bits 10:0, 1/8 °C − 49 °C) when present.
pub fn ccd_temp(raw: u32, needs_valid_bit: bool) -> Option<f64> {
    let present = if needs_valid_bit {
        raw & CCD_VALID != 0
    } else {
        raw & 0xFFF > 0
    };
    let t = f64::from(raw & 0x7FF) * 0.125 - 49.0;
    (present && t < 125.0).then_some(t)
}

/// Where the CCD sensors live and how many there can be, by family / model.
pub fn ccd_layout(family: u32, model: u32) -> Option<(u32, u32, bool)> {
    if family == 0x1A && model <= 0x2F {
        return Some((CCD_TEMP_F1AH_M00H, 16, true));
    }
    match model {
        0x61 | 0x44 => Some((CCD_TEMP_M61H, 8, false)),
        0x31 | 0x71 | 0x21 => Some((CCD_TEMP_M70H, 8, false)),
        _ => None,
    }
}

/// Core VID in volts from MSR_HARDWARE_PSTATE_STATUS (CurCpuVid bits 21:14, SVI2).
pub fn vid(pstate: u64) -> Option<f64> {
    let v = ((pstate >> 14) & 0xFF) as f64;
    let volts = 1.55 - 0.00625 * v;
    (v > 0.0 && volts > 0.0).then_some(volts)
}

fn on_cpu<T>(cpu: u32, f: impl FnOnce() -> T) -> T {
    let aff = GROUP_AFFINITY {
        Mask: 1usize << cpu,
        Group: 0,
        Reserved: [0; 3],
    };
    let mut prev = GROUP_AFFINITY::default();
    // SAFETY: current-thread pseudo-handle; restored below.
    let pinned = unsafe { SetThreadGroupAffinity(GetCurrentThread(), &aff, &mut prev) } != 0;
    let out = f();
    if pinned {
        // SAFETY: restore the previous affinity.
        unsafe { SetThreadGroupAffinity(GetCurrentThread(), &prev, std::ptr::null_mut()) };
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Src {
    Tctl,
    Tdie,
    Ccd(u32),
    CcdMax,
    PackagePower,
    CorePower(usize),
    CoreVid(usize),
    CoreClock(usize),
    EffClock,
}

#[derive(Debug)]
pub struct AmdZenProvider {
    io: PawnIo,
    cores: Vec<u32>,
    offset: f64,
    ccd: Option<(u32, u32, bool)>,
    ccds: Vec<u32>,
    joule: f64,
    pkg: Option<u32>,
    core_energy: Vec<Option<u32>>,
    aperf: Vec<Option<u64>>,
    last: Option<Instant>,
    sensors: Vec<Src>,
}

impl AmdZenProvider {
    pub fn new(io: PawnIo) -> Self {
        Self {
            io,
            cores: Vec::new(),
            offset: 0.0,
            ccd: None,
            ccds: Vec::new(),
            joule: 1.0 / 65536.0,
            pkg: None,
            core_energy: Vec::new(),
            aperf: Vec::new(),
            last: None,
            sensors: Vec::new(),
        }
    }
}

impl SensorProvider for AmdZenProvider {
    fn id(&self) -> &'static str {
        "amd-zen"
    }

    fn warmup(&self) -> std::time::Duration {
        crate::RATE_WARMUP
    }

    fn discover(&mut self) -> Discovery {
        let topo = nfd_hwinfo::win::topology::native();
        let cpuid = nfd_hwinfo::cpu::native();
        self.cores = topo.core_threads.iter().map(|c| c.0).collect();
        self.core_energy = vec![None; self.cores.len()];
        self.aperf = vec![None; self.cores.len()];
        self.offset = tdie_offset(&cpuid.brand);
        self.ccd = ccd_layout(cpuid.family, cpuid.model);
        let src = "PawnIO AMDFamily17";
        let mut d = Discovery::default();
        let mut list = Vec::new();
        let mut add = |s: SensorInfo, k: Src| {
            d.sensors.push(s);
            list.push(k);
        };
        if self.io.read_smn(THM_TCON_CUR_TMP).is_some() {
            add(
                SensorInfo::new("cpu", Kind::Temperature, "cpu-package", src).primary(),
                Src::Tctl,
            );
            if self.offset < 0.0 {
                add(
                    SensorInfo::new("cpu", Kind::Temperature, "cpu-tdie", src).primary(),
                    Src::Tdie,
                );
            }
        }
        if let Some((base, count, valid)) = self.ccd {
            for i in 0..count {
                if self
                    .io
                    .read_smn(base + i * 4)
                    .and_then(|r| ccd_temp(r, valid))
                    .is_some()
                {
                    self.ccds.push(i);
                    add(
                        SensorInfo::new("cpu", Kind::Temperature, "cpu-ccd", src).index(i + 1),
                        Src::Ccd(i),
                    );
                }
            }
            if self.ccds.len() > 1 {
                add(
                    SensorInfo::new("cpu", Kind::Temperature, "cpu-ccd-max", src),
                    Src::CcdMax,
                );
            }
        }
        if let Some(u) = self.io.read_msr(MSR_PWR_UNIT) {
            self.joule = 1.0 / f64::from(1u32 << ((u >> 8) & 0x1F));
            if let Some(v) = self.io.read_msr(MSR_PKG_ENERGY_STAT) {
                self.pkg = Some(v as u32);
                add(
                    SensorInfo::new("cpu", Kind::Power, "cpu-power-package", src).primary(),
                    Src::PackagePower,
                );
            }
            for i in 0..self.cores.len() {
                add(
                    SensorInfo::new("cpu", Kind::Power, "cpu-power-core", src).index(i as u32 + 1),
                    Src::CorePower(i),
                );
            }
        }
        if self
            .io
            .read_msr(MSR_HARDWARE_PSTATE_STATUS)
            .and_then(vid)
            .is_some()
        {
            for i in 0..self.cores.len() {
                add(
                    SensorInfo::new("cpu", Kind::Voltage, "cpu-vid", src).index(i as u32 + 1),
                    Src::CoreVid(i),
                );
            }
        }
        add(
            SensorInfo::new("cpu", Kind::Clock, "cpu-clock-eff", src).primary(),
            Src::EffClock,
        );
        for i in 0..self.cores.len() {
            add(
                SensorInfo::new("cpu", Kind::Clock, "cpu-clock-core", src).index(i as u32 + 1),
                Src::CoreClock(i),
            );
        }
        self.sensors = list;
        self.last = Some(Instant::now());
        d
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        let now = Instant::now();
        let secs = self.last.map_or(0.0, |t| (now - t).as_secs_f64());
        self.last = Some(now);
        let per_core: Vec<(Option<u64>, Option<u64>, Option<u64>)> = self
            .cores
            .iter()
            .map(|&cpu| {
                on_cpu(cpu, || {
                    (
                        self.io.read_msr(MSR_CORE_ENERGY_STAT),
                        self.io.read_msr(MSR_HARDWARE_PSTATE_STATUS),
                        self.io.read_msr(MSR_APERF_RO),
                    )
                })
            })
            .collect();
        let tctl_now = self.io.read_smn(THM_TCON_CUR_TMP).map(tctl);
        let ccd_now: Vec<(u32, Option<f64>)> = match self.ccd {
            Some((base, _, valid)) => self
                .ccds
                .iter()
                .map(|&i| {
                    (
                        i,
                        self.io
                            .read_smn(base + i * 4)
                            .and_then(|r| ccd_temp(r, valid)),
                    )
                })
                .collect(),
            None => Vec::new(),
        };
        let pkg_now = self.io.read_msr(MSR_PKG_ENERGY_STAT).map(|v| v as u32);
        let watts = |prev: Option<u32>, now: Option<u32>| match (prev, now) {
            (Some(p), Some(n)) if secs > 0.01 => {
                Some(f64::from(n.wrapping_sub(p)) * self.joule / secs)
            }
            _ => None,
        };
        let mut eff_sum = 0.0;
        let mut eff_n = 0u32;
        let clocks: Vec<Option<f64>> = per_core
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let e = match (self.aperf[i], c.2) {
                    (Some(p), Some(n)) if secs > 0.05 && n >= p => {
                        Some((n - p) as f64 / secs / 1e6)
                    }
                    _ => None,
                };
                if let Some(v) = e {
                    eff_sum += v;
                    eff_n += 1;
                }
                e
            })
            .collect();
        for (local, &src) in self.sensors.iter().enumerate() {
            let v = match src {
                Src::Tctl => tctl_now,
                Src::Tdie => tctl_now.map(|t| t + self.offset),
                Src::Ccd(i) => ccd_now.iter().find(|c| c.0 == i).and_then(|c| c.1),
                Src::CcdMax => ccd_now.iter().filter_map(|c| c.1).reduce(f64::max),
                Src::PackagePower => watts(self.pkg, pkg_now),
                Src::CorePower(i) => watts(self.core_energy[i], per_core[i].0.map(|v| v as u32)),
                Src::CoreVid(i) => per_core[i].1.and_then(vid),
                Src::CoreClock(i) => clocks[i],
                Src::EffClock => (eff_n > 0).then(|| eff_sum / f64::from(eff_n)),
            };
            if let Some(v) = v {
                out.push((local, v));
            }
        }
        self.pkg = pkg_now.or(self.pkg);
        for (i, c) in per_core.iter().enumerate() {
            self.core_energy[i] = c.0.map(|v| v as u32).or(self.core_energy[i]);
            self.aperf[i] = c.2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tctl_and_offsets() {
        // 62.5 °C, no range select
        assert_eq!(tctl(500 << 21), 62.5);
        // range select: −49 °C
        assert_eq!(tctl((800 << 21) | RANGE_SEL), 51.0);
        assert_eq!(tdie_offset("AMD Ryzen 7 1700X Eight-Core Processor"), -20.0);
        assert_eq!(tdie_offset("AMD Ryzen 9 7950X 16-Core Processor"), 0.0);
    }

    #[test]
    fn ccd_decoding_and_layout() {
        // 0x3A0 * 0.125 − 49 = 67 °C
        assert_eq!(ccd_temp(0x3A0, false), Some(67.0));
        assert_eq!(ccd_temp(0, false), None);
        assert_eq!(ccd_temp(0x3A0, true), None, "Turin needs the valid bit");
        assert_eq!(ccd_temp(0x3A0 | CCD_VALID, true), Some(67.0));
        assert_eq!(ccd_layout(0x19, 0x61), Some((CCD_TEMP_M61H, 8, false)));
        assert_eq!(ccd_layout(0x19, 0x21), Some((CCD_TEMP_M70H, 8, false)));
        assert_eq!(ccd_layout(0x1A, 0x08), Some((CCD_TEMP_F1AH_M00H, 16, true)));
        assert_eq!(ccd_layout(0x17, 0x08), None, "Zen 1 has no CCD sensors");
    }

    #[test]
    fn vid_from_pstate() {
        // CurCpuVid 0x38 → 1.55 − 0.35 = 1.20 V
        assert!((vid(0x38 << 14).expect("vid") - 1.2).abs() < 1e-9);
        assert_eq!(vid(0), None);
    }
}
