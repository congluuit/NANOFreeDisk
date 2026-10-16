//! Good / Caution / Bad in the manner of CrystalDiskInfo (docs/PHASE7_HEALTH.md §2.2).
//! Pure functions over parsed data, so the rules are the same in the app and the CLI.

use crate::ata::Attribute;
use crate::nvme::{NvmeLog, warning};
use crate::{Reason, Status};

/// SSD life at or below this percentage is a Caution.
pub const LOW_LIFE: u8 = 10;

/// Raw counters whose any non-zero value is a Caution (CrystalDiskInfo's default:
/// reallocated, pending and uncorrectable sectors).
const CAUTION_RAW: [(u8, &str); 3] = [
    (0x05, "ata-reallocated"),
    (0xC5, "ata-pending"),
    (0xC6, "ata-uncorrectable"),
];

fn reason(key: &str, value: Option<f64>, attr: Option<u8>) -> Reason {
    Reason {
        key: key.to_owned(),
        value,
        attr,
    }
}

/// Status of an NVMe drive from its health log.
#[must_use]
pub fn nvme(log: &NvmeLog) -> (Status, Vec<Reason>) {
    let mut bad = Vec::new();
    let mut caution = Vec::new();
    let w = log.critical_warning;
    if w & warning::SPARE != 0 || log.available_spare < log.spare_threshold {
        bad.push(reason(
            "nvme-spare-low",
            Some(f64::from(log.available_spare)),
            None,
        ));
    }
    if w & warning::RELIABILITY != 0 {
        bad.push(reason("nvme-reliability", None, None));
    }
    if w & warning::READ_ONLY != 0 {
        bad.push(reason("nvme-read-only", None, None));
    }
    if w & warning::BACKUP != 0 {
        bad.push(reason("nvme-backup", None, None));
    }
    if w & warning::TEMPERATURE != 0 {
        caution.push(reason(
            "nvme-temperature",
            log.temperature.map(f64::from),
            None,
        ));
    }
    if log.life_left() <= LOW_LIFE {
        caution.push(reason("worn", Some(f64::from(log.life_left())), None));
    }
    if log.media_errors > 0 {
        caution.push(reason(
            "nvme-media-errors",
            Some(log.media_errors as f64),
            None,
        ));
    }
    finish(bad, caution)
}

/// Status of an ATA drive from its attribute table. `predict_failure` is the
/// drive's own SMART RETURN STATUS / `IOCTL_STORAGE_PREDICT_FAILURE` flag.
#[must_use]
pub fn ata(
    attrs: &[Attribute],
    predict_failure: Option<bool>,
    life_left: Option<u8>,
) -> (Status, Vec<Reason>) {
    let mut bad = Vec::new();
    let mut caution = Vec::new();
    if predict_failure == Some(true) {
        bad.push(reason("ata-predict-failure", None, None));
    }
    for a in attrs.iter().filter(|a| a.failing()) {
        bad.push(reason(
            "ata-threshold",
            Some(f64::from(a.current)),
            Some(a.id),
        ));
    }
    for (id, key) in CAUTION_RAW {
        if let Some(a) = attrs.iter().find(|a| a.id == id && a.raw > 0) {
            caution.push(reason(key, Some((a.raw & 0xFFFF_FFFF) as f64), Some(id)));
        }
    }
    if let Some(life) = life_left.filter(|&l| l <= LOW_LIFE) {
        caution.push(reason("worn", Some(f64::from(life)), None));
    }
    if attrs.is_empty() && predict_failure.is_none() {
        return (Status::Unknown, Vec::new());
    }
    finish(bad, caution)
}

fn finish(bad: Vec<Reason>, caution: Vec<Reason>) -> (Status, Vec<Reason>) {
    let status = if !bad.is_empty() {
        Status::Bad
    } else if !caution.is_empty() {
        Status::Caution
    } else {
        Status::Good
    };
    (status, bad.into_iter().chain(caution).collect())
}

