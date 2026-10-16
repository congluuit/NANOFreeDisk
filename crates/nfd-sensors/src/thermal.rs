//! Temperature alerts, the 24-hour log and the Health Score contribution
//! (docs/HARDWARE_MONITOR.md §7 slice 6). Pure: fed with [`Snapshot`]s.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{DeviceKind, Kind, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Cpu,
    Gpu,
    Disk,
}

/// (warm, hot) °C — the same limits the UI colors with (`hw/overview.ts`).
pub fn limits(part: Part) -> (f64, f64) {
    match part {
        Part::Cpu => (80.0, 95.0),
        Part::Gpu => (80.0, 90.0),
        Part::Disk => (60.0, 70.0),
    }
}

/// A watched temperature right now.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub part: Part,
    pub sensor: String,
    pub device: String,
    pub value: f64,
    /// Hot limit of this device when it reports one (a drive's warning temperature).
    pub hot: Option<f64>,
}

impl Reading {
    /// (warm, hot) for this reading: the device's own warning temperature (warm
    /// 10 degrees below it) or the part's defaults.
    pub fn limits(&self) -> (f64, f64) {
        match self.hot {
            Some(h) => (h - 10.0, h),
            None => limits(self.part),
        }
    }
}

/// The temperatures that matter for alerts: CPU package / hottest core (Tdie on
/// AMD), each GPU core, each disk. ACPI zones are too coarse to alert on.
pub fn readings(snap: &Snapshot) -> Vec<Reading> {
    let name = |id: &str| {
        snap.devices
            .iter()
            .find(|d| d.id == id)
            .map(|d| (d.kind, d.name.clone()))
    };
    let warning = |device: &str| {
        snap.sensors
            .iter()
            .find(|s| s.info.device == device && s.info.label == "disk-temp-warning")
            .and_then(|s| s.value)
    };
    let mut out = Vec::new();
    for s in &snap.sensors {
        let (Some(v), Kind::Temperature) = (s.value, s.info.kind) else {
            continue;
        };
        let Some((kind, device)) = name(&s.info.device) else {
            continue;
        };
        let part = match (kind, s.info.label.as_str()) {
            (DeviceKind::Cpu, "cpu-package" | "cpu-core-max" | "cpu-tdie") => Part::Cpu,
            (DeviceKind::Gpu, "gpu-temp") => Part::Gpu,
            (DeviceKind::Storage, "disk-temp") => Part::Disk,
            _ => continue,
        };
        out.push(Reading {
            part,
            sensor: s.info.id.clone(),
            device,
            value: v,
            hot: if part == Part::Disk {
                warning(&s.info.device)
            } else {
                None
            },
        });
    }
    out
}

/// One minute of the log: the hottest value per part and which device it was.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Minute {
    /// Unix time / 60.
    pub t: u64,
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub disk: Option<f32>,
    #[serde(default)]
    pub hottest: HashMap<Part, String>,
    /// Hot limit of the hottest device, when it reports its own.
    #[serde(default)]
    pub limit: HashMap<Part, f32>,
}

impl Minute {
    fn slot(&mut self, p: Part) -> &mut Option<f32> {
        match p {
            Part::Cpu => &mut self.cpu,
            Part::Gpu => &mut self.gpu,
            Part::Disk => &mut self.disk,
        }
    }

    pub fn get(&self, p: Part) -> Option<f32> {
        match p {
            Part::Cpu => self.cpu,
            Part::Gpu => self.gpu,
            Part::Disk => self.disk,
        }
    }
}

pub const LOG_MINUTES: usize = 24 * 60;

/// Per-minute maxima of the last 24 hours (only minutes in which something was
/// sampled — the hub samples only while asked).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThermalLog {
    pub minutes: VecDeque<Minute>,
}

