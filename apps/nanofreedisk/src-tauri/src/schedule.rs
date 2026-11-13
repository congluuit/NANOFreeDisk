//! Automatic cleaning and low disk space alerts (`docs/SCHEDULE.md`): the pure
//! decisions, with the time passed in. The side effects (scanning, the executor,
//! notifications, the `Run` key) live in [`crate::commands::schedule`].
//!
//! Everything here is opt-in and off by default. What an automatic clean may touch
//! is decided by [`qualifies`] from the rule pack itself, never from the settings
//! file: the settings can only leave categories out, never add a rule, a level or a
//! location.

use std::collections::BTreeMap;

use nfd_core::RiskLevel;
use serde::{Deserialize, Serialize};

/// How often the automatic clean runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Frequency {
    #[default]
    Off,
    Weekly,
    Monthly,
}

impl Frequency {
    /// Shortest time between two automatic cleans, in seconds.
    #[must_use]
    pub const fn interval(self) -> Option<i64> {
        match self {
            Self::Off => None,
            Self::Weekly => Some(7 * DAY),
            Self::Monthly => Some(30 * DAY),
        }
    }
}

pub const DAY: i64 = 86_400;
/// "Idle" = no keyboard or mouse input for this long.
pub const IDLE_SECS: u64 = 5 * 60;
/// Low space alert: at most one notification per drive in this time.
pub const ALERT_GAP: i64 = DAY;

/// The user's choices (the `config` part of `schedule.json`). Unknown fields are ignored;
/// a file that does not parse means everything is off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub frequency: Frequency,
    /// Wait until the PC has had no input for [`IDLE_SECS`].
    pub idle_only: bool,
    /// Rule categories the user left out (`system`, `browser`…). Only narrows.
    pub off_categories: Vec<String>,
    /// Low disk space alert on.
    pub low_space: bool,
    /// Alert below this share of the drive…
    pub low_percent: f64,
    /// …or below this many GB, whichever is smaller in bytes.
    pub low_gb: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frequency: Frequency::Off,
            idle_only: true,
            off_categories: Vec::new(),
            low_space: false,
            low_percent: 10.0,
            low_gb: 10.0,
        }
    }
}

impl Config {
    /// Out-of-range numbers pulled back into range (hand-edited files).
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        let clamp = |v: f64, lo: f64, hi: f64, def: f64| {
            if v.is_finite() { v.clamp(lo, hi) } else { def }
        };
        self.low_percent = clamp(self.low_percent, 1.0, 50.0, 10.0);
        self.low_gb = clamp(self.low_gb, 1.0, 10_000.0, 10.0);
        self.off_categories.retain(|c| !c.is_empty());
        self
    }
}

/// What happened, kept between runs (the `status` part of `schedule.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Status {
    /// When automatic cleaning was turned on (the first run is one period later).
    pub enabled_at: Option<i64>,
    /// Last automatic (or "Run now") clean, Unix seconds.
    pub last_run: Option<i64>,
    pub last_freed: u64,
    pub last_items: usize,
    /// Journal id of the last run (Cleanup history).
    pub last_op: Option<String>,
    /// Low space alert per drive root.
    pub drives: BTreeMap<String, DriveMark>,
}

/// One drive's low space state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DriveMark {
    /// Below the threshold at the last check.
    pub low: bool,
    /// Last notification, Unix seconds.
    pub alerted_at: Option<i64>,
}

/// Is an automatic clean due?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    Off,
    /// Not before this time.
    NotYet(i64),
    Now,
}

impl Status {
    /// Bring the status in step with `config` at `now`: remember when automatic
    /// cleaning was turned on, forget it when it is off, and pull times that lie in
    /// the future (clock moved backwards, hand-edited file) back to `now`, so a
    /// wrong clock can neither start a run early nor block runs for years.
    pub fn sync(&mut self, config: &Config, now: i64) {
        if config.frequency == Frequency::Off {
            self.enabled_at = None;
        } else if self.enabled_at.is_none() {
            self.enabled_at = Some(now);
        }
        for v in [&mut self.enabled_at, &mut self.last_run]
            .into_iter()
            .flatten()
        {
            if *v > now {
                *v = now;
            }
        }
        for mark in self.drives.values_mut() {
            if mark.alerted_at.is_some_and(|t| t > now) {
                mark.alerted_at = Some(now);
            }
        }
    }