/// Temperature level against the drive's own limits, else 60 °C (HDD) / 70 °C (SSD)
/// as the warning and +10 °C as critical.
#[must_use]
pub fn heat(
    temp: Option<f32>,
    warning: Option<f32>,
    critical: Option<f32>,
    ssd: Option<bool>,
) -> crate::Heat {
    let Some(t) = temp else {
        return crate::Heat::Unknown;
    };
    let warn = warning.unwrap_or(if ssd == Some(false) { 60.0 } else { 70.0 });
    let crit = critical.unwrap_or(warn + 10.0).max(warn);
    if t >= crit {
        crate::Heat::Critical
    } else if t >= warn {
        crate::Heat::Warning
    } else {
        crate::Heat::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Heat;
    use crate::ata::{self, tests as fx};
    use crate::nvme::{self, tests::pm981};

    fn keys(r: &[Reason]) -> Vec<&str> {
        r.iter().map(|r| r.key.as_str()).collect()
    }

    #[test]
    fn healthy_nvme_is_good() {
        let log = nvme::NvmeLog::parse(&pm981()).expect("log");
        assert_eq!(nvme(&log), (Status::Good, Vec::new()));
    }

    #[test]
    fn nvme_rules() {
        let base = nvme::NvmeLog::parse(&pm981()).expect("log");
        let with = |f: &dyn Fn(&mut nvme::NvmeLog)| {
            let mut l = base.clone();
            f(&mut l);
            nvme(&l)
        };
        let (s, r) = with(&|l| l.critical_warning = warning::RELIABILITY);
        assert_eq!((s, keys(&r)), (Status::Bad, vec!["nvme-reliability"]));
        let (s, r) = with(&|l| l.available_spare = 5);
        assert_eq!((s, keys(&r)), (Status::Bad, vec!["nvme-spare-low"]));
        let (s, r) = with(&|l| l.critical_warning = warning::READ_ONLY | warning::BACKUP);
        assert_eq!(
            (s, keys(&r)),
            (Status::Bad, vec!["nvme-read-only", "nvme-backup"])
        );
        let (s, r) = with(&|l| l.critical_warning = warning::TEMPERATURE);
        assert_eq!((s, keys(&r)), (Status::Caution, vec!["nvme-temperature"]));
        let (s, r) = with(&|l| l.percentage_used = 90);
        assert_eq!((s, keys(&r)), (Status::Caution, vec!["worn"]));
        assert_eq!(r[0].value, Some(10.0));
        assert_eq!(with(&|l| l.percentage_used = 89).0, Status::Good);
        let (s, r) = with(&|l| l.media_errors = 3);
        assert_eq!((s, keys(&r)), (Status::Caution, vec!["nvme-media-errors"]));
        // Bad reasons come first
        let (s, r) = with(&|l| {
            l.media_errors = 1;
            l.critical_warning = warning::RELIABILITY;
        });
        assert_eq!(
            (s, keys(&r)),
            (Status::Bad, vec!["nvme-reliability", "nvme-media-errors"])
        );
    }

    fn judge(rows: &[fx::Row], thresholds: bool, ssd: bool) -> (Status, Vec<Reason>) {
        let (data, thr) = fx::sectors(rows);
        let attrs = ata::parse(&data, thresholds.then_some(thr.as_slice()));
        let life = ata::derive(&attrs, ssd).life_left;
        ata(&attrs, Some(false), life)
    }

    /// The three sample tables give what CrystalDiskInfo shows for them.
    #[test]
    fn matches_crystaldiskinfo_on_the_samples() {
        let (s, r) = judge(&fx::hdd_pending(), true, false);
        assert_eq!((s, keys(&r)), (Status::Caution, vec!["ata-pending"]));
        assert_eq!(r[0].value, Some(8.0));
        assert_eq!(judge(&fx::ssd_good(), true, true).0, Status::Good);
        let (s, r) = judge(&fx::hdd_failing(), true, false);
        assert_eq!(s, Status::Bad);
        assert_eq!(
            keys(&r),
            vec![
                "ata-threshold",
                "ata-reallocated",
                "ata-pending",
                "ata-uncorrectable"
            ]
        );
        assert_eq!(r[0].attr, Some(0x05));
        // without thresholds (no admin) the raw counters still give a Caution
        assert_eq!(judge(&fx::hdd_failing(), false, false).0, Status::Caution);
    }

    #[test]
    fn worn_ssd_and_predict_flag() {
        let mut rows = fx::ssd_good();
        rows[3].2 = 8; // B1 Wear Leveling Count at 8 %
        let (s, r) = judge(&rows, true, true);
        assert_eq!((s, keys(&r)), (Status::Caution, vec!["worn"]));
        let (data, _) = fx::sectors(&fx::ssd_good());
        let attrs = ata::parse(&data, None);
        assert_eq!(ata(&attrs, Some(true), None).0, Status::Bad);
        assert_eq!(ata(&[], None, None), (Status::Unknown, Vec::new()));
        assert_eq!(ata(&[], Some(false), None).0, Status::Good);
    }

    #[test]
    fn heat_levels() {
        assert_eq!(heat(None, None, None, None), Heat::Unknown);
        assert_eq!(heat(Some(52.0), Some(80.0), Some(85.0), None), Heat::Normal);
        assert_eq!(
            heat(Some(81.0), Some(80.0), Some(85.0), None),
            Heat::Warning
        );
        assert_eq!(
            heat(Some(85.0), Some(80.0), Some(85.0), None),
            Heat::Critical
        );
        // defaults: 60 °C for a hard disk, 70 °C for an SSD
        assert_eq!(heat(Some(62.0), None, None, Some(false)), Heat::Warning);
        assert_eq!(heat(Some(62.0), None, None, Some(true)), Heat::Normal);
        assert_eq!(heat(Some(80.0), None, None, Some(true)), Heat::Critical);
    }
}