impl ThermalLog {
    pub fn observe(&mut self, unix: u64, readings: &[Reading]) {
        if readings.is_empty() {
            return;
        }
        let t = unix / 60;
        if self.minutes.back().is_none_or(|m| m.t != t) {
            self.minutes.push_back(Minute {
                t,
                ..Minute::default()
            });
        }
        while self
            .minutes
            .front()
            .is_some_and(|m| m.t + LOG_MINUTES as u64 <= t)
        {
            self.minutes.pop_front();
        }
        while self.minutes.len() > LOG_MINUTES {
            self.minutes.pop_front();
        }
        let Some(m) = self.minutes.back_mut() else {
            return;
        };
        for r in readings {
            let v = r.value as f32;
            // rank by how close each device is to its own limit, so a drive rated
            // for 80 degrees at 69 does not outrank one rated for 70 at 65
            let default = limits(r.part).1 as f32;
            let mine = r.hot.map_or(default, |h| h as f32) - v;
            let their_limit = m.limit.get(&r.part).copied().unwrap_or(default);
            let theirs = m.get(r.part).map(|old| their_limit - old);
            if theirs.is_none_or(|t| mine < t) {
                *m.slot(r.part) = Some(v);
                m.hottest.insert(r.part, r.device.clone());
                match r.hot {
                    Some(h) => m.limit.insert(r.part, h as f32),
                    None => m.limit.remove(&r.part),
                };
            }
        }
    }

    /// Drop minutes older than 24 h relative to `unix` (after loading from disk).
    pub fn prune(&mut self, unix: u64) {
        let t = unix / 60;
        self.minutes
            .retain(|m| m.t + LOG_MINUTES as u64 > t && m.t <= t);
    }

    /// Hottest value of `part` in the log and the device it came from.
    pub fn max(&self, part: Part) -> Option<(f32, String)> {
        self.worst(part).map(|(v, d, _)| (v, d))
    }

    /// The minute closest to its limit: (value, device, own hot limit).
    pub fn worst(&self, part: Part) -> Option<(f32, String, Option<f32>)> {
        let default = limits(part).1 as f32;
        self.minutes
            .iter()
            .filter_map(|m| {
                let v = m.get(part)?;
                Some((
                    v,
                    m.hottest.get(&part).cloned().unwrap_or_default(),
                    m.limit.get(&part).copied(),
                ))
            })
            .reduce(|a, b| {
                if b.2.unwrap_or(default) - b.0 < a.2.unwrap_or(default) - a.0 {
                    b
                } else {
                    a
                }
            })
    }
}

/// An alert to show.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alert {
    pub part: Part,
    pub sensor: String,
    pub device: String,
    pub value: f64,
    pub limit: f64,
}

/// Fires once when a temperature reaches its hot limit, then stays quiet until it
/// has dropped 5 °C below the limit and at least `cooldown_secs` have passed.
#[derive(Debug, Clone)]
pub struct Watch {
    pub cooldown_secs: u64,
    armed: HashMap<String, bool>,
    last: HashMap<String, u64>,
}

impl Default for Watch {
    fn default() -> Self {
        Self {
            cooldown_secs: 15 * 60,
            armed: HashMap::new(),
            last: HashMap::new(),
        }
    }
}

impl Watch {
    pub fn check(&mut self, unix: u64, readings: &[Reading]) -> Vec<Alert> {
        let mut out = Vec::new();
        for r in readings {
            let (_, hot) = r.limits();
            let armed = self.armed.entry(r.sensor.clone()).or_insert(true);
            if r.value < hot - 5.0 {
                *armed = true;
                continue;
            }
            let cooled = self
                .last
                .get(&r.sensor)
                .is_none_or(|&t| unix >= t + self.cooldown_secs);
            if r.value >= hot && *armed && cooled {
                *armed = false;
                self.last.insert(r.sensor.clone(), unix);
                out.push(Alert {
                    part: r.part,
                    sensor: r.sensor.clone(),
                    device: r.device.clone(),
                    value: r.value,
                    limit: hot,
                });
            }
        }
        out
    }
}

/// A Health Score finding from the last 24 hours.
#[derive(Debug, Clone, PartialEq)]
pub struct HeatFinding {
    pub part: Part,
    pub device: String,
    pub max: f64,
    pub hot: bool,
    /// Points off the Health Score.
    pub penalty: u32,
}