    /// When the next automatic clean may run.
    #[must_use]
    pub fn due(&self, config: &Config, now: i64) -> Due {
        let Some(every) = config.frequency.interval() else {
            return Due::Off;
        };
        // the first run is one period after turning it on, later ones one period
        // after the last run; a time in the future counts as "now"
        let base = self.last_run.or(self.enabled_at).unwrap_or(now).min(now);
        let at = base.saturating_add(every);
        if now >= at { Due::Now } else { Due::NotYet(at) }
    }

    /// Note a finished run (also a run that found nothing).
    pub fn ran(&mut self, now: i64, freed: u64, items: usize, op: Option<String>) {
        self.last_run = Some(now);
        self.last_freed = freed;
        self.last_items = items;
        self.last_op = op;
    }
}

/// The facts of a rule that decide whether an automatic clean may use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFacts<'a> {
    pub id: &'a str,
    pub category: &'a str,
    pub level: RiskLevel,
    pub regenerates: bool,
    pub admin: bool,
    /// A guidance rule that points to a Windows tool.
    pub tool: bool,
    pub requires_closed: &'a [String],
    pub paths: &'a [String],
}

impl<'a> RuleFacts<'a> {
    #[must_use]
    pub fn of(rule: &'a nfd_rules::Rule) -> Self {
        Self {
            id: &rule.id,
            category: &rule.category,
            level: rule.level,
            regenerates: rule.regenerates,
            admin: rule.admin,
            tool: rule.tool.is_some(),
            requires_closed: &rule.requires_closed,
            paths: &rule.paths,
        }
    }
}

/// Why a rule stays out of the automatic clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Exclusion {
    /// Not Safe.
    NotSafe,
    /// Safe, but not marked as something its program rebuilds by itself.
    NotRegenerating,
    /// Needs administrator rights (automatic cleans never elevate).
    Admin,
    /// Points to a Windows tool instead of deleting.
    Tool,
    /// A program it needs closed is running.
    ProgramRunning,
    /// The Recycle Bin is never emptied automatically.
    RecycleBin,
    /// The user left this category out.
    CategoryOff,
}

/// May an automatic clean use this rule? `running` = running exe names, `None`
/// when they could not be read (then no rule that needs a program closed qualifies).
///
/// Hard-coded on purpose: only Safe rules that `regenerates`, never `admin`,
/// never a guidance (`tool`) rule, never the Recycle Bin, and not while a program
/// the rule needs closed is running. The settings can only leave categories out.
///
/// # Errors
/// The reason the rule stays out.
pub fn qualifies(
    rule: &RuleFacts<'_>,
    config: &Config,
    running: Option<&[String]>,
) -> Result<(), Exclusion> {
    if rule.level != RiskLevel::Safe {
        return Err(Exclusion::NotSafe);
    }
    if !rule.regenerates {
        return Err(Exclusion::NotRegenerating);
    }
    if rule.admin {
        return Err(Exclusion::Admin);
    }
    if rule.tool {
        return Err(Exclusion::Tool);
    }
    let recycle = |p: &str| {
        let p = p.to_ascii_lowercase();
        p.contains("$recycle.bin") || p.contains("/.trash") || p.contains("recycler")
    };
    if rule.paths.iter().any(|p| recycle(p)) || rule.id.to_ascii_lowercase().contains("recycle") {
        return Err(Exclusion::RecycleBin);
    }
    if rule
        .requires_closed
        .iter()
        .any(|exe| running.is_none_or(|r| nfd_platform::locks::is_running(r, exe)))
    {
        return Err(Exclusion::ProgramRunning);
    }
    if config
        .off_categories
        .iter()
        .any(|c| c.eq_ignore_ascii_case(rule.category))
    {
        return Err(Exclusion::CategoryOff);
    }
    Ok(())
}

