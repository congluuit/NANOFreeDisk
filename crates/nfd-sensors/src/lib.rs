//! Live hardware sensors for Hardware → Sensors, the Home temperature card and
//! `nfd sensors` (`docs/HARDWARE_MONITOR.md` §3–4).
//!
//! A [`SensorHub`] owns a set of [`SensorProvider`]s, samples them on demand (the
//! caller decides the cadence — nothing runs while no view asks) and keeps the
//! current value, min / max and a 10-minute history per sensor.
//!
//! The default Windows providers need no driver and no admin rights: CPU load and
//! estimated clock, memory, ACPI thermal zones, GPUs (D3DKMT + NVML), disks and
//! batteries. CPU package/core temperatures, power and voltages need the opt-in
//! PawnIO helper (ADR 0002); without it the hub reports a [`Note`] instead of a
//! value. Hardware is only ever read — nothing here writes a register, a fan curve,
//! a voltage or a limit.

#[cfg(windows)]
pub mod win;

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// What a sensor measures; fixes its unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// °C
    Temperature,
    /// V
    Voltage,
    /// W
    Power,
    /// MHz
    Clock,
    /// %
    Load,
    /// RPM
    Fan,
    /// % of max fan speed
    Control,
    /// bytes
    Data,
    /// bytes / s
    Throughput,
    /// % (battery charge, wear)
    Level,
    /// mWh
    Energy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Cpu,
    Gpu,
    Memory,
    Storage,
    Battery,
    Board,
    Network,
}

/// A piece of hardware the sensors belong to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub kind: DeviceKind,
    /// Hardware name ("NVIDIA GeForce RTX 4050 Laptop GPU"); not translated.
    pub name: String,
}

/// Static description of one sensor.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SensorInfo {
    /// Stable id, `device/label[/index]`.
    pub id: String,
    pub device: String,
    pub kind: Kind,
    /// Message key suffix for the UI (`hw-s-<label>`), e.g. `cpu-total`.
    pub label: String,
    /// Core / thread / zone number for labels like "Thread #3".
    pub index: Option<u32>,
    /// Untranslated detail: "P" / "E" core type, ACPI zone name, engine name.
    pub detail: Option<String>,
    /// Shown in Simple mode and used by the Home card.
    pub primary: bool,
    /// Approximation (e.g. clock from `% Processor Performance`), flagged in the UI.
    pub estimated: bool,
    /// Where the value comes from ("NtQuerySystemInformation", "D3DKMT", "NVML"…).
    pub source: &'static str,
}

impl SensorInfo {
    pub fn new(device: &str, kind: Kind, label: &str, source: &'static str) -> Self {
        Self {
            id: format!("{device}/{label}"),
            device: device.to_owned(),
            kind,
            label: label.to_owned(),
            index: None,
            detail: None,
            primary: false,
            estimated: false,
            source,
        }
    }

    pub fn index(mut self, i: u32) -> Self {
        self.id = format!("{}/{}/{i}", self.device, self.label);
        self.index = Some(i);
        self
    }

    pub fn detail(mut self, d: impl Into<String>) -> Self {
        let d = d.into();
        if self.index.is_none() {
            // keep ids unique for per-name sensors (thermal zones, engines)
            self.id = format!("{}/{}/{}", self.device, self.label, d.to_ascii_lowercase());
        }
        self.detail = Some(d);
        self
    }

    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    pub fn estimated(mut self) -> Self {
        self.estimated = true;
        self
    }
}

/// Why a group of sensors is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub device: String,
    pub kind: Kind,
    /// `needs-pawnio`, `gpu-asleep`, `not-reported`…
    pub reason: String,
}

/// Sensors a provider found.
#[derive(Debug, Default)]
pub struct Discovery {
    pub devices: Vec<Device>,
    pub sensors: Vec<SensorInfo>,
    pub notes: Vec<Note>,
}

/// One source of readings. `sample` pushes `(index into discover().sensors, value)`;
/// a sensor left out of a sample keeps its last value but is marked stale.
pub trait SensorProvider: Send {
    fn id(&self) -> &'static str;
    fn discover(&mut self) -> Discovery;
    fn sample(&mut self, out: &mut Vec<(usize, f64)>);
    /// Minimum time between two samples (slow sources: batteries, disks).
    fn interval(&self) -> Duration {
        Duration::from_secs(1)
    }
    /// Notes that change while running (a GPU going to sleep).
    fn live_notes(&self) -> Vec<Note> {
        Vec::new()
    }
}

/// 10 minutes at one sample per second.
pub const HISTORY: usize = 600;

