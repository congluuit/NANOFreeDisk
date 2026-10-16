//! Alerts (PLAN §4.6, G5): a drive's verdict got worse, a drive is at or above its
//! warning temperature, a volume dropped below 10 % free. Compared against the state
//! of the previous check (kept by the caller between runs), so each event is
//! reported once — not on every check while it lasts.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Drive, HealthReport, Heat, Status};

/// Below this share of free space a volume alerts…
pub const LOW_FREE_PERCENT: f64 = 10.0;
/// …and it alerts again only after climbing back above this.
pub const LOW_FREE_CLEAR: f64 = 12.0;

/// What the previous check saw.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertState {
    /// Drive key → last verdict.
    pub drives: BTreeMap<String, Status>,
    /// Drives currently at or above their warning temperature.
    pub hot: BTreeSet<String>,
    /// Volumes currently below [`LOW_FREE_PERCENT`].
    pub low: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlertKind {
    /// Verdict got worse (or a new drive is already Caution / Bad).
    Status,
    Hot,
    LowSpace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub kind: AlertKind,
    /// Drive model or volume root.
    pub subject: String,
    /// New verdict for [`AlertKind::Status`].
    pub status: Option<Status>,
    /// °C for `Hot`, percent free for `LowSpace`.
    pub value: f64,
    /// Warning temperature for `Hot`.
    pub limit: Option<f64>,
}

/// Stable identity of a drive across checks (serial, else model + index).
#[must_use]
pub fn drive_key(d: &Drive) -> String {
    match d.serial.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => format!("{}|{s}", d.model),
        None => format!("{}|#{}", d.model, d.index),
    }
}

/// Compare `report` with `state`, update `state` and return the new alerts.
/// `temperatures` off leaves temperature to the Hardware Monitor's own alerts.
pub fn check(state: &mut AlertState, report: &HealthReport, temperatures: bool) -> Vec<Alert> {
    let mut out = Vec::new();
    for d in &report.drives {
        let key = drive_key(d);
        let before = state.drives.get(&key).copied();
        // Unknown (nothing readable this time) neither alerts nor forgets the last verdict
        if d.status != Status::Unknown {
            let worse = match before {
                Some(b) => d.status > b && d.status >= Status::Caution,
                None => d.status >= Status::Caution,
            };
            if worse {
                out.push(Alert {
                    kind: AlertKind::Status,
                    subject: d.model.clone(),
                    status: Some(d.status),
                    value: 0.0,
                    limit: None,
                });
            }
            state.drives.insert(key.clone(), d.status);
        }
        match d.heat {
            Heat::Warning | Heat::Critical => {
                if state.hot.insert(key) && temperatures {
                    out.push(Alert {
                        kind: AlertKind::Hot,
                        subject: d.model.clone(),
                        status: None,
                        value: f64::from(d.temperature.unwrap_or(0.0)),
                        limit: d
                            .temp_warning
                            .map(f64::from)
                            .or(Some(if d.ssd == Some(false) { 60.0 } else { 70.0 })),
                    });
                }
            }
            Heat::Normal => {
                state.hot.remove(&key);
            }
            Heat::Unknown => {}
        }
    }
    for v in report.volumes.iter().filter(|v| v.total > 0) {
        let pct = v.free_percent();
        if pct < LOW_FREE_PERCENT {
            if state.low.insert(v.root.clone()) {
                out.push(Alert {
                    kind: AlertKind::LowSpace,
                    subject: v.root.clone(),
                    status: None,
                    value: pct,
                    limit: Some(LOW_FREE_PERCENT),
                });
            }
        } else if pct >= LOW_FREE_CLEAR {
            state.low.remove(&v.root);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RawDrive, SCHEMA, Volume};

    fn report(status: Status, heat: Heat, free: u64) -> HealthReport {
        let mut d = Drive::build(
            RawDrive {
                index: 0,
                model: "Disk".into(),
                serial: Some("S1".into()),
                bus: "NVMe".into(),
                ..RawDrive::default()
            },
            Vec::new(),
        );
        d.status = status;
        d.heat = heat;
        d.temperature = Some(75.0);
        HealthReport {
            schema: SCHEMA.into(),
            generated_at: 0,
            elevated: false,
            drives: vec![d],
            volumes: vec![Volume {
                root: r"C:\".into(),
                label: String::new(),
                fs: "NTFS".into(),
                total: 100,
                free,
                system: true,
                removable: false,
                disk: Some(0),
                offset: None,
                aligned: None,
                cluster: None,
                trim_enabled: None,
                bitlocker: None,
                dirty: None,
                days_to_full: None,
                has_trend: false,
            }],
        }
    }

    fn kinds(a: &[Alert]) -> Vec<AlertKind> {
        a.iter().map(|a| a.kind).collect()
    }

    #[test]
    fn status_alerts_only_when_worse() {
        let mut s = AlertState::default();
        assert!(check(&mut s, &report(Status::Good, Heat::Normal, 50), true).is_empty());
        let a = check(&mut s, &report(Status::Caution, Heat::Normal, 50), true);
        assert_eq!(kinds(&a), vec![AlertKind::Status]);
        assert_eq!(a[0].status, Some(Status::Caution));
        // still Caution: nothing new; Unknown keeps the last verdict
        assert!(check(&mut s, &report(Status::Caution, Heat::Normal, 50), true).is_empty());
        assert!(check(&mut s, &report(Status::Unknown, Heat::Normal, 50), true).is_empty());
        assert_eq!(s.drives.values().next(), Some(&Status::Caution));
        let a = check(&mut s, &report(Status::Bad, Heat::Normal, 50), true);
        assert_eq!(a[0].status, Some(Status::Bad));
        // getting better is not an alert
        assert!(check(&mut s, &report(Status::Good, Heat::Normal, 50), true).is_empty());
        // a drive seen for the first time already in trouble
        let mut fresh = AlertState::default();
        assert_eq!(
            kinds(&check(
                &mut fresh,
                &report(Status::Bad, Heat::Normal, 50),
                true
            )),
            vec![AlertKind::Status]
        );
    }

    #[test]
    fn heat_and_low_space_alert_once() {
        let mut s = AlertState::default();
        let a = check(&mut s, &report(Status::Good, Heat::Warning, 5), true);
        assert_eq!(kinds(&a), vec![AlertKind::Hot, AlertKind::LowSpace]);
        assert_eq!(a[0].limit, Some(70.0));
        assert!((a[1].value - 5.0).abs() < 1e-9);
        assert!(check(&mut s, &report(Status::Good, Heat::Critical, 6), true).is_empty());
        // 11 % is not yet clear of the band; 12 % is
        assert!(check(&mut s, &report(Status::Good, Heat::Normal, 11), true).is_empty());
        assert!(check(&mut s, &report(Status::Good, Heat::Normal, 9), true).is_empty());
        check(&mut s, &report(Status::Good, Heat::Normal, 12), true);
        let a = check(&mut s, &report(Status::Good, Heat::Warning, 9), true);
        assert_eq!(kinds(&a), vec![AlertKind::Hot, AlertKind::LowSpace]);
        // with temperatures off the state still follows, without alerts
        let mut quiet = AlertState::default();
        assert!(
            kinds(&check(
                &mut quiet,
                &report(Status::Good, Heat::Warning, 50),
                false
            ))
            .is_empty()
        );
        assert_eq!(quiet.hot.len(), 1);
    }
}
