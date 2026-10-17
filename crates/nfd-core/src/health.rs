//! Health Score (PLAN §13 F1): one number from 0 to 100 for the home screen, plus
//! the reasons behind every point lost, each pointing at the view that fixes it.
//!
//! Pure scoring over facts the caller gathers (volume sizes, the free-space forecast
//! from the scan history, the Old & Large total of the current scan), so the rules
//! are unit-tested and the same in the GUI and a future CLI. Phase 2 only has disk
//! space signals; Phase 3 adds junk the rule packs would clean safely
//! ([`JunkFacts`], fixed in the Clean view); Phase 7 adds drive health
//! ([`DriveFacts`], [`add_drive_findings`]: SMART / NVMe verdict, temperature against
//! the drive's own limits, volumes marked dirty); startup items (E2) come later.

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
    let mut out = HealthScore {
        score: 100,
        grade: "good",
        findings,
    };
    regrade(&mut out);
    out
}

/// Drive verdict as [`DriveFacts`] carries it (mirrors `nfd_health::Status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveStatus {
    Unknown,
    Good,
    Caution,
    Bad,
}

/// Drive temperature against its own limits (mirrors `nfd_health::Heat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveHeat {
    Unknown,
    Normal,
    Warning,
    Critical,
}

/// One physical drive as the score sees it (Phase 7).
#[derive(Debug, Clone, PartialEq)]
pub struct DriveFacts {
    /// Model name (the subject of the finding).
    pub name: String,
    pub status: DriveStatus,
    pub heat: DriveHeat,
    /// °C, for the message.
    pub temperature: Option<f64>,
    /// SSD life left in percent, for the message.
    pub life_left: Option<u8>,
}

/// (status, penalty).
const DRIVE_STATUS: [(DriveStatus, u32); 2] = [(DriveStatus::Bad, 45), (DriveStatus::Caution, 15)];
/// (heat, penalty).
const DRIVE_HEAT: [(DriveHeat, u32); 2] = [(DriveHeat::Critical, 10), (DriveHeat::Warning, 4)];
/// Penalty for a volume Windows marked dirty (needs chkdsk).
const DIRTY: u32 = 10;

/// Add the drive-health findings: `drive-bad` / `drive-caution` per drive,
/// `drive-hot` / `drive-warm` unless the Hardware Monitor already reported the same
/// drive (`hot-disk` / `warm-disk`), `drive-unknown` (information) when nothing could
/// be read, and `volume-dirty` per dirty volume root. Then [`regrade`].
pub fn add_drive_findings(score: &mut HealthScore, drives: &[DriveFacts], dirty: &[String]) {
    for d in drives {
        if let Some(&(_, penalty)) = DRIVE_STATUS.iter().find(|(s, _)| *s == d.status) {
            let bad = d.status == DriveStatus::Bad;
            score.findings.push(Finding {
                kind: if bad { "drive-bad" } else { "drive-caution" },
                severity: if bad { Severity::Bad } else { Severity::Warn },
                penalty,
                subject: d.name.clone(),
                value: d.life_left.map_or(0.0, f64::from),
                bytes: 0,
                fix: "health",
            });
        } else if d.status == DriveStatus::Unknown {
            score.findings.push(Finding {
                kind: "drive-unknown",
                severity: Severity::Info,
                penalty: 0,
                subject: d.name.clone(),
                value: 0.0,
                bytes: 0,
                fix: "health",
            });
        }
        let already = score
            .findings
            .iter()
            .any(|f| (f.kind == "hot-disk" || f.kind == "warm-disk") && f.subject == d.name);
        if let Some(&(heat, penalty)) = DRIVE_HEAT.iter().find(|(h, _)| *h == d.heat) {
            if !already {
                let hot = heat == DriveHeat::Critical;
                score.findings.push(Finding {
                    kind: if hot { "drive-hot" } else { "drive-warm" },
                    severity: if hot { Severity::Bad } else { Severity::Warn },
                    penalty,
                    subject: d.name.clone(),
                    value: d.temperature.unwrap_or(0.0),
                    bytes: 0,
                    fix: "health",
                });
            }
        }
    }
    for root in dirty {
        score.findings.push(Finding {
            kind: "volume-dirty",
            severity: Severity::Bad,
            penalty: DIRTY,
            subject: root.clone(),
            value: 0.0,
            bytes: 0,
            fix: "health",
        });
    }
    regrade(score);
}

/// (bytes at least, penalty) for the leftovers of finished AI sessions.
const AI_LEFTOVERS: [(u64, u32); 3] = [(10 << 30, 6), (2 << 30, 3), (500 << 20, 1)];

/// Add what AI coding agents left behind in finished sessions (`bytes` the AI
/// leftovers scan pre-selected), fixed in the DevCache view's AI leftovers tab.
pub fn add_ai_leftovers(score: &mut HealthScore, bytes: u64) {
    if let Some(&(_, penalty)) = AI_LEFTOVERS.iter().find(|(min, _)| bytes >= *min) {
        score.findings.push(Finding {
            kind: "ai-leftovers",
            severity: Severity::Warn,
            penalty,
            subject: String::new(),
            value: 0.0,
            bytes,
            fix: "devcache",
        });
        regrade(score);
    }
}