#[derive(Debug, Clone, Default)]
struct Stat {
    value: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    history: VecDeque<f32>,
    /// Sample sequence number of the last update.
    seen: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SensorValue {
    #[serde(flatten)]
    pub info: SensorInfo,
    pub value: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// Everything the Sensors view shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub devices: Vec<Device>,
    pub sensors: Vec<SensorValue>,
    pub notes: Vec<Note>,
    /// Samples taken since the hub started.
    pub samples: u64,
    /// Wall time of the last sample, ms.
    pub sample_ms: f64,
}

struct Slot {
    provider: Box<dyn SensorProvider>,
    /// Provider-local sensor index -> hub index (None: dropped duplicate).
    map: Vec<Option<usize>>,
    next: Option<Instant>,
}

/// Owns the providers and the statistics.
pub struct SensorHub {
    slots: Vec<Slot>,
    devices: Vec<Device>,
    sensors: Vec<SensorInfo>,
    stats: Vec<Stat>,
    notes: Vec<Note>,
    by_id: HashMap<String, usize>,
    seq: u64,
    sample_ms: f64,
    last: Option<Instant>,
    buf: Vec<(usize, f64)>,
}

impl std::fmt::Debug for SensorHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SensorHub")
            .field(
                "providers",
                &self
                    .slots
                    .iter()
                    .map(|s| s.provider.id())
                    .collect::<Vec<_>>(),
            )
            .field("sensors", &self.sensors.len())
            .finish()
    }
}

impl SensorHub {
    pub fn new(providers: Vec<Box<dyn SensorProvider>>) -> Self {
        let mut hub = Self {
            slots: Vec::new(),
            devices: Vec::new(),
            sensors: Vec::new(),
            stats: Vec::new(),
            notes: Vec::new(),
            by_id: HashMap::new(),
            seq: 0,
            sample_ms: 0.0,
            last: None,
            buf: Vec::new(),
        };
        for mut p in providers {
            let d = p.discover();
            let mut map = Vec::with_capacity(d.sensors.len());
            for s in d.sensors {
                // a duplicate id would make the UI tree ambiguous: keep the first
                if hub.by_id.contains_key(&s.id) {
                    map.push(None);
                    continue;
                }
                map.push(Some(hub.sensors.len()));
                hub.by_id.insert(s.id.clone(), hub.sensors.len());
                hub.sensors.push(s);
            }
            for dev in d.devices {
                if !hub.devices.iter().any(|x| x.id == dev.id) {
                    hub.devices.push(dev);
                }
            }
            hub.notes.extend(d.notes);
            hub.slots.push(Slot {
                provider: p,
                map,
                next: None,
            });
        }
        hub.stats = vec![Stat::default(); hub.sensors.len()];
        hub
    }

    /// The default providers for this platform.
    pub fn native() -> Self {
        #[cfg(windows)]
        let providers = win::providers();
        #[cfg(not(windows))]
        let providers: Vec<Box<dyn SensorProvider>> = Vec::new();
        Self::new(providers)
    }

    /// Time since the last sample (None before the first).
    pub fn since_last(&self) -> Option<Duration> {
        self.last.map(|t| t.elapsed())
    }

    /// Sample every provider that is due.
    pub fn sample(&mut self) {
        let start = Instant::now();
        self.seq += 1;
        let total = self.sensors.len();
        for slot in &mut self.slots {
            if slot.next.is_some_and(|n| start < n) {
                // not due: keep its values current (slow sources)
                for &i in slot.map.iter().flatten() {
                    let st = &mut self.stats[i];
                    if let Some(v) = st.value {
                        st.seen = self.seq;
                        push_history(&mut st.history, v);
                    }
                }
                continue;
            }
            slot.next = Some(start + slot.provider.interval() - Duration::from_millis(100));
            self.buf.clear();
            slot.provider.sample(&mut self.buf);
            for &(local, v) in &self.buf {
                let Some(&Some(i)) = slot.map.get(local) else {
                    continue;
                };
                if i >= total || !v.is_finite() {
                    continue;
                }
                let st = &mut self.stats[i];
                st.value = Some(v);
                st.min = Some(st.min.map_or(v, |m| m.min(v)));
                st.max = Some(st.max.map_or(v, |m| m.max(v)));
                st.seen = self.seq;
                push_history(&mut st.history, v);
            }
        }
        // stale sensors lose their current value (min/max stay)
        for st in &mut self.stats {
            if st.seen != self.seq {
                st.value = None;
            }
        }
        self.sample_ms = start.elapsed().as_secs_f64() * 1000.0;
        self.last = Some(start);
    }

    pub fn reset_minmax(&mut self) {
        for st in &mut self.stats {
            st.min = st.value;
            st.max = st.value;
        }
    }

    /// Up to the last `n` values of a sensor, oldest first.
    pub fn history(&self, id: &str, n: usize) -> Vec<f32> {
        let Some(&i) = self.by_id.get(id) else {
            return Vec::new();
        };
        let h = &self.stats[i].history;
        h.iter().skip(h.len().saturating_sub(n)).copied().collect()
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut notes = self.notes.clone();
        for s in &self.slots {
            notes.extend(s.provider.live_notes());
        }
        Snapshot {
            devices: self.devices.clone(),
            sensors: self
                .sensors
                .iter()
                .zip(&self.stats)
                .map(|(info, st)| SensorValue {
                    info: info.clone(),
                    value: st.value,
                    min: st.min,
                    max: st.max,
                })
                .collect(),
            notes,
            samples: self.seq,
            sample_ms: self.sample_ms,
        }
    }
}

