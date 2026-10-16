//! DevCache Manager (PLAN §4.3, C1): the rule packs marked `devcache = true` grouped
//! **vendor → app → folder** with totals, what is deliberately kept (🔒, the 🔴
//! Protected rules) and per-vendor size thresholds.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use nfd_core::RiskLevel;
use nfd_rules::matcher::{MatchReport, RuleSummary};
use nfd_rules::{Pack, expand};
use serde::Serialize;

const GIB: u64 = 1 << 30;

/// The DevCache packs among `packs`.
#[must_use]
pub fn packs(packs: &[Pack]) -> Vec<Pack> {
    packs.iter().filter(|p| p.pack.devcache).cloned().collect()
}

/// Ids of the rules whose app is on this machine: one of their paths (up to its
/// first glob) exists. 🔒 Keep rules of apps that are not installed are not shown.
#[must_use]
pub fn present_rules(packs: &[Pack], resolve: &impl Fn(&str) -> Option<String>) -> HashSet<String> {
    packs
        .iter()
        .flat_map(|p| &p.rules)
        .filter(|r| {
            r.paths
                .iter()
                .filter_map(|p| expand(p, resolve))
                .any(|p| Path::new(&crate::literal_base(&p)).exists())
        })
        .map(|r| r.id.clone())
        .collect()
}

/// Thresholds used until the user sets their own (vendor → bytes): Ollama models
/// 50 GB, Hugging Face 30 GB, JetBrains 10 GB.
#[must_use]
pub fn default_thresholds() -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("Ollama".to_owned(), 50 * GIB),
        ("Hugging Face".to_owned(), 30 * GIB),
        ("JetBrains".to_owned(), 10 * GIB),
    ])
}

/// One rule with the newest write time of what it found.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevRule {
    #[serde(flatten)]
    pub summary: RuleSummary,
    /// Newest last write inside the rule's candidates (Unix seconds, 0 = none).
    pub newest: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppGroup {
    pub app: String,
    pub bytes: u64,
    pub files: u64,
    pub safe_bytes: u64,
    pub newest: i64,
    /// Apps that must be closed and are running (exe names, deduplicated).
    pub running: Vec<String>,
    /// Rules with something to clean, 🟢 first then by size.
    pub rules: Vec<DevRule>,
    /// 🔒 What the app deliberately keeps (🔴 Protected rules).
    pub kept: Vec<DevRule>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorGroup {
    pub vendor: String,
    pub bytes: u64,
    pub files: u64,
    pub safe_bytes: u64,
    pub caution_bytes: u64,
    pub advanced_bytes: u64,
    pub newest: i64,
    pub running: Vec<String>,
    /// Size the user wants to be reminded about, if any.
    pub threshold: Option<u64>,
    pub over_threshold: bool,
    pub apps: Vec<AppGroup>,
}

/// Everything the DevCache view shows on its Cache tab.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCacheReport {
    /// Largest first.
    pub vendors: Vec<VendorGroup>,
    pub safe: u64,
    pub caution: u64,
    pub advanced: u64,
    /// Number of 🔒 Keep rules.
    pub kept: usize,
}

impl DevCacheReport {
    /// Rule ids of every 🟢 Safe rule with something to clean, optionally of one vendor.
    #[must_use]
    pub fn safe_rule_ids(&self, vendor: Option<&str>) -> Vec<String> {
        self.vendors
            .iter()
            .filter(|v| vendor.is_none_or(|x| v.vendor.eq_ignore_ascii_case(x)))
            .flat_map(|v| &v.apps)
            .flat_map(|a| &a.rules)
            .filter(|r| r.summary.level == Some(RiskLevel::Safe) && r.summary.candidates > 0)
            .map(|r| r.summary.rule_id.clone())
            .collect()
    }

    /// Vendors over their threshold.
    pub fn over_threshold(&self) -> impl Iterator<Item = &VendorGroup> + '_ {
        self.vendors.iter().filter(|v| v.over_threshold)
    }
}

fn push_unique(list: &mut Vec<String>, items: &[String]) {
    for i in items {
        if !list.iter().any(|x| x.eq_ignore_ascii_case(i)) {
            list.push(i.clone());
        }
    }
}

