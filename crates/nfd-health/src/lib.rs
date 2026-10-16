//! Drive diagnostics (PLAN §4.6, docs/PHASE7_HEALTH.md): SMART / NVMe health of
//! every physical drive, read-only volume checks, and a Good / Caution / Bad verdict
//! in the manner of CrystalDiskInfo.
//!
//! Strictly read-only. What a standard user can read (drive descriptors, the NVMe
//! health log, the ATA attribute table through the storage driver, BitLocker state,
//! cluster size, partition alignment) comes from [`collect`]. The rest — SMART
//! thresholds, SMART over USB bridges, the volume dirty bit — needs administrator
//! rights and comes from [`privileged`], run either in an elevated process (the
//! CLI in an admin terminal) or in the app's elevated helper, then folded in with
//! [`HealthReport::merge`]. The only commands ever sent to a drive are the fixed
//! SMART READ DATA / READ THRESHOLDS in [`cmd`].

pub mod alerts;
pub mod assess;
pub mod ata;
pub mod cmd;
pub mod nvme;

#[cfg(windows)]
pub mod win;

use serde::{Deserialize, Serialize};

/// JSON schema id of [`HealthReport`].
pub const SCHEMA: &str = "nanofreedisk.health.v1";

/// Verdict for a drive. Ordered from "nothing known" to worst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Unknown,
    Good,
    Caution,
    Bad,
}

/// Temperature against the drive's limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Heat {
    Unknown,
    Normal,
    Warning,
    Critical,
}

/// Where the health data came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// NVMe log page 02h.
    Nvme,
    /// ATA attribute table through `IOCTL_STORAGE_PREDICT_FAILURE` (no thresholds).
    AtaDriver,
    /// `SMART_RCV_DRIVE_DATA` with thresholds (administrator).
    AtaSmart,
    /// SCSI / ATA Translation through a USB bridge (administrator).
    Sat,
    /// Nothing could be read.
    None,
}

/// One reason behind a verdict. `key` is the message id suffix (`health-reason-*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reason {
    pub key: String,
    pub value: Option<f64>,
    /// ATA attribute id, for `ata-*` reasons.
    pub attr: Option<u8>,
}

/// ATA SMART as read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtaSmart {
    pub attributes: Vec<ata::Attribute>,
    /// The thresholds were read (else Bad can only come from `predict_failure`).
    pub thresholds: bool,
    pub predict_failure: Option<bool>,
}

/// One physical drive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    /// `N` of `\\.\PhysicalDriveN`.
    pub index: u32,
    pub model: String,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    /// "NVMe", "SATA", "USB"…
    pub bus: String,
    pub size: Option<u64>,
    pub ssd: Option<bool>,
    pub removable: bool,
    /// The device supports TRIM / UNMAP.
    pub trim: Option<bool>,
    pub status: Status,
    pub reasons: Vec<Reason>,
    /// °C.
    pub temperature: Option<f32>,
    pub temp_warning: Option<f32>,
    pub temp_critical: Option<f32>,
    pub heat: Heat,
    /// Percent (SSD).
    pub life_left: Option<u8>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    pub bytes_written: Option<u64>,
    pub bytes_read: Option<u64>,
    pub unsafe_shutdowns: Option<u64>,
    pub source: Source,
    /// Why SMART is missing or partial: `usb-bridge`, `needs-admin`, `raid`,
    /// `virtual`, `no-smart`.
    pub note: Option<String>,
    pub nvme: Option<nvme::NvmeLog>,
    pub ata: Option<AtaSmart>,
    /// Roots (`C:\`) of the volumes on this drive.
    pub volumes: Vec<String>,
}

/// One volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub root: String,
    pub label: String,
    pub fs: String,
    pub total: u64,
    pub free: u64,
    pub system: bool,
    pub removable: bool,
    /// Physical drive index.
    pub disk: Option<u32>,
    /// Partition start in bytes.
    pub offset: Option<u64>,
    /// Partition start on a 4 KiB boundary.
    pub aligned: Option<bool>,
    /// Bytes.
    pub cluster: Option<u32>,
    /// Windows sends TRIM for this file system (`DisableDeleteNotify` = 0).
    pub trim_enabled: Option<bool>,
    /// `on`, `off`, `encrypting`, `decrypting`, `suspended`, `locked`, `waiting`.
    pub bitlocker: Option<String>,
    /// `None` = not checked (needs administrator rights).
    pub dirty: Option<bool>,
    /// From the scan history (filled in by the caller).
    pub days_to_full: Option<f64>,
    pub has_trend: bool,
}

