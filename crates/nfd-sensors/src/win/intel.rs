//! Intel CPU sensors through PawnIO's IntelMSR module (elevated helper only):
//! core / package temperatures, RAPL power, PL1 / PL2, VID, clocks. Registers are
//! only read; see `pawnio.rs` for why nothing here can write one.

use std::time::Instant;

use windows_sys::Win32::System::SystemInformation::GROUP_AFFINITY;
use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadGroupAffinity};

use super::pawnio::PawnIo;
use crate::{Discovery, Kind, SensorInfo, SensorProvider};

const IA32_APERF: u32 = 0xE8;
const IA32_PERF_STATUS: u32 = 0x198;
const IA32_THERM_STATUS: u32 = 0x19C;
const IA32_TEMPERATURE_TARGET: u32 = 0x1A2;
const IA32_PACKAGE_THERM_STATUS: u32 = 0x1B1;
const MSR_RAPL_POWER_UNIT: u32 = 0x606;
const MSR_PKG_POWER_LIMIT: u32 = 0x610;
const MSR_PKG_ENERGY_STATUS: u32 = 0x611;
const MSR_DRAM_ENERGY_STATUS: u32 = 0x619;
const MSR_PP0_ENERGY_STATUS: u32 = 0x639;
const MSR_PP1_ENERGY_STATUS: u32 = 0x641;
const MSR_PLATFORM_ENERGY_STATUS: u32 = 0x64D;
const MSR_PLATFORM_INFO: u32 = 0xCE;

/// Bus clock (MHz) the way LibreHardwareMonitor derives it: measured TSC rate
/// divided by the non-turbo ratio (MSR_PLATFORM_INFO bits 15:8).
pub fn bus_from_tsc(tsc_delta: u64, secs: f64, nominal_ratio: f64) -> Option<f64> {
    (secs > 0.2 && nominal_ratio > 0.0).then(|| tsc_delta as f64 / secs / nominal_ratio / 1e6)
}

fn rdtsc() -> u64 {
    // SAFETY: RDTSC exists on every x86-64 CPU and has no side effects.
    unsafe { std::arch::x86_64::_rdtsc() }
}

/// TjMax (°C) from IA32_TEMPERATURE_TARGET bits 23:16.
pub fn tjmax(target: u64) -> Option<f64> {
    let t = (target >> 16) & 0xFF;
    (t > 0).then_some(t as f64)
}

/// Temperature from a (package) thermal status: TjMax − digital readout, when the
/// reading-valid bit (31) is set.
pub fn therm(status: u64, tjmax: f64) -> Option<f64> {
    (status & (1 << 31) != 0).then_some(tjmax - ((status >> 16) & 0x7F) as f64)
}

/// (watts, joules) per RAPL unit.
pub fn rapl_units(v: u64) -> (f64, f64) {
    (
        1.0 / f64::from(1u32 << (v & 0xF)),
        1.0 / f64::from(1u32 << ((v >> 8) & 0x1F)),
    )
}

/// PL1 / PL2 in watts when enabled (MSR_PKG_POWER_LIMIT).
pub fn power_limits(v: u64, watt_unit: f64) -> (Option<f64>, Option<f64>) {
    let pl1 = (v & (1 << 15) != 0).then_some((v & 0x7FFF) as f64 * watt_unit);
    let pl2 = (v & (1 << 47) != 0).then_some(((v >> 32) & 0x7FFF) as f64 * watt_unit);
    (pl1, pl2)
}

/// Core VID in volts (IA32_PERF_STATUS bits 47:32, 1/8192 V).
pub fn vid(perf_status: u64) -> Option<f64> {
    let v = (perf_status >> 32) & 0xFFFF;
    (v > 0).then(|| v as f64 / 8192.0)
}

/// Current ratio (IA32_PERF_STATUS bits 15:8).
pub fn ratio(perf_status: u64) -> u64 {
    (perf_status >> 8) & 0xFF
}

/// Watts from two 32-bit energy counter readings.
pub fn rapl_watts(prev: u32, now: u32, joule_unit: f64, secs: f64) -> Option<f64> {
    (secs > 0.01).then(|| f64::from(now.wrapping_sub(prev)) * joule_unit / secs)
}

