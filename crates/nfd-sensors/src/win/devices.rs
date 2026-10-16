//! Disks (temperature through the storage driver, activity / throughput through
//! PDH), batteries and network adapters.

use std::time::Duration;

use nfd_hwinfo::win::Handle;

use super::pdh::{Counter, Query};
use crate::{Device, DeviceKind, Discovery, Kind, SensorInfo, SensorProvider};

/// "0 C: D:" → 0 (the PhysicalDisk instance's disk number).
pub fn disk_number(instance: &str) -> Option<u32> {
    instance.split_whitespace().next()?.parse().ok()
}

#[derive(Debug)]
struct Disk {
    index: u32,
    handle: Option<Handle>,
    temp: Option<usize>,
    read: Option<usize>,
    write: Option<usize>,
    active: Option<usize>,
}

#[derive(Debug)]
pub struct StorageProvider {
    disks: Vec<Disk>,
    pdh: Option<(Query, Counter, Counter, Counter)>,
}

impl Default for StorageProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageProvider {
    pub fn new() -> Self {
        Self {
            disks: Vec::new(),
            pdh: None,
        }
    }
}

impl SensorProvider for StorageProvider {
    fn id(&self) -> &'static str {
        "storage"
    }

    fn discover(&mut self) -> Discovery {
        let mut d = Discovery::default();
        let mut pdh = Query::new();
        let counters = pdh.as_mut().map(|q| {
            (
                q.add(r"\PhysicalDisk(*)\Disk Read Bytes/sec"),
                q.add(r"\PhysicalDisk(*)\Disk Write Bytes/sec"),
                q.add(r"\PhysicalDisk(*)\% Idle Time"),
            )
        });
        if let (Some(mut q), Some((Some(r), Some(w), Some(i)))) = (pdh, counters) {
            q.collect();
            self.pdh = Some((q, r, w, i));
        }
        for disk in nfd_hwinfo::win::storage::native() {
            let id = format!("disk{}", disk.index);
            d.devices.push(Device {
                id: id.clone(),
                kind: DeviceKind::Storage,
                name: disk.model.clone(),
            });
            let mut entry = Disk {
                index: disk.index,
                handle: nfd_hwinfo::win::storage::open(disk.index),
                temp: None,
                read: None,
                write: None,
                active: None,
            };
            let mut add = |s: SensorInfo| {
                d.sensors.push(s);
                Some(d.sensors.len() - 1)
            };
            if disk.temperature_c.is_some() {
                entry.temp = add(SensorInfo::new(
                    &id,
                    Kind::Temperature,
                    "disk-temp",
                    "IOCTL_STORAGE_QUERY_PROPERTY",
                )
                .primary());
            }
            if self.pdh.is_some() {
                entry.active = add(SensorInfo::new(
                    &id,
                    Kind::Load,
                    "disk-active",
                    "PDH PhysicalDisk",
                ));
                entry.read = add(SensorInfo::new(
                    &id,
                    Kind::Throughput,
                    "disk-read",
                    "PDH PhysicalDisk",
                ));
                entry.write = add(SensorInfo::new(
                    &id,
                    Kind::Throughput,
                    "disk-write",
                    "PDH PhysicalDisk",
                ));
            }
            self.disks.push(entry);
        }
        d
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        for disk in &self.disks {
            if let (Some(i), Some(h)) = (disk.temp, &disk.handle) {
                if let Some(t) = nfd_hwinfo::win::storage::temperature_of(h) {
                    out.push((i, f64::from(t)));
                }
            }
        }
        let Some((q, r, w, idle)) = &mut self.pdh else {
            return;
        };
        if !q.collect() {
            return;
        }
        for (counter, pick) in [(*r, 0), (*w, 1), (*idle, 2)] {
            for (inst, v) in q.values(counter) {
                let Some(n) = disk_number(&inst) else {
                    continue;
                };
                let Some(disk) = self.disks.iter().find(|d| d.index == n) else {
                    continue;
                };
                let slot = match pick {
                    0 => disk.read,
                    1 => disk.write,
                    _ => disk.active,
                };
                if let Some(i) = slot {
                    out.push((
                        i,
                        if pick == 2 {
                            (100.0 - v).clamp(0.0, 100.0)
                        } else {
                            v
                        },
                    ));
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct BatteryProvider {
    count: usize,
    design: Vec<u32>,
}

impl SensorProvider for BatteryProvider {
    fn id(&self) -> &'static str {
        "battery"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn discover(&mut self) -> Discovery {
        let mut d = Discovery::default();
        let bats = nfd_hwinfo::win::battery::native();
        self.count = bats.len();
        for (i, b) in bats.iter().enumerate() {
            let id = format!("battery{i}");
            self.design.push(b.design_mwh);
            d.devices.push(Device {
                id: id.clone(),
                kind: DeviceKind::Battery,
                name: b.name.clone().unwrap_or_else(|| "Battery".into()),
            });
            let src = "IOCTL_BATTERY_QUERY_STATUS";
            d.sensors.extend([
                SensorInfo::new(&id, Kind::Level, "bat-charge", src).primary(),
                SensorInfo::new(&id, Kind::Level, "bat-wear", src),
                SensorInfo::new(&id, Kind::Power, "bat-rate", src).primary(),
                SensorInfo::new(&id, Kind::Voltage, "bat-voltage", src),
                SensorInfo::new(&id, Kind::Energy, "bat-remaining", src),
                SensorInfo::new(&id, Kind::Energy, "bat-full", src),
                SensorInfo::new(&id, Kind::Energy, "bat-design", src),
            ]);
        }
        d
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        for (i, s) in nfd_hwinfo::win::battery::status()
            .iter()
            .enumerate()
            .take(self.count)
        {
            let base = i * 7;
            let design = self.design.get(i).copied().unwrap_or(0);
            if s.full_mwh > 0 {
                out.push((
                    base,
                    (f64::from(s.capacity_mwh) * 100.0 / f64::from(s.full_mwh)).min(100.0),
                ));
            }
            if design > 0 && s.full_mwh > 0 {
                out.push((
                    base + 1,
                    (100.0 - f64::from(s.full_mwh) * 100.0 / f64::from(design)).max(0.0),
                ));
            }
            if let Some(r) = s.rate_mw {
                out.push((base + 2, f64::from(r) / 1000.0));
            }
            if let Some(v) = s.voltage_mv {
                out.push((base + 3, f64::from(v) / 1000.0));
            }
            out.push((base + 4, f64::from(s.capacity_mwh)));
            out.push((base + 5, f64::from(s.full_mwh)));
            if design > 0 {
                out.push((base + 6, f64::from(design)));
            }
        }
    }
}

/// PDH spells interface instances with `(`→`[`, `)`→`]`, `#`/`/`/`\`→`_`.
pub fn pdh_instance_name(description: &str) -> String {
    description
        .chars()
        .map(|c| match c {
            '(' => '[',
            ')' => ']',
            '#' | '/' | '\\' => '_',
            c => c,
        })
        .collect()
}

#[derive(Debug)]
pub struct NetworkProvider {
    names: Vec<String>,
    pdh: Option<(Query, Counter, Counter)>,
}

impl Default for NetworkProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkProvider {
    pub fn new() -> Self {
        Self {
            names: Vec::new(),
            pdh: None,
        }
    }
}

impl SensorProvider for NetworkProvider {
    fn id(&self) -> &'static str {
        "network"
    }

    fn discover(&mut self) -> Discovery {
        let mut d = Discovery::default();
        let mut pdh = Query::new();
        let counters = pdh.as_mut().map(|q| {
            (
                q.add(r"\Network Interface(*)\Bytes Received/sec"),
                q.add(r"\Network Interface(*)\Bytes Sent/sec"),
            )
        });
        let (Some(mut q), Some((Some(rx), Some(tx)))) = (pdh, counters) else {
            return d;
        };
        // a rate counter lists its instances once it has two samples
        q.collect();
        q.collect();
        let present: Vec<String> = q
            .instances(rx)
            .into_iter()
            .map(|n| n.to_ascii_lowercase())
            .collect();
        for (i, a) in nfd_hwinfo::win::net::native().into_iter().enumerate() {
            let inst = pdh_instance_name(&a.name);
            if !present.contains(&inst.to_ascii_lowercase()) {
                continue;
            }
            let id = format!("net{i}");
            d.devices.push(Device {
                id: id.clone(),
                kind: DeviceKind::Network,
                name: a.name.clone(),
            });
            d.sensors.push(
                SensorInfo::new(&id, Kind::Throughput, "net-down", "PDH Network Interface")
                    .primary(),
            );
            d.sensors.push(
                SensorInfo::new(&id, Kind::Throughput, "net-up", "PDH Network Interface").primary(),
            );
            self.names.push(inst.to_ascii_lowercase());
        }
        self.pdh = Some((q, rx, tx));
        d
    }

    fn sample(&mut self, out: &mut Vec<(usize, f64)>) {
        let Some((q, rx, tx)) = &mut self.pdh else {
            return;
        };
        if !q.collect() {
            return;
        }
        for (counter, off) in [(*rx, 0), (*tx, 1)] {
            for (inst, v) in q.values(counter) {
                if let Some(i) = self
                    .names
                    .iter()
                    .position(|n| *n == inst.to_ascii_lowercase())
                {
                    out.push((i * 2 + off, v));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_numbers() {
        assert_eq!(disk_number("0 C: D:"), Some(0));
        assert_eq!(disk_number("12"), Some(12));
        assert_eq!(disk_number("_Total"), None);
    }

    #[test]
    fn pdh_names() {
        assert_eq!(
            pdh_instance_name("Intel(R) Wi-Fi 6E AX211 160MHz"),
            "Intel[R] Wi-Fi 6E AX211 160MHz"
        );
        assert_eq!(pdh_instance_name("Realtek #2"), "Realtek _2");
    }
}