impl Volume {
    /// Free share in percent.
    #[must_use]
    pub fn free_percent(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            self.free as f64 * 100.0 / self.total as f64
        }
    }
}

/// Result of [`collect`] (`nanofreedisk.health.v1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub schema: String,
    /// Unix seconds.
    pub generated_at: i64,
    /// Administrator-only reads were included.
    pub elevated: bool,
    pub drives: Vec<Drive>,
    pub volumes: Vec<Volume>,
}

/// What the platform layer read for one drive, before interpretation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawDrive {
    pub index: u32,
    pub model: String,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub bus: String,
    pub size: Option<u64>,
    pub ssd: Option<bool>,
    pub removable: bool,
    pub trim: Option<bool>,
    pub temperature: Option<f32>,
    pub temp_warning: Option<f32>,
    pub temp_critical: Option<f32>,
    pub nvme_log: Option<Vec<u8>>,
    /// SMART data sector, from whichever path worked.
    pub smart: Option<Vec<u8>>,
    pub thresholds: Option<Vec<u8>>,
    pub smart_via: Option<Source>,
    pub predict_failure: Option<bool>,
}

/// SMART sectors read with administrator rights for one drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegedSmart {
    pub disk: u32,
    pub data: Vec<u8>,
    pub thresholds: Option<Vec<u8>>,
    /// `AtaSmart` or `Sat`.
    pub via: Source,
}

/// Dirty bit of one volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegedVolume {
    pub root: String,
    pub dirty: bool,
}

/// Everything [`privileged`] read (sent over the helper pipe as is).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Privileged {
    pub smart: Vec<PrivilegedSmart>,
    pub volumes: Vec<PrivilegedVolume>,
    /// USB drives the bridge did not answer for.
    pub sat_failed: Vec<u32>,
}

fn is_usb(bus: &str) -> bool {
    bus == "USB"
}

impl Drive {
    /// Interpret what was read.
    #[must_use]
    pub fn build(raw: RawDrive, volumes: Vec<String>) -> Self {
        let mut d = Self {
            index: raw.index,
            model: raw.model,
            serial: raw.serial,
            firmware: raw.firmware,
            bus: raw.bus,
            size: raw.size,
            ssd: raw.ssd,
            removable: raw.removable,
            trim: raw.trim,
            status: Status::Unknown,
            reasons: Vec::new(),
            temperature: raw.temperature,
            temp_warning: raw.temp_warning,
            temp_critical: raw.temp_critical,
            heat: Heat::Unknown,
            life_left: None,
            power_on_hours: None,
            power_cycles: None,
            bytes_written: None,
            bytes_read: None,
            unsafe_shutdowns: None,
            source: Source::None,
            note: None,
            nvme: None,
            ata: None,
            volumes,
        };
        if let Some(log) = raw.nvme_log.as_deref().and_then(nvme::NvmeLog::parse) {
            let (status, reasons) = assess::nvme(&log);
            d.status = status;
            d.reasons = reasons;
            d.temperature = d.temperature.or(log.temperature.map(|t| t as f32));
            d.life_left = Some(log.life_left());
            d.power_on_hours = Some(log.power_on_hours);
            d.power_cycles = Some(log.power_cycles);
            d.bytes_written = Some(log.bytes_written());
            d.bytes_read = Some(log.bytes_read());
            d.unsafe_shutdowns = Some(log.unsafe_shutdowns);
            d.source = Source::Nvme;
            d.ssd = Some(true);
            d.nvme = Some(log);
        } else if let Some(data) = raw.smart.as_deref() {
            d.apply_smart(
                data,
                raw.thresholds.as_deref(),
                raw.smart_via.unwrap_or(Source::AtaDriver),
                raw.predict_failure,
            );
        } else if raw.predict_failure.is_some() {
            d.apply_smart(&[], None, Source::AtaDriver, raw.predict_failure);
        }
        if d.source == Source::None {
            d.note = Some(
                match d.bus.as_str() {
                    "USB" => "usb-bridge",
                    "RAID" | "Storage Spaces" => "raid",
                    "Virtual" => "virtual",
                    "SATA" | "ATA" => "needs-admin",
                    _ => "no-smart",
                }
                .to_owned(),
            );
        }
        d.heat = assess::heat(d.temperature, d.temp_warning, d.temp_critical, d.ssd);
        d
    }