/// Run `f` pinned to logical processor `cpu` of group 0.
fn on_cpu<T>(cpu: u32, f: impl FnOnce() -> T) -> T {
    let aff = GROUP_AFFINITY {
        Mask: 1usize << cpu,
        Group: 0,
        Reserved: [0; 3],
    };
    let mut prev = GROUP_AFFINITY::default();
    // SAFETY: current thread pseudo-handle; affinity restored below.
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
    CoreTemp(usize),
    CoreMax,
    CoreAvg,
    Package,
    Tjmax,
    Power(usize),
    Pl1,
    Pl2,
    CoreVid(usize),
    VidMax,
    CoreClock(usize),
    EffClock,
    Bus,
}

const ENERGY: [(u32, &str); 5] = [
    (MSR_PKG_ENERGY_STATUS, "cpu-power-package"),
    (MSR_PP0_ENERGY_STATUS, "cpu-power-cores"),
    (MSR_PP1_ENERGY_STATUS, "cpu-power-gt"),
    (MSR_DRAM_ENERGY_STATUS, "cpu-power-dram"),
    (MSR_PLATFORM_ENERGY_STATUS, "cpu-power-psys"),
];

#[derive(Debug)]
pub struct IntelMsrProvider {
    io: PawnIo,
    cores: Vec<(u32, u8)>,
    tjmax: f64,
    watt: f64,
    joule: f64,
    bus_mhz: f64,
    energy: Vec<Option<u32>>,
    aperf: Vec<Option<u64>>,
    last: Option<Instant>,
    /// Non-turbo ratio and the last TSC reading, for the measured bus clock.
    nominal_ratio: f64,
    tsc: u64,
    sensors: Vec<Src>,
}

impl IntelMsrProvider {
    pub fn new(io: PawnIo) -> Self {
        Self {
            io,
            cores: Vec::new(),
            tjmax: 100.0,
            watt: 0.125,
            joule: 1.0 / 16384.0,
            bus_mhz: 100.0,
            energy: vec![None; ENERGY.len()],
            aperf: Vec::new(),
            last: None,
            nominal_ratio: 0.0,
            tsc: 0,
            sensors: Vec::new(),
        }
    }
}