/// Rule ids of `packs` an automatic clean may use, in pack order (this platform's
/// rules only).
#[must_use]
pub fn picks(
    packs: &[nfd_rules::Pack],
    config: &Config,
    running: Option<&[String]>,
    platform: &str,
) -> Vec<String> {
    packs
        .iter()
        .flat_map(|p| &p.rules)
        .filter(|r| r.platform.iter().any(|p| p == platform))
        .filter(|r| qualifies(&RuleFacts::of(r), config, running).is_ok())
        .map(|r| r.id.clone())
        .collect()
}

/// Categories the picker offers: those of rules that pass every hard-coded check
/// (whatever the user left out), sorted.
#[must_use]
pub fn categories(packs: &[nfd_rules::Pack], platform: &str) -> Vec<String> {
    let all = Config::default();
    let mut out: Vec<String> = packs
        .iter()
        .flat_map(|p| &p.rules)
        .filter(|r| r.platform.iter().any(|p| p == platform))
        .filter(|r| {
            // a running program only matters for one run, not for the list
            let facts = RuleFacts {
                requires_closed: &[],
                ..RuleFacts::of(r)
            };
            qualifies(&facts, &all, Some(&[])).is_ok()
        })
        .map(|r| r.category.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

const GIB: f64 = 1_073_741_824.0;

/// Free space below which `total` counts as low: `low_percent` of the drive or
/// `low_gb`, whichever is smaller.
#[must_use]
pub fn low_threshold(total: u64, config: &Config) -> u64 {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let pct = (total as f64 * config.low_percent / 100.0) as u64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let gb = (config.low_gb * GIB) as u64;
    pct.min(gb)
}

/// A drive that just ran low.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LowSpace {
    pub root: String,
    pub free: u64,
    pub total: u64,
    pub threshold: u64,
}

/// Compare the fixed drives `(root, free, total)` with the last check and return
/// the drives to notify about: a drive alerts when it falls below its threshold,
/// at most once per [`ALERT_GAP`], and again only after it climbed back above the
/// threshold plus a margin (a fifth of it, at least 1 GiB).
pub fn low_space(
    status: &mut Status,
    drives: &[(String, u64, u64)],
    config: &Config,
    now: i64,
) -> Vec<LowSpace> {
    let mut out = Vec::new();
    if !config.low_space {
        return out;
    }
    for (root, free, total) in drives.iter().filter(|d| d.2 > 0) {
        let threshold = low_threshold(*total, config);
        let mark = status.drives.entry(root.clone()).or_default();
        if *free < threshold {
            let quiet = mark
                .alerted_at
                .is_some_and(|t| now.saturating_sub(t) < ALERT_GAP);
            if !mark.low && !quiet {
                mark.alerted_at = Some(now);
                out.push(LowSpace {
                    root: root.clone(),
                    free: *free,
                    total: *total,
                    threshold,
                });
            }
            mark.low = true;
        } else {
            let margin = (threshold / 5).max(1 << 30);
            if *free >= threshold.saturating_add(margin) {
                mark.low = false;
            }
        }
    }
    out
}

/// The `Run` value: the exe, quoted, started in the tray.
#[must_use]
#[cfg_attr(not(windows), allow(dead_code))]
pub fn autostart_command(exe: &str) -> String {
    format!("\"{exe}\" {}", crate::TRAY_FLAG)
}

/// The macOS LaunchAgent that starts the app in the tray at login.
#[must_use]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn launch_agent(label: &str, exe: &str) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n\
  <key>Label</key>\n  <string>{}</string>\n\
  <key>ProgramArguments</key>\n  <array>\n    <string>{}</string>\n    <string>{}</string>\n  </array>\n\
  <key>RunAtLoad</key>\n  <true/>\n\
</dict>\n</plist>\n",
        esc(label),
        esc(exe),
        crate::TRAY_FLAG
    )
}

#[cfg(test)]
mod tests;