    /// Use an ATA attribute table (and thresholds when known).
    pub fn apply_smart(
        &mut self,
        data: &[u8],
        thresholds: Option<&[u8]>,
        via: Source,
        predict_failure: Option<bool>,
    ) {
        let attributes = ata::parse(data, thresholds);
        let derived = ata::derive(&attributes, self.ssd != Some(false));
        let predict_failure = predict_failure.or(self.ata.as_ref().and_then(|a| a.predict_failure));
        let (status, reasons) = assess::ata(&attributes, predict_failure, derived.life_left);
        if status == Status::Unknown {
            return;
        }
        self.status = status;
        self.reasons = reasons;
        self.temperature = self.temperature.or(derived.temperature.map(|t| t as f32));
        self.life_left = derived.life_left;
        self.power_on_hours = derived.power_on_hours;
        self.power_cycles = derived.power_cycles;
        self.bytes_written = derived.bytes_written;
        self.bytes_read = derived.bytes_read;
        self.source = via;
        self.note =
            (thresholds.is_none() && via == Source::AtaDriver).then(|| "needs-admin".to_owned());
        self.ata = Some(AtaSmart {
            thresholds: thresholds.is_some_and(|t| t.len() >= ata::SECTOR),
            attributes,
            predict_failure,
        });
        self.heat = assess::heat(
            self.temperature,
            self.temp_warning,
            self.temp_critical,
            self.ssd,
        );
    }
}

impl HealthReport {
    /// Fold in what was read with administrator rights.
    pub fn merge(&mut self, p: &Privileged) {
        for s in &p.smart {
            if let Some(d) = self.drives.iter_mut().find(|d| d.index == s.disk) {
                // NVMe already has everything; never downgrade it
                if d.source != Source::Nvme {
                    d.apply_smart(&s.data, s.thresholds.as_deref(), s.via, None);
                }
            }
        }
        for &disk in &p.sat_failed {
            if let Some(d) = self.drives.iter_mut().find(|d| d.index == disk) {
                if d.source == Source::None {
                    d.note = Some("usb-bridge".to_owned());
                }
            }
        }
        for v in &p.volumes {
            if let Some(mine) = self
                .volumes
                .iter_mut()
                .find(|m| m.root.eq_ignore_ascii_case(&v.root))
            {
                mine.dirty = Some(v.dirty);
            }
        }
        self.elevated = true;
    }

    /// Worst drive status.
    #[must_use]
    pub fn worst(&self) -> Status {
        self.drives
            .iter()
            .map(|d| d.status)
            .max()
            .unwrap_or(Status::Unknown)
    }

    /// CLI exit code: 0 all good / unknown, 1 a drive needs attention, 2 a drive is bad.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self.worst() {
            Status::Bad => 2,
            Status::Caution => 1,
            _ => 0,
        }
    }

    /// Drop serial numbers (for reports sent to someone else).
    pub fn redact(&mut self) {
        for d in &mut self.drives {
            d.serial = None;
        }
    }
}