fn push_history(h: &mut VecDeque<f32>, v: f64) {
    if h.len() == HISTORY {
        h.pop_front();
    }
    h.push_back(v as f32);
}

/// Unit symbol for plain-text output.
pub fn unit(kind: Kind) -> &'static str {
    match kind {
        Kind::Temperature => "°C",
        Kind::Voltage => "V",
        Kind::Power => "W",
        Kind::Clock => "MHz",
        Kind::Load | Kind::Level | Kind::Control => "%",
        Kind::Fan => "RPM",
        Kind::Data => "B",
        Kind::Throughput => "B/s",
        Kind::Energy => "mWh",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        n: f64,
        slow: bool,
    }

    impl SensorProvider for Fake {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn discover(&mut self) -> Discovery {
            Discovery {
                devices: vec![Device {
                    id: "cpu".into(),
                    kind: DeviceKind::Cpu,
                    name: "Test CPU".into(),
                }],
                sensors: vec![
                    SensorInfo::new("cpu", Kind::Load, "cpu-total", "test").primary(),
                    SensorInfo::new("cpu", Kind::Load, "cpu-thread", "test").index(0),
                    SensorInfo::new("cpu", Kind::Load, "cpu-thread", "test").index(0),
                ],
                notes: vec![Note {
                    device: "cpu".into(),
                    kind: Kind::Temperature,
                    reason: "needs-pawnio".into(),
                }],
            }
        }
        fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
            self.n += 1.0;
            out.push((0, self.n * 10.0));
            if self.n < 3.0 {
                out.push((1, f64::NAN));
            } else {
                out.push((1, 5.0));
            }
        }
        fn interval(&self) -> Duration {
            if self.slow {
                Duration::from_secs(60)
            } else {
                Duration::ZERO
            }
        }
    }

    #[test]
    fn min_max_history_and_reset() {
        let mut hub = SensorHub::new(vec![Box::new(Fake {
            n: 0.0,
            slow: false,
        })]);
        let snap = hub.snapshot();
        assert_eq!(snap.sensors.len(), 2, "duplicate id dropped");
        assert_eq!(snap.sensors[0].value, None);
        for _ in 0..4 {
            hub.sample();
        }
        let snap = hub.snapshot();
        let total = &snap.sensors[0];
        assert_eq!(
            (total.value, total.min, total.max),
            (Some(40.0), Some(10.0), Some(40.0))
        );
        let thread = &snap.sensors[1];
        assert_eq!(thread.value, Some(5.0), "NaN samples are ignored");
        assert_eq!(hub.history("cpu/cpu-total", 2), vec![30.0, 40.0]);
        assert!(hub.history("nope", 2).is_empty());
        hub.reset_minmax();
        let s = &hub.snapshot().sensors[0];
        assert_eq!((s.min, s.max), (Some(40.0), Some(40.0)));
        assert_eq!(hub.snapshot().notes[0].reason, "needs-pawnio");
        assert_eq!(hub.snapshot().samples, 4);
    }

    #[test]
    fn slow_providers_keep_their_value_between_samples() {
        let mut hub = SensorHub::new(vec![Box::new(Fake { n: 0.0, slow: true })]);
        hub.sample();
        hub.sample();
        let s = &hub.snapshot().sensors[0];
        assert_eq!(s.value, Some(10.0), "sampled once, value kept");
        assert_eq!(hub.history("cpu/cpu-total", 10).len(), 2);
    }

    #[test]
    fn snapshot_serializes_flat() {
        let mut hub = SensorHub::new(vec![Box::new(Fake {
            n: 0.0,
            slow: false,
        })]);
        hub.sample();
        let json = serde_json::to_value(hub.snapshot()).expect("json");
        assert_eq!(json["sensors"][0]["label"], "cpu-total");
        assert_eq!(json["sensors"][0]["kind"], "load");
        assert_eq!(json["devices"][0]["kind"], "cpu");
    }

    #[test]
    fn native_hub_samples_without_admin() {
        let mut hub = SensorHub::native();
        hub.sample();
        std::thread::sleep(Duration::from_millis(300));
        hub.sample();
        let snap = hub.snapshot();
        #[cfg(windows)]
        {
            let total = snap
                .sensors
                .iter()
                .find(|s| s.info.id == "cpu/cpu-total")
                .expect("cpu total");
            let v = total.value.expect("cpu load value");
            assert!((0.0..=100.0).contains(&v), "{v}");
            assert!(
                snap.sensors
                    .iter()
                    .any(|s| s.info.id == "memory/mem-load" && s.value.is_some())
            );
        }
        let _ = snap;
    }
}