pub fn findings(log: &ThermalLog) -> Vec<HeatFinding> {
    [Part::Cpu, Part::Gpu, Part::Disk]
        .into_iter()
        .filter_map(|p| {
            let (max, device, own) = log.worst(p)?;
            let max = f64::from(max);
            let (warm, hot) = match own {
                Some(h) => (f64::from(h) - 10.0, f64::from(h)),
                None => limits(p),
            };
            if max >= hot {
                Some(HeatFinding {
                    part: p,
                    device,
                    max,
                    hot: true,
                    penalty: 8,
                })
            } else if max >= warm {
                Some(HeatFinding {
                    part: p,
                    device,
                    max,
                    hot: false,
                    penalty: 2,
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(part: Part, sensor: &str, v: f64) -> Reading {
        Reading {
            part,
            sensor: sensor.into(),
            device: format!("dev-{sensor}"),
            value: v,
            hot: None,
        }
    }

    #[test]
    fn alert_fires_once_and_rearms_after_cooling_and_cooldown() {
        let mut w = Watch::default();
        let cpu = |v| vec![r(Part::Cpu, "cpu/cpu-package", v)];
        assert!(w.check(0, &cpu(94.0)).is_empty());
        assert_eq!(w.check(5, &cpu(96.0)).len(), 1);
        assert!(w.check(10, &cpu(97.0)).is_empty(), "still hot: no repeat");
        assert!(w.check(20, &cpu(85.0)).is_empty(), "cooled: re-armed");
        assert!(w.check(30, &cpu(96.0)).is_empty(), "cooldown not over");
        assert!(w.check(200, &cpu(80.0)).is_empty());
        assert_eq!(w.check(1000, &cpu(95.0)).len(), 1, "after cooldown");
        // disks have a lower limit
        assert_eq!(
            w.check(0, &[r(Part::Disk, "disk0/disk-temp", 71.0)])[0].limit,
            70.0
        );
    }

    #[test]
    fn log_keeps_minute_maxima_for_a_day() {
        let mut log = ThermalLog::default();
        log.observe(60, &[r(Part::Cpu, "a", 70.0), r(Part::Disk, "d", 50.0)]);
        log.observe(90, &[r(Part::Cpu, "b", 80.0)]);
        log.observe(130, &[r(Part::Gpu, "g", 60.0)]);
        assert_eq!(log.minutes.len(), 2);
        assert_eq!(log.minutes[0].cpu, Some(80.0));
        assert_eq!(log.minutes[0].hottest[&Part::Cpu], "dev-b");
        assert_eq!(log.max(Part::Cpu), Some((80.0, "dev-b".into())));
        log.observe(60 + 24 * 3600, &[r(Part::Cpu, "a", 40.0)]);
        assert_eq!(log.minutes.len(), 2, "minute 1 fell out of the window");
        assert_eq!(log.max(Part::Disk), None);
        let json = serde_json::to_string(&log).expect("json");
        let back: ThermalLog = serde_json::from_str(&json).expect("back");
        assert_eq!(back, log);
        let mut old = back;
        old.prune(60 + 48 * 3600);
        assert!(old.minutes.is_empty());
    }

    #[test]
    fn drives_use_their_own_warning_temperature() {
        let nvme = Reading {
            hot: Some(80.0),
            ..r(Part::Disk, "disk1/disk-temp", 72.0)
        };
        assert_eq!(nvme.limits(), (70.0, 80.0));
        let mut w = Watch::default();
        assert!(
            w.check(0, std::slice::from_ref(&nvme)).is_empty(),
            "72 is fine for an 80-degree drive"
        );
        let mut log = ThermalLog::default();
        log.observe(0, &[nvme.clone(), r(Part::Disk, "disk0/disk-temp", 66.0)]);
        // 66 on a drive with the 70-degree default is closer to its limit than 72 / 80
        assert_eq!(
            log.max(Part::Disk),
            Some((66.0, "dev-disk0/disk-temp".into()))
        );
        let f = findings(&log);
        assert!(f[0].part == Part::Disk && !f[0].hot);
        let mut only_nvme = ThermalLog::default();
        only_nvme.observe(0, &[nvme]);
        assert_eq!(
            findings(&only_nvme)[0].max,
            72.0,
            "warm: within 10 degrees of 80"
        );
    }

    #[test]
    fn findings_rate_the_hottest_minute() {
        let mut log = ThermalLog::default();
        log.observe(
            0,
            &[
                r(Part::Cpu, "c", 97.0),
                r(Part::Gpu, "g", 82.0),
                r(Part::Disk, "d", 45.0),
            ],
        );
        let f = findings(&log);
        assert_eq!(f.len(), 2);
        assert!(f[0].hot && f[0].part == Part::Cpu && f[0].penalty == 8);
        assert!(!f[1].hot && f[1].part == Part::Gpu && f[1].penalty == 2);
        assert!(findings(&ThermalLog::default()).is_empty());
    }
}