/// Options for [`collect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Include volumes.
    pub volumes: bool,
    /// When this process is elevated, also do the [`privileged`] reads.
    pub privileged: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            volumes: true,
            privileged: true,
        }
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Read every drive and volume this process can read.
#[must_use]
pub fn collect(opts: Options) -> HealthReport {
    #[cfg(windows)]
    {
        let volumes = if opts.volumes {
            win::volumes()
        } else {
            Vec::new()
        };
        let drives = win::raw_drives()
            .into_iter()
            .map(|raw| {
                let on: Vec<String> = volumes
                    .iter()
                    .filter(|v| v.disk == Some(raw.index))
                    .map(|v| v.root.clone())
                    .collect();
                Drive::build(raw, on)
            })
            .collect();
        let mut report = HealthReport {
            schema: SCHEMA.to_owned(),
            generated_at: unix_now(),
            elevated: false,
            drives,
            volumes,
        };
        if opts.privileged && win::is_elevated() {
            let p = privileged(&report);
            report.merge(&p);
        }
        report
    }
    #[cfg(not(windows))]
    {
        let _ = opts;
        HealthReport {
            schema: SCHEMA.to_owned(),
            generated_at: unix_now(),
            elevated: false,
            drives: Vec::new(),
            volumes: Vec::new(),
        }
    }
}