/// Group the rules of `report` that belong to `devcache_packs` (pack ids).
/// `thresholds` maps vendor names (case-insensitive) to bytes; 🔒 Keep rules are listed
/// when their id is in `present` ([`present_rules`]). Apps and vendors with nothing to
/// clean and nothing kept are left out.
#[must_use]
pub fn group(
    report: &MatchReport,
    devcache_packs: &[String],
    thresholds: &BTreeMap<String, u64>,
    present: &HashSet<String>,
) -> DevCacheReport {
    let mut newest: BTreeMap<&str, i64> = BTreeMap::new();
    for c in &report.candidates {
        let n = newest.entry(c.rule_id.as_str()).or_insert(0);
        *n = (*n).max(c.newest_mtime);
    }
    // vendor → app → rules, in first-seen order of the packs
    let mut vendors: Vec<VendorGroup> = Vec::new();
    let mut out = DevCacheReport::default();
    for summary in &report.rules {
        if !devcache_packs.contains(&summary.pack_id) {
            continue;
        }
        let level = summary.level.unwrap_or(RiskLevel::Protected);
        let kept = level == RiskLevel::Protected;
        if (kept && !present.contains(&summary.rule_id)) || (!kept && summary.candidates == 0) {
            continue;
        }
        let rule = DevRule {
            summary: summary.clone(),
            newest: newest.get(summary.rule_id.as_str()).copied().unwrap_or(0),
        };
        let vi = match vendors
            .iter()
            .position(|v| v.vendor.eq_ignore_ascii_case(&summary.vendor))
        {
            Some(i) => i,
            None => {
                vendors.push(VendorGroup {
                    vendor: summary.vendor.clone(),
                    ..VendorGroup::default()
                });
                vendors.len() - 1
            }
        };
        let vendor = &mut vendors[vi];
        let ai = match vendor
            .apps
            .iter()
            .position(|a| a.app.eq_ignore_ascii_case(&summary.app))
        {
            Some(i) => i,
            None => {
                vendor.apps.push(AppGroup {
                    app: summary.app.clone(),
                    ..AppGroup::default()
                });
                vendor.apps.len() - 1
            }
        };
        let app = &mut vendor.apps[ai];
        if kept {
            out.kept += 1;
            app.kept.push(rule);
            continue;
        }
        app.bytes += summary.bytes;
        app.files += summary.files;
        app.newest = app.newest.max(rule.newest);
        push_unique(&mut app.running, &summary.running);
        vendor.bytes += summary.bytes;
        vendor.files += summary.files;
        vendor.newest = vendor.newest.max(rule.newest);
        push_unique(&mut vendor.running, &summary.running);
        match level {
            RiskLevel::Safe => {
                app.safe_bytes += summary.bytes;
                vendor.safe_bytes += summary.bytes;
                out.safe += summary.bytes;
            }
            RiskLevel::Caution => {
                vendor.caution_bytes += summary.bytes;
                out.caution += summary.bytes;
            }
            RiskLevel::Advanced => {
                vendor.advanced_bytes += summary.bytes;
                out.advanced += summary.bytes;
            }
            RiskLevel::Protected => {}
        }
        app.rules.push(rule);
    }
    for v in &mut vendors {
        v.threshold = thresholds
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&v.vendor))
            .map(|(_, b)| *b);
        v.over_threshold = v.threshold.is_some_and(|t| t > 0 && v.bytes >= t);
        for a in &mut v.apps {
            a.rules.sort_by(|x, y| {
                x.summary
                    .level
                    .cmp(&y.summary.level)
                    .then(y.summary.bytes.cmp(&x.summary.bytes))
            });
        }
        v.apps
            .sort_by(|x, y| y.bytes.cmp(&x.bytes).then(x.app.cmp(&y.app)));
    }
    vendors.sort_by(|x, y| y.bytes.cmp(&x.bytes).then(x.vendor.cmp(&y.vendor)));
    out.vendors = vendors;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfd_rules::matcher::Candidate;

    fn summary(id: &str, vendor: &str, app: &str, level: RiskLevel, bytes: u64) -> RuleSummary {
        RuleSummary {
            rule_id: id.into(),
            pack_id: "dev".into(),
            app: app.into(),
            vendor: vendor.into(),
            level: Some(level),
            bytes,
            files: 1,
            candidates: usize::from(bytes > 0),
            running: if id == "a.cache" {
                vec!["a.exe".into()]
            } else {
                Vec::new()
            },
            ..RuleSummary::default()
        }
    }

    fn report() -> MatchReport {
        let mut other = summary("x.temp", "Microsoft", "Windows", RiskLevel::Safe, 999);
        other.pack_id = "windows-system".into();
        MatchReport {
            candidates: vec![Candidate {
                rule_id: "a.cache".into(),
                pack_id: "dev".into(),
                path: "C:/x".into(),
                is_dir: true,
                bytes: 10,
                size: 0,
                files: 1,
                newest_mtime: 1234,
                mtime: 1,
                level: RiskLevel::Safe,
            }],
            rules: vec![
                summary("a.cache", "Acme", "Editor", RiskLevel::Safe, 10),
                summary("a.history", "acme", "Editor", RiskLevel::Caution, 30),
                summary("a.config", "Acme", "Editor", RiskLevel::Protected, 0),
                summary("a.cli", "Acme", "CLI", RiskLevel::Safe, 0),
                summary("b.models", "Big", "Models", RiskLevel::Caution, 5 * GIB),
                other,
            ],
        }
    }

    #[test]
    fn groups_vendor_app_and_keeps() {
        let present = HashSet::from(["a.config".to_owned()]);
        let r = group(&report(), &["dev".into()], &BTreeMap::new(), &present);
        assert_eq!(r.vendors.len(), 2, "windows-system is not a DevCache pack");
        assert_eq!(r.vendors[0].vendor, "Big");
        let acme = &r.vendors[1];
        assert_eq!(acme.bytes, 40);
        assert_eq!(acme.safe_bytes, 10);
        assert_eq!(acme.caution_bytes, 30);
        assert_eq!(acme.running, ["a.exe"]);
        // CLI found nothing and keeps nothing: left out
        assert_eq!(acme.apps.len(), 1);
        let editor = acme
            .apps
            .iter()
            .find(|a| a.app == "Editor")
            .expect("editor");
        assert_eq!(editor.rules[0].summary.rule_id, "a.cache", "safe first");
        assert_eq!(editor.rules[0].newest, 1234);
        assert_eq!(editor.kept.len(), 1);
        assert_eq!(r.kept, 1);
        assert_eq!(r.safe, 10);
        assert_eq!(r.safe_rule_ids(None), ["a.cache"]);
        assert!(r.safe_rule_ids(Some("big")).is_empty());
    }

    #[test]
    fn flags_vendors_over_their_threshold() {
        let mut t = default_thresholds();
        t.insert("big".into(), 4 * GIB);
        let r = group(&report(), &["dev".into()], &t, &HashSet::new());
        let big = &r.vendors[0];
        assert_eq!(big.threshold, Some(4 * GIB));
        assert!(big.over_threshold);
        assert_eq!(r.over_threshold().count(), 1);
    }
}