/// Use the advice of the current scan (`nfd-advice`: what unblocked actions can
/// free, 🟢 + 🟡) instead of its Old & Large total: the `old-large` finding becomes
/// `reclaimable`, fixed in the Analyze view's suggestions (`fix` = `suggestions`).
/// Same thresholds.
pub fn add_advice(score: &mut HealthScore, root: &str, volume_total: u64, reclaim: u64) {
    score.findings.retain(|f| f.kind != "old-large");
    if volume_total > 0 {
        let share = reclaim as f64 / volume_total as f64;
        if let Some(&(_, penalty)) = RECLAIMABLE.iter().find(|row| share >= row.0) {
            score.findings.push(Finding {
                kind: "reclaimable",
                severity: Severity::Warn,
                penalty,
                subject: root.to_owned(),
                value: share * 100.0,
                bytes: reclaim,
                fix: "suggestions",
            });
        }
    }
    regrade(score);
}

/// Sort the findings and recompute score and grade after findings were added
/// outside [`score_with`] (same bands: >= 85 good, >= 60 fair).
pub fn regrade(score: &mut HealthScore) {
    score.findings.sort_by(|a, b| {
        b.penalty
            .cmp(&a.penalty)
            .then(b.severity.cmp(&a.severity))
            .then_with(|| a.subject.cmp(&b.subject))
    });
    let lost: u32 = score.findings.iter().map(|f| f.penalty).sum();
    score.score = 100u32.saturating_sub(lost);
    score.grade = match score.score {
        85.. => "good",
        60..85 => "fair",
        _ => "poor",
    };
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
    fn advice_replaces_old_and_large() {
        let c = [vol(r"C:\", 500 * GB, 200 * GB, true)];
        let scan = ScanFacts {
            root: r"D:\".into(),
            volume_total: 1000 * GB,
            reclaimable: 200 * GB,
        };
        let mut h = score(&c, Some(&scan));
        assert_eq!(kinds(&h), [("old-large", 10)]);
        add_advice(&mut h, r"D:\", 1000 * GB, 50 * GB);
        assert_eq!(kinds(&h), [("reclaimable", 5)]);
        assert_eq!(h.findings[0].fix, "suggestions");
        assert_eq!(h.score, 95);
        add_advice(&mut h, r"D:\", 1000 * GB, GB);
        assert!(h.findings.iter().all(|f| f.kind != "old-large"));
        assert_eq!(
            h.score, 95,
            "a second call does not add the old finding back"
        );
    }

    #[test]
    fn ai_leftovers_cost_points_by_size() {
        let c = [vol(r"C:\", 500 * GB, 200 * GB, true)];
        let mut h = score(&c, None);
        let before = h.score;
        add_ai_leftovers(&mut h, 100 << 20);
        assert_eq!(h.score, before, "small leftovers are free");
        add_ai_leftovers(&mut h, 12 * GB);
        assert_eq!(h.score, before - 6);
        let f = h
            .findings
            .iter()
            .find(|f| f.kind == "ai-leftovers")
            .expect("finding");
        assert_eq!(f.fix, "devcache");
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

    fn drive(status: DriveStatus, heat: DriveHeat) -> DriveFacts {
        DriveFacts {
            name: "Samsung SSD 980 PRO 1TB".into(),
            status,
            heat,
            temperature: Some(84.0),
            life_left: Some(7),
        }
    }

    #[test]
    fn drive_health_costs_points() {
        let c = [vol(r"C:\", 500 * GB, 200 * GB, true)];
        let mut h = score(&c, None);
        let base = h.score;
        add_drive_findings(
            &mut h,
            &[
                drive(DriveStatus::Caution, DriveHeat::Warning),
                DriveFacts {
                    name: "USB disk".into(),
                    ..drive(DriveStatus::Unknown, DriveHeat::Unknown)
                },
            ],
            &[r"D:\".into()],
        );
        assert_eq!(
            kinds(&h)[..3],
            [
                ("drive-caution", 15),
                ("volume-dirty", 10),
                ("drive-warm", 4)
            ]
        );
        assert!(kinds(&h).contains(&("drive-unknown", 0)));
        assert_eq!(h.score, base - 29);
        assert_eq!(h.findings[0].fix, "health");
        assert_eq!(h.findings[0].value, 7.0);

        let mut bad = score(&c, None);
        add_drive_findings(
            &mut bad,
            &[drive(DriveStatus::Bad, DriveHeat::Critical)],
            &[],
        );
        assert_eq!(kinds(&bad)[..2], [("drive-bad", 45), ("drive-hot", 10)]);
        // a failing drive alone makes the machine "poor"
        assert_eq!(bad.grade, "poor");
        let mut good = score(&c, None);
        let before = good.clone();
        add_drive_findings(
            &mut good,
            &[drive(DriveStatus::Good, DriveHeat::Normal)],
            &[],
        );
        assert_eq!(good, before);
    }

    #[test]
    fn heat_is_not_counted_twice() {
        let c = [vol(r"C:\", 500 * GB, 200 * GB, true)];
        let mut h = score(&c, None);
        h.findings.push(Finding {
            kind: "hot-disk",
            severity: Severity::Bad,
            penalty: 8,
            subject: "Samsung SSD 980 PRO 1TB".into(),
            value: 84.0,
            bytes: 0,
            fix: "hardware",
        });
        add_drive_findings(
            &mut h,
            &[drive(DriveStatus::Good, DriveHeat::Critical)],
            &[],
        );
        assert!(!kinds(&h).iter().any(|k| k.0 == "drive-hot"));
        assert_eq!(h.score, 92);
    }
}