/// The administrator-only reads for the drives and volumes of `report`: SMART with
/// thresholds for ATA drives, SAT for USB drives, the dirty bit for every volume.
/// Fails quietly (empty lists) without rights.
#[must_use]
pub fn privileged(report: &HealthReport) -> Privileged {
    #[cfg(windows)]
    {
        let mut p = Privileged::default();
        for d in report.drives.iter().filter(|d| d.source != Source::Nvme) {
            if is_usb(&d.bus) {
                match win::sat_smart(d.index) {
                    Some((data, thresholds)) => p.smart.push(PrivilegedSmart {
                        disk: d.index,
                        data,
                        thresholds,
                        via: Source::Sat,
                    }),
                    None => p.sat_failed.push(d.index),
                }
            } else if let Some((data, thresholds)) = win::ata_smart(d.index) {
                p.smart.push(PrivilegedSmart {
                    disk: d.index,
                    data,
                    thresholds,
                    via: Source::AtaSmart,
                });
            }
        }
        for v in &report.volumes {
            if let Some(dirty) = win::volume_dirty(&v.root) {
                p.volumes.push(PrivilegedVolume {
                    root: v.root.clone(),
                    dirty,
                });
            }
        }
        p
    }
    #[cfg(not(windows))]
    {
        let _ = (report, is_usb);
        Privileged::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nvme_raw() -> RawDrive {
        RawDrive {
            index: 1,
            model: "SAMSUNG MZVLB512HAJQ-000L7".into(),
            bus: "NVMe".into(),
            ssd: Some(true),
            temp_warning: Some(81.0),
            temp_critical: Some(82.0),
            nvme_log: Some(nvme::tests::pm981()),
            ..RawDrive::default()
        }
    }

    #[test]
    fn nvme_drive() {
        let d = Drive::build(nvme_raw(), vec![r"C:\".into()]);
        assert_eq!(d.status, Status::Good);
        assert_eq!(d.source, Source::Nvme);
        assert_eq!(d.life_left, Some(98));
        assert_eq!(d.temperature, Some(52.0));
        assert_eq!(d.heat, Heat::Normal);
        assert_eq!(d.note, None);
        assert_eq!(d.volumes, vec![r"C:\".to_owned()]);
    }

    #[test]
    fn sata_without_then_with_admin() {
        let (data, thr) = ata::tests::sectors(&ata::tests::hdd_failing());
        let raw = RawDrive {
            index: 0,
            model: "ST2000DM001".into(),
            bus: "SATA".into(),
            ssd: Some(false),
            smart: Some(data.clone()),
            predict_failure: Some(false),
            ..RawDrive::default()
        };
        let d = Drive::build(raw, Vec::new());
        assert_eq!(d.status, Status::Caution);
        assert_eq!(d.source, Source::AtaDriver);
        assert_eq!(d.note.as_deref(), Some("needs-admin"));
        assert_eq!(d.temperature, Some(38.0));
        assert_eq!(d.heat, Heat::Normal);
        let mut report = HealthReport {
            schema: SCHEMA.into(),
            generated_at: 0,
            elevated: false,
            drives: vec![d, Drive::build(nvme_raw(), Vec::new())],
            volumes: vec![Volume {
                root: r"C:\".into(),
                label: String::new(),
                fs: "NTFS".into(),
                total: 100,
                free: 5,
                system: true,
                removable: false,
                disk: Some(1),
                offset: Some(1 << 20),
                aligned: Some(true),
                cluster: Some(4096),
                trim_enabled: Some(true),
                bitlocker: Some("off".into()),
                dirty: None,
                days_to_full: None,
                has_trend: false,
            }],
        };
        assert_eq!(report.exit_code(), 1);
        report.merge(&Privileged {
            smart: vec![PrivilegedSmart {
                disk: 0,
                data,
                thresholds: Some(thr),
                via: Source::AtaSmart,
            }],
            volumes: vec![PrivilegedVolume {
                root: r"c:\".into(),
                dirty: true,
            }],
            sat_failed: Vec::new(),
        });
        let d = &report.drives[0];
        assert_eq!(d.status, Status::Bad);
        assert_eq!(d.source, Source::AtaSmart);
        assert_eq!(d.note, None);
        assert!(d.ata.as_ref().is_some_and(|a| a.thresholds));
        assert_eq!(report.volumes[0].dirty, Some(true));
        assert!(report.elevated);
        assert_eq!(report.exit_code(), 2);
        assert!((report.volumes[0].free_percent() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn unreadable_drives_get_a_note() {
        let note = |bus: &str| {
            Drive::build(
                RawDrive {
                    bus: bus.into(),
                    ..RawDrive::default()
                },
                Vec::new(),
            )
            .note
        };
        assert_eq!(note("USB").as_deref(), Some("usb-bridge"));
        assert_eq!(note("RAID").as_deref(), Some("raid"));
        assert_eq!(note("SATA").as_deref(), Some("needs-admin"));
        assert_eq!(note("SD").as_deref(), Some("no-smart"));
        let d = Drive::build(RawDrive::default(), Vec::new());
        assert_eq!((d.status, d.source), (Status::Unknown, Source::None));
    }

    #[test]
    fn report_json_shape() {
        let mut r = HealthReport {
            schema: SCHEMA.into(),
            generated_at: 1,
            elevated: false,
            drives: vec![Drive::build(
                RawDrive {
                    serial: Some("S4DXNF0M".into()),
                    ..nvme_raw()
                },
                Vec::new(),
            )],
            volumes: Vec::new(),
        };
        r.redact();
        let v = serde_json::to_value(&r).expect("json");
        assert_eq!(v["schema"], SCHEMA);
        assert_eq!(v["drives"][0]["status"], "good");
        assert_eq!(v["drives"][0]["source"], "nvme");
        assert_eq!(v["drives"][0]["serial"], serde_json::Value::Null);
        assert_eq!(v["drives"][0]["nvme"]["percentageUsed"], 2);
        let back: HealthReport = serde_json::from_value(v).expect("round trip");
        assert_eq!(back, r);
    }
}

#[cfg(test)]
mod schema_tests {
    /// Every field the report writes is described in `docs/schema/health-v1.schema.json`.
    #[test]
    fn schema_covers_the_fields() {
        let schema = include_str!("../../../docs/schema/health-v1.schema.json");
        let (data, thr) = crate::ata::tests::sectors(&crate::ata::tests::ssd_good());
        let drive = crate::Drive::build(
            crate::RawDrive {
                nvme_log: Some(crate::nvme::tests::pm981()),
                smart: Some(data),
                thresholds: Some(thr),
                ..crate::RawDrive::default()
            },
            Vec::new(),
        );
        let mut v = serde_json::to_value(&drive).expect("json");
        let mut ata = drive.clone();
        ata.nvme = None;
        ata.apply_smart(
            &crate::ata::tests::sectors(&crate::ata::tests::ssd_good()).0,
            None,
            crate::Source::AtaDriver,
            None,
        );
        v["ata"] = serde_json::to_value(&ata.ata).expect("json");
        let mut keys: Vec<String> = Vec::new();
        let mut walk = vec![v];
        while let Some(x) = walk.pop() {
            match x {
                serde_json::Value::Object(m) => {
                    for (k, val) in m {
                        keys.push(k);
                        walk.push(val);
                    }
                }
                serde_json::Value::Array(a) => walk.extend(a),
                _ => {}
            }
        }
        for k in keys {
            assert!(
                schema.contains(&format!("\"{k}\"")),
                "{k} missing from the schema"
            );
        }
    }
}
