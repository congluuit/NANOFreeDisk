//! Health Score (PLAN §13 F1): one number from 0 to 100 for the home screen, plus
//! the reasons behind every point lost, each pointing at the view that fixes it.
//!
//! Pure scoring over facts the caller gathers (volume sizes, the free-space forecast
//! from the scan history, the Old & Large total of the current scan), so the rules
//! are unit-tested and the same in the GUI and a future CLI. Phase 2 only has disk
//! space signals; Phase 3 adds junk the rule packs would clean safely
//! ([`JunkFacts`], fixed in the Clean view); SMART (Phase 7) and startup items (E2)
//! add their own [`Finding`]s later.

use serde::Serialize;

/// One fixed drive as the score sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeFacts {
    /// Root such as `C:\`.
    pub root: String,
    pub total: u64,
    pub free: u64,
    /// The Windows drive (weighs more).
    pub system: bool,
    /// From the scan history; `None` when unknown or not filling.
    pub days_to_full: Option<f64>,
    /// The history has enough readings for a forecast of this drive.
    pub has_trend: bool,
}

/// What the current scan adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanFacts {
    pub root: String,
    /// Size of the volume the scan lives on (0 = unknown).
    pub volume_total: u64,
    /// Old & Large total, each file once.
    pub reclaimable: u64,
}

/// What the last rule scan found (🟢 Safe junk only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunkFacts {
    pub bytes: u64,
    /// Size of the system volume (0 = unknown).
    pub volume_total: u64,
}

/// (bytes at least, share of the system volume at least, penalty): the first row
/// either threshold reaches applies.
const JUNK: [(u64, f64, u32); 3] = [(10 << 30, 0.05, 8), (2 << 30, 0.02, 4), (500 << 20, 1.0, 2)];

/// How bad a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Bad,
}

/// One reason in the score. `kind` doubles as the UI message id suffix.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub kind: &'static str,
    pub severity: Severity,
    /// Points taken off.
    pub penalty: u32,
    /// Drive or folder the finding is about.
    pub subject: String,
    /// Percent free, days to full… depending on `kind`.
    pub value: f64,
    pub bytes: u64,
    /// View that fixes it (`analyze`, `oldlarge`, `home`).
    pub fix: &'static str,
}

/// Result of [`score`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthScore {
    pub score: u32,
    /// `good` (≥ 85), `fair` (≥ 60) or `poor`.
    pub grade: &'static str,
    /// Biggest penalty first; informational findings (penalty 0) last.
    pub findings: Vec<Finding>,
}

/// Penalty tables: (free-ratio threshold, system drive, other drive).
const LOW_SPACE: [(f64, u32, u32); 3] = [(0.05, 35, 20), (0.10, 25, 12), (0.20, 10, 5)];
/// (days threshold, system drive, other drive).
const FILLING: [(f64, u32, u32); 2] = [(30.0, 15, 8), (90.0, 7, 4)];
/// (share of the volume, penalty).
const RECLAIMABLE: [(f64, u32); 2] = [(0.10, 10), (0.03, 5)];

fn pick<T: Copy>(table: &[(f64, T, T)], v: f64, system: bool) -> Option<T> {
    table
        .iter()
        .find(|row| v < row.0)
        .map(|row| if system { row.1 } else { row.2 })
}

/// Score the machine from what is known.
#[must_use]
pub fn score(volumes: &[VolumeFacts], scan: Option<&ScanFacts>) -> HealthScore {
    score_with(volumes, scan, None)
}