impl SensorProvider for IntelMsrProvider {
    fn id(&self) -> &'static str {
        "intel-msr"
    }

    fn discover(&mut self) -> Discovery {
        let topo = nfd_hwinfo::win::topology::native();
        let cpuid = nfd_hwinfo::cpu::native();
        self.cores = topo.core_threads.clone();
        self.aperf = vec![None; self.cores.len()];
        self.bus_mhz = cpuid.bus_mhz.map_or(100.0, f64::from);
        self.nominal_ratio = self
            .io
            .read_msr(MSR_PLATFORM_INFO)
            .map_or(0.0, |v| ((v >> 8) & 0xFF) as f64);
        self.tsc = rdtsc();
        let hybrid = topo.p_cores.is_some();
        let max_class = self.cores.iter().map(|c| c.1).max().unwrap_or(0);
        let src = "PawnIO IntelMSR";
        let mut d = Discovery::default();
        let mut add = |s: SensorInfo, k: Src, list: &mut Vec<Src>| {
            d.sensors.push(s);
            list.push(k);
        };
        let mut list = Vec::new();
        let tj = self.io.read_msr(IA32_TEMPERATURE_TARGET).and_then(tjmax);
        if let Some(t) = tj {
            self.tjmax = t;
            if self.io.read_msr(IA32_PACKAGE_THERM_STATUS).is_some() {
                add(
                    SensorInfo::new("cpu", Kind::Temperature, "cpu-package", src).primary(),
                    Src::Package,
                    &mut list,
                );
            }
            add(
                SensorInfo::new("cpu", Kind::Temperature, "cpu-core-max", src).primary(),
                Src::CoreMax,
                &mut list,
            );
            add(
                SensorInfo::new("cpu", Kind::Temperature, "cpu-core-avg", src),
                Src::CoreAvg,
                &mut list,
            );
            for (i, &(_, class)) in self.cores.iter().enumerate() {
                let mut s = SensorInfo::new("cpu", Kind::Temperature, "cpu-core-temp", src)
                    .index(i as u32 + 1);
                if hybrid {
                    s.detail = Some(if class == max_class { "P" } else { "E" }.into());
                }
                add(s, Src::CoreTemp(i), &mut list);
            }
            add(
                SensorInfo::new("cpu", Kind::Temperature, "cpu-tjmax", src),
                Src::Tjmax,
                &mut list,
            );
        }
        if let Some(u) = self.io.read_msr(MSR_RAPL_POWER_UNIT) {
            (self.watt, self.joule) = rapl_units(u);
            for (i, (msr, label)) in ENERGY.iter().enumerate() {
                let Some(v) = self.io.read_msr(*msr) else {
                    continue;
                };
                // a counter that never moves (no DRAM / PSys domain) reads 0
                if v as u32 == 0 {
                    continue;
                }
                self.energy[i] = Some(v as u32);
                let s = SensorInfo::new("cpu", Kind::Power, label, src);
                add(
                    if i == 0 { s.primary() } else { s },
                    Src::Power(i),
                    &mut list,
                );
            }
            if let Some(v) = self.io.read_msr(MSR_PKG_POWER_LIMIT) {
                let (pl1, pl2) = power_limits(v, self.watt);
                if pl1.is_some() {
                    add(
                        SensorInfo::new("cpu", Kind::Power, "cpu-pl1", src),
                        Src::Pl1,
                        &mut list,
                    );
                }
                if pl2.is_some() {
                    add(
                        SensorInfo::new("cpu", Kind::Power, "cpu-pl2", src),
                        Src::Pl2,
                        &mut list,
                    );
                }
            }
        }
        if self.io.read_msr(IA32_PERF_STATUS).and_then(vid).is_some() {
            add(
                SensorInfo::new("cpu", Kind::Voltage, "cpu-vid-max", src).primary(),
                Src::VidMax,
                &mut list,
            );
            for i in 0..self.cores.len() {
                add(
                    SensorInfo::new("cpu", Kind::Voltage, "cpu-vid", src).index(i as u32 + 1),
                    Src::CoreVid(i),
                    &mut list,
                );
            }
        }
        add(
            SensorInfo::new("cpu", Kind::Clock, "cpu-clock-eff", src).primary(),
            Src::EffClock,
            &mut list,
        );
        for i in 0..self.cores.len() {
            add(
                SensorInfo::new("cpu", Kind::Clock, "cpu-clock-core", src).index(i as u32 + 1),
                Src::CoreClock(i),
                &mut list,
            );
        }
        add(
            SensorInfo::new("cpu", Kind::Clock, "cpu-bus", src),
            Src::Bus,
            &mut list,
        );
        self.sensors = list;
        self.last = Some(Instant::now());
        d
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        let now = Instant::now();
        let secs = self.last.map_or(0.0, |t| (now - t).as_secs_f64());
        self.last = Some(now);
        let tsc = rdtsc();
        if let Some(bus) = bus_from_tsc(tsc.wrapping_sub(self.tsc), secs, self.nominal_ratio) {
            // a plausible bus clock only (guards against a sleep/resume jump)
            if (90.0..=110.0).contains(&bus) {
                self.bus_mhz = bus;
            }
        }
        self.tsc = tsc;
        // Two passes over the cores, like LibreHardwareMonitor: thermal status first,
        // then perf status (ratio, VID) and APERF. Reading an idle core wakes it; in
        // the second pass it reports the ratio / VID it actually runs at instead of
        // the wake-up state, which keeps the numbers comparable with other tools.
        let therms: Vec<Option<u64>> = self
            .cores
            .iter()
            .map(|&(cpu, _)| on_cpu(cpu, || self.io.read_msr(IA32_THERM_STATUS)))
            .collect();
        let per_core: Vec<(Option<u64>, Option<u64>, Option<u64>)> = self
            .cores
            .iter()
            .zip(therms)
            .map(|(&(cpu, _), t)| {
                on_cpu(cpu, || {
                    (
                        t,
                        self.io.read_msr(IA32_PERF_STATUS),
                        self.io.read_msr(IA32_APERF),
                    )
                })
            })
            .collect();
        let temps: Vec<Option<f64>> = per_core
            .iter()
            .map(|c| c.0.and_then(|s| therm(s, self.tjmax)))
            .collect();
        let valid: Vec<f64> = temps.iter().flatten().copied().collect();
        let vids: Vec<Option<f64>> = per_core.iter().map(|c| c.1.and_then(vid)).collect();
        let mut eff_sum = 0.0;
        let mut eff_n = 0;
        let mut eff: Vec<Option<f64>> = Vec::with_capacity(per_core.len());
        for (i, c) in per_core.iter().enumerate() {
            let e = match (self.aperf.get(i).copied().flatten(), c.2) {
                (Some(p), Some(n)) if secs > 0.05 && n >= p => Some((n - p) as f64 / secs / 1e6),
                _ => None,
            };
            if let Some(v) = e {
                eff_sum += v;
                eff_n += 1;
            }
            eff.push(e);
            if let Some(slot) = self.aperf.get_mut(i) {
                *slot = c.2;
            }
        }
        let mut energy_now = vec![None; ENERGY.len()];
        let pkg_limit = self
            .io
            .read_msr(MSR_PKG_POWER_LIMIT)
            .map(|v| power_limits(v, self.watt));
        let package = self
            .io
            .read_msr(IA32_PACKAGE_THERM_STATUS)
            .and_then(|s| therm(s, self.tjmax));
        for (local, &src) in self.sensors.iter().enumerate() {
            let v = match src {
                Src::CoreTemp(i) => temps.get(i).copied().flatten(),
                Src::CoreMax => valid.iter().copied().reduce(f64::max),
                Src::CoreAvg => {
                    (!valid.is_empty()).then(|| valid.iter().sum::<f64>() / valid.len() as f64)
                }
                Src::Package => package,
                Src::Tjmax => Some(self.tjmax),
                Src::Power(i) => {
                    let now = self.io.read_msr(ENERGY[i].0).map(|v| v as u32);
                    energy_now[i] = now;
                    match (self.energy[i], now) {
                        (Some(p), Some(n)) => rapl_watts(p, n, self.joule, secs),
                        _ => None,
                    }
                }
                Src::Pl1 => pkg_limit.and_then(|l| l.0),
                Src::Pl2 => pkg_limit.and_then(|l| l.1),
                Src::CoreVid(i) => vids.get(i).copied().flatten(),
                Src::VidMax => vids.iter().flatten().copied().reduce(f64::max),
                Src::CoreClock(i) => per_core
                    .get(i)
                    .and_then(|c| c.1)
                    .map(|p| ratio(p) as f64 * self.bus_mhz),
                Src::EffClock => (eff_n > 0).then(|| eff_sum / f64::from(eff_n)),
                Src::Bus => Some(self.bus_mhz),
            };
            if let Some(v) = v {
                out.push((local, v));
            }
        }
        for (i, n) in energy_now.into_iter().enumerate() {
            if n.is_some() {
                self.energy[i] = n;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_thermal_registers() {
        // TjMax 100 °C; readout 38 → 62 °C
        assert_eq!(tjmax(0x0064_0000), Some(100.0));
        assert_eq!(therm(0x8826_0000, 100.0), Some(62.0));
        assert_eq!(therm(0x0826_0000, 100.0), None, "reading not valid");
    }

    #[test]
    fn decodes_rapl() {
        // typical 0x000A0E03: 1/8 W, 1/16384 J
        let (w, j) = rapl_units(0x000A_0E03);
        assert_eq!(w, 0.125);
        assert_eq!(j, 1.0 / 16384.0);
        // 55 W PL1 enabled, 157 W PL2 enabled
        let v = (1u64 << 47) | ((157 * 8) << 32) | (1 << 15) | (55 * 8);
        assert_eq!(power_limits(v, w), (Some(55.0), Some(157.0)));
        assert_eq!(
            power_limits(55 * 8, w),
            (None, None),
            "disabled limits are hidden"
        );
        // counter wrap-around
        assert_eq!(rapl_watts(u32::MAX - 16383, 16384, j, 1.0), Some(2.0));
        assert_eq!(rapl_watts(0, 1, j, 0.0), None);
    }

    #[test]
    fn bus_clock_from_tsc() {
        // 2.4 GHz TSC, ratio 24 → 100 MHz
        let b = bus_from_tsc(2_419_200_000, 1.0, 24.0).expect("bus");
        assert!((b - 100.8).abs() < 1e-9);
        assert_eq!(bus_from_tsc(1, 0.1, 24.0), None);
        assert_eq!(bus_from_tsc(1, 1.0, 0.0), None);
    }

    #[test]
    fn decodes_perf_status() {
        let v = (9830u64 << 32) | (52 << 8);
        assert!((vid(v).expect("vid") - 1.19995).abs() < 1e-4);
        assert_eq!(ratio(v), 52);
        assert_eq!(vid(52 << 8), None);
    }
}