/// [`score`] plus the junk found by the rule packs.
#[must_use]
pub fn score_with(
    volumes: &[VolumeFacts],
    scan: Option<&ScanFacts>,
    junk: Option<&JunkFacts>,
) -> HealthScore {
    let mut findings = Vec::new();
    if let Some(j) = junk {
        let share = if j.volume_total > 0 {
            j.bytes as f64 / j.volume_total as f64
        } else {
            0.0
        };
        if let Some(&(_, _, penalty)) = JUNK
            .iter()
            .find(|(min, ratio, _)| j.bytes >= *min || share >= *ratio)
        {
            findings.push(Finding {
                kind: "junk",
                severity: Severity::Warn,
                penalty,
                subject: String::new(),
                value: share * 100.0,
                bytes: j.bytes,
                fix: "clean",
            });
        }
    }
    for v in volumes.iter().filter(|v| v.total > 0) {
        let ratio = v.free as f64 / v.total as f64;
        if let Some(penalty) = pick(&LOW_SPACE, ratio, v.system) {
            findings.push(Finding {
                kind: "low-space",
                severity: if ratio < 0.10 {
                    Severity::Bad
                } else {
                    Severity::Warn
                },
                penalty,
                subject: v.root.clone(),
                value: ratio * 100.0,
                bytes: v.free,
                fix: "oldlarge",
            });
        }
        if let Some(days) = v.days_to_full {
            if let Some(penalty) = pick(&FILLING, days, v.system) {
                findings.push(Finding {
                    kind: "filling-fast",
                    severity: if days < 30.0 {
                        Severity::Bad
                    } else {
                        Severity::Warn
                    },
                    penalty,
                    subject: v.root.clone(),
                    value: days,
                    bytes: v.free,
                    fix: "analyze",
                });
            }
        }
        if v.system && !v.has_trend {
            findings.push(Finding {
                kind: "no-trend",
                severity: Severity::Info,
                penalty: 0,
                subject: v.root.clone(),
                value: 0.0,
                bytes: 0,
                fix: "home",
            });
        }
    }
    match scan {
        Some(s) if s.volume_total > 0 => {
            let share = s.reclaimable as f64 / s.volume_total as f64;
            if let Some(&(_, penalty)) = RECLAIMABLE.iter().find(|row| share >= row.0) {
                findings.push(Finding {
                    kind: "old-large",
                    severity: Severity::Warn,
                    penalty,
                    subject: s.root.clone(),
                    value: share * 100.0,
                    bytes: s.reclaimable,
                    fix: "oldlarge",
                });
            }
        }
        Some(_) => {}
        None => findings.push(Finding {
            kind: "no-scan",
            severity: Severity::Info,
            penalty: 0,
            subject: String::new(),
            value: 0.0,
            bytes: 0,
            fix: "home",
        }),
    }
    findings.sort_by(|a, b| {
        b.penalty
            .cmp(&a.penalty)
            .then(b.severity.cmp(&a.severity))
            .then_with(|| a.subject.cmp(&b.subject))
    });
    let lost: u32 = findings.iter().map(|f| f.penalty).sum();
    let score = 100u32.saturating_sub(lost);
    HealthScore {
        score,
        grade: match score {
            85.. => "good",
            60..85 => "fair",
            _ => "poor",
        },
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    fn vol(root: &str, total: u64, free: u64, system: bool) -> VolumeFacts {
        VolumeFacts {
            root: root.into(),
            total,
            free,
            system,
            days_to_full: None,
            has_trend: true,
        }
    }

    fn kinds(h: &HealthScore) -> Vec<(&str, u32)> {
        h.findings.iter().map(|f| (f.kind, f.penalty)).collect()
    }

    #[test]
    fn junk_costs_points_by_size_or_share() {
        let c = [vol(r"C:\", 500 * GB, 200 * GB, true)];
        let junk = |bytes| JunkFacts {
            bytes,
            volume_total: 500 * GB,
        };
        let penalty = |bytes| {
            score_with(&c, None, Some(&junk(bytes)))
                .findings
                .iter()
                .find(|f| f.kind == "junk")
                .map(|f| (f.penalty, f.fix))
        };
        assert_eq!(penalty(100 << 20), None);
        assert_eq!(penalty(600 << 20), Some((2, "clean")));
        assert_eq!(penalty(3 * GB), Some((4, "clean")));
        assert_eq!(penalty(30 * GB), Some((8, "clean")));
        assert_eq!(score_with(&c, None, None), score(&c, None));
    }

    #[test]
    fn healthy_machine_scores_100() {
        let scan = ScanFacts {
            root: r"C:\".into(),
            volume_total: 500 * GB,
            reclaimable: GB,
        };
        let h = score(&[vol(r"C:\", 500 * GB, 200 * GB, true)], Some(&scan));
        assert_eq!((h.score, h.grade), (100, "good"));
        assert!(h.findings.is_empty());
    }

    #[test]
    fn low_space_weighs_more_on_the_system_drive() {
        let h = score(
            &[
                vol(r"C:\", 100 * GB, 4 * GB, true),   // 4 %: -35
                vol(r"D:\", 100 * GB, 8 * GB, false),  // 8 %: -12
                vol(r"E:\", 100 * GB, 15 * GB, false), // 15 %: -5
                vol(r"F:\", 0, 0, false),              // unknown size: ignored
            ],
            Some(&ScanFacts {
                root: r"C:\".into(),
                volume_total: 0,
                reclaimable: 0,
            }),
        );
        assert_eq!(
            kinds(&h),
            [("low-space", 35), ("low-space", 12), ("low-space", 5)]
        );
        assert_eq!(h.score, 48);
        assert_eq!(h.grade, "poor");
        assert_eq!(h.findings[0].severity, Severity::Bad);
        assert_eq!(h.findings[2].severity, Severity::Warn);
        assert!((h.findings[0].value - 4.0).abs() < 1e-9);
        assert_eq!(h.findings[0].fix, "oldlarge");
    }

    #[test]
    fn forecast_reclaimable_and_info_findings() {
        let mut c = vol(r"C:\", 1000 * GB, 400 * GB, true);
        c.days_to_full = Some(20.0);
        let mut d = vol(r"D:\", 1000 * GB, 400 * GB, false);
        d.days_to_full = Some(60.0);
        let scan = ScanFacts {
            root: r"D:\Data".into(),
            volume_total: 1000 * GB,
            reclaimable: 150 * GB,
        };
        let h = score(&[c, d], Some(&scan));
        assert_eq!(
            kinds(&h),
            [("filling-fast", 15), ("old-large", 10), ("filling-fast", 4)]
        );
        assert_eq!(h.score, 71);
        assert_eq!(h.grade, "fair");

        // nothing scanned, no history yet: informational only
        let mut c = vol(r"C:\", 1000 * GB, 400 * GB, true);
        c.has_trend = false;
        let h = score(&[c], None);
        assert_eq!(kinds(&h), [("no-scan", 0), ("no-trend", 0)]);
        assert_eq!(h.score, 100);
    }

    #[test]
    fn score_never_goes_below_zero() {
        let vols: Vec<VolumeFacts> = (0..10)
            .map(|i| {
                let mut v = vol(&format!("{i}:"), 100 * GB, GB, true);
                v.days_to_full = Some(1.0);
                v
            })
            .collect();
        let h = score(&vols, None);
        assert_eq!(h.score, 0);
        assert_eq!(h.grade, "poor");
    }
}
