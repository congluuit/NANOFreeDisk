//! Matching rule packs against the real filesystem (PHASE3_CLEAN §2.2).
//!
//! A rule path such as `${LOCALAPPDATA}/Google/Chrome/User Data/*/Cache/**` is expanded
//! with this machine's folders and then matched **one component at a time** from its
//! literal base: `*`, `?`, `[a-z]` and `{a,b}` match inside one name (case-insensitive
//! on Windows), a trailing `**` means "everything inside" (each child becomes a
//! candidate, the folder itself stays) and a `**` in the middle descends any number
//! of folders. The walk never follows links or crosses into another volume, and stops
//! at [`MatchOptions::max_depth`].
//!
//! A matching folder becomes **one** candidate with its total size, unless something
//! inside it must stay — a link, an `exclude` match, a file newer than `min_age_days`,
//! or an unreadable sub-folder. Then it is split and only what may go is proposed.
//! Every candidate is checked with the lexical [`SafetyGate`] right away; refused ones
//! are counted, never proposed.
//!
//! The paths of 🔴 Protected rules ("🔒 Keep": config, credentials, extensions,
//! memories…) are an `exclude` of **every** rule, so a broad cache rule can never
//! sweep up something another rule promises to keep.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobBuilder, GlobMatcher};
use nfd_core::RiskLevel;
use nfd_core::plan::{DeleteMode, PlanItem};
use nfd_core::safety::{Origin, SafetyGate};
use nfd_platform::fsinfo::{self, FileInfo, LinkKind};
use rayon::prelude::*;
use serde::Serialize;

use crate::{Pack, Rule, expand};

/// A path a rule proposes to delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub rule_id: String,
    pub pack_id: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Bytes on disk.
    pub bytes: u64,
    /// Logical size of a file (0 for folders); compared again before deleting.
    pub size: u64,
    pub files: u64,
    /// Newest last-write time inside (Unix seconds).
    pub newest_mtime: i64,
    /// Last write of the entry itself.
    pub mtime: i64,
    pub level: RiskLevel,
}

impl Candidate {
    /// The candidate as a planner input.
    #[must_use]
    pub fn to_plan_item(&self, requires_closed: &[String]) -> PlanItem {
        PlanItem {
            path: self.path.clone(),
            origin: Origin::Rule { level: self.level },
            rule_id: Some(self.rule_id.clone()),
            is_dir: self.is_dir,
            bytes: self.bytes,
            files: self.files,
            size: self.size,
            mtime: self.mtime,
            requires_closed: requires_closed.to_vec(),
            mode: DeleteMode::Quarantine,
            selected: false,
            keeper: None,
        }
    }
}

/// Matching options.
#[derive(Debug, Clone, Copy)]
pub struct MatchOptions {
    /// "Now" for `min_age_days` (Unix seconds).
    pub now: i64,
    /// Deepest folder level below a rule's literal base that is visited.
    pub max_depth: usize,
    /// Most candidates one rule may propose (the rest are summarized as truncated).
    pub max_candidates: usize,
    /// Also report rules marked 🔴 Protected (as guidance only — they are never
    /// walked, so they carry no size).
    pub include_protected: bool,
}

impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            now: now(),
            max_depth: 32,
            max_candidates: 5_000,
            include_protected: true,
        }
    }
}

/// Current time in Unix seconds.
#[must_use]
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Per-rule totals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSummary {
    pub rule_id: String,
    pub pack_id: String,
    pub app: String,
    pub vendor: String,
    pub category: String,
    pub level: Option<RiskLevel>,
    pub impact: String,
    pub impact_vi: Option<String>,
    pub requires_closed: Vec<String>,
    /// Apps of `requires_closed` that are running right now.
    pub running: Vec<String>,
    pub bytes: u64,
    pub files: u64,
    pub candidates: usize,
    /// Items skipped because they are too new (`min_age_days`), excluded, links,
    /// unreadable, or refused by the SafetyGate.
    pub skipped: u64,
    pub refused: u64,
    pub truncated: bool,
}

/// Everything the matcher found.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchReport {
    pub candidates: Vec<Candidate>,
    pub rules: Vec<RuleSummary>,
}

impl MatchReport {
    /// Total bytes per pack id.
    #[must_use]
    pub fn bytes_by_pack(&self) -> BTreeMap<String, u64> {
        let mut out = BTreeMap::new();
        for r in &self.rules {
            *out.entry(r.pack_id.clone()).or_insert(0) += r.bytes;
        }
        out
    }

    /// Total bytes per level.
    #[must_use]
    pub fn bytes_by_level(&self) -> BTreeMap<RiskLevel, u64> {
        let mut out = BTreeMap::new();
        for c in &self.candidates {
            *out.entry(c.level).or_insert(0) += c.bytes;
        }
        out
    }

    /// The `requires_closed` list of a rule.
    #[must_use]
    pub fn requires_closed(&self, rule_id: &str) -> &[String] {
        self.rules
            .iter()
            .find(|r| r.rule_id == rule_id)
            .map_or(&[], |r| r.requires_closed.as_slice())
    }

    /// All candidates as planner inputs.
    #[must_use]
    pub fn plan_items(&self) -> Vec<PlanItem> {
        self.candidates
            .iter()
            .map(|c| c.to_plan_item(self.requires_closed(&c.rule_id)))
            .collect()
    }
}

/// One compiled rule path: literal base + remaining component patterns.
#[derive(Debug, Clone)]
struct CompiledPath {
    base: PathBuf,
    rest: Vec<Part>,
}

#[derive(Debug, Clone)]
enum Part {
    /// `**`
    Any,
    /// One name.
    Name(GlobMatcher),
}

#[derive(Debug, Clone)]
struct CompiledRule {
    rule: Rule,
    pack: String,
    paths: Vec<CompiledPath>,
    exclude: Vec<GlobMatcher>,
}

/// Rule packs compiled for this machine.
#[derive(Debug, Clone)]
pub struct Matcher {
    rules: Vec<CompiledRule>,
    /// Paths of 🔴 Protected rules, excluded from every rule.
    keep: Vec<GlobMatcher>,
    gate: SafetyGate,
}

fn is_glob(component: &str) -> bool {
    component.contains(['*', '?', '[', '{'])
}

fn glob(pattern: &str, separator: bool) -> Option<GlobMatcher> {
    GlobBuilder::new(pattern)
        .case_insensitive(cfg!(windows))
        .literal_separator(separator)
        .backslash_escape(false)
        .build()
        .ok()
        .map(|g: Glob| g.compile_matcher())
}

/// `/`-separated text of a path for glob matching.
fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn compile_path(expanded: &str) -> Option<CompiledPath> {
    let text = expanded.replace('\\', "/");
    let comps: Vec<&str> = text.split('/').collect();
    let first_glob = comps.iter().position(|c| is_glob(c)).unwrap_or(comps.len());
    let base_text = comps[..first_glob].join("/");
    let base_text = if cfg!(windows) {
        base_text.replace('/', "\\")
    } else {
        base_text
    };
    let base = if base_text.is_empty() {
        PathBuf::from("/")
    } else if base_text.ends_with(':') {
        PathBuf::from(format!("{base_text}{}", std::path::MAIN_SEPARATOR))
    } else {
        PathBuf::from(base_text)
    };
    if !base.is_absolute() {
        return None;
    }
    let rest = comps[first_glob..]
        .iter()
        .filter(|c| !c.is_empty())
        .map(|c| {
            if *c == "**" {
                Some(Part::Any)
            } else {
                glob(c, true).map(Part::Name)
            }
        })
        .collect::<Option<Vec<_>>>()?;
    Some(CompiledPath { base, rest })
}

impl Matcher {
    /// Compile the rules of `packs` for this machine (`platform` = `"windows"`,
    /// `"linux"`, `"macos"`); rules for other platforms and paths whose variables do
    /// not resolve are left out.
    #[must_use]
    pub fn new(packs: &[Pack], resolve: &impl Fn(&str) -> Option<String>, platform: &str) -> Self {
        let mut rules = Vec::new();
        let mut keep = Vec::new();
        for pack in packs {
            for rule in &pack.rules {
                if !rule.platform.iter().any(|p| p == platform) {
                    continue;
                }
                let paths = rule
                    .paths
                    .iter()
                    .filter_map(|p| expand(p, resolve))
                    .filter_map(|p| compile_path(&p))
                    .collect();
                if rule.level == RiskLevel::Protected {
                    keep.extend(
                        rule.paths
                            .iter()
                            .filter_map(|p| expand(p, resolve))
                            .filter_map(|p| glob(&p.replace('\\', "/"), true)),
                    );
                }
                let exclude = rule
                    .exclude
                    .iter()
                    .filter_map(|p| expand(p, resolve))
                    .filter_map(|p| glob(&p.replace('\\', "/"), true))
                    .collect();
                rules.push(CompiledRule {
                    rule: rule.clone(),
                    pack: pack.pack.id.clone(),
                    paths,
                    exclude,
                });
            }
        }
        Self {
            rules,
            keep,
            gate: SafetyGate::new(resolve),
        }
    }

    /// Rule ids, in pack order.
    pub fn rule_ids(&self) -> impl Iterator<Item = &str> + '_ {
        self.rules.iter().map(|r| r.rule.id.as_str())
    }

    /// Walk every rule whose id passes `keep` and collect candidates. `running` is
    /// the list of running exe names (`nfd_platform::locks::running_processes`).
    #[must_use]
    pub fn scan(
        &self,
        keep: &(dyn Fn(&Rule, &str) -> bool + Sync),
        opts: &MatchOptions,
        running: &[String],
    ) -> MatchReport {
        // rules are independent: walk them in parallel, report them in pack order
        let results: Vec<Option<(RuleSummary, Vec<Candidate>)>> = self
            .rules
            .par_iter()
            .map(|compiled| {
                keep(&compiled.rule, &compiled.pack)
                    .then(|| self.scan_rule(compiled, opts, running))
                    .flatten()
            })
            .collect();
        let mut report = MatchReport::default();
        for (summary, candidates) in results.into_iter().flatten() {
            report.candidates.extend(candidates);
            report.rules.push(summary);
        }
        report
    }

    /// One rule: its summary and candidates (`None` = a Protected rule left out).
    fn scan_rule(
        &self,
        compiled: &CompiledRule,
        opts: &MatchOptions,
        running: &[String],
    ) -> Option<(RuleSummary, Vec<Candidate>)> {
        let rule = &compiled.rule;
        let mut summary = RuleSummary {
            rule_id: rule.id.clone(),
            pack_id: compiled.pack.clone(),
            app: rule.app.clone(),
            vendor: rule.vendor.clone(),
            category: rule.category.clone(),
            level: Some(rule.level),
            impact: rule.impact.clone(),
            impact_vi: rule.impact_vi.clone(),
            requires_closed: rule.requires_closed.clone(),
            running: rule
                .requires_closed
                .iter()
                .filter(|exe| nfd_platform::locks::is_running(running, exe))
                .cloned()
                .collect(),
            ..RuleSummary::default()
        };
        if rule.level == RiskLevel::Protected {
            return opts.include_protected.then_some((summary, Vec::new()));
        }
        let mut walk = Walk {
            rule: compiled,
            keep: &self.keep,
            opts,
            gate: &self.gate,
            cutoff: opts.now - i64::from(rule.min_age_days) * 86_400,
            out: Vec::new(),
            summary: &mut summary,
        };
        for path in &compiled.paths {
            walk.start(path);
        }
        let mut found = walk.out;
        // Two paths of one rule may overlap; keep the outermost.
        found.sort_by_key(|a| slash(&a.path));
        let mut kept: Vec<Candidate> = Vec::with_capacity(found.len());
        for c in found {
            let key = slash(&c.path).to_lowercase();
            let nested = kept.iter().rev().any(|k| {
                let outer = slash(&k.path).to_lowercase();
                key == outer || key.starts_with(&format!("{outer}/"))
            });
            if !nested {
                kept.push(c);
            }
        }
        if rule.keep_newest {
            kept.retain(|c| older_revision(&c.path));
        }
        summary.candidates = kept.len();
        summary.bytes = kept.iter().map(|c| c.bytes).sum();
        summary.files = kept.iter().map(|c| c.files).sum();
        Some((summary, kept))
    }

    /// Rules (with their pack id) whose paths match `path` as a whole — for
    /// `nfd plan explain`.
    #[must_use]
    pub fn rules_matching(&self, path: &Path) -> Vec<(&Rule, &str)> {
        let text = slash(path);
        self.rules
            .iter()
            .filter(|r| {
                r.paths.iter().any(|p| full_match(p, &text))
                    && !r.exclude.iter().any(|e| e.is_match(&text))
            })
            .map(|r| (&r.rule, r.pack.as_str()))
            .collect()
    }

    /// The literal base folder of every rule path (Protected rules included), with
    /// the index of its rule: nothing outside these folders can match that rule. Lets
    /// a caller that walks a whole scan (`nfd-advice`) ask
    /// [`Self::rules_matching_among`] only the rules that may answer.
    #[must_use]
    pub fn bases(&self) -> Vec<(PathBuf, usize)> {
        let mut out: Vec<(PathBuf, usize)> = self
            .rules
            .iter()
            .enumerate()
            .flat_map(|(i, r)| r.paths.iter().map(move |p| (p.base.clone(), i)))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The rule at `index` (as [`Self::bases`] numbers them).
    #[must_use]
    pub fn rule(&self, index: usize) -> Option<&Rule> {
        self.rules.get(index).map(|r| &r.rule)
    }

    /// [`Self::rules_matching`] restricted to the rules at `indices`.
    #[must_use]
    pub fn rules_matching_among(&self, path: &Path, indices: &[usize]) -> Vec<(&Rule, &str)> {
        let text = slash(path);
        indices
            .iter()
            .filter_map(|&i| self.rules.get(i))
            .filter(|r| {
                r.paths.iter().any(|p| full_match(p, &text))
                    && !r.exclude.iter().any(|e| e.is_match(&text))
            })
            .map(|r| (&r.rule, r.pack.as_str()))
            .collect()
    }

    /// The SafetyGate the matcher checks candidates with.
    #[must_use]
    pub const fn gate(&self) -> &SafetyGate {
        &self.gate
    }
}

/// `true` when `path` is a revision folder ([`crate::revision_of`]) and a folder of
/// the same name with a higher revision sits next to it on disk (links ignored).
fn older_revision(path: &Path) -> bool {
    let Some((name, rev)) = path
        .file_name()
        .and_then(|n| crate::revision_of(&n.to_string_lossy()))
    else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    fsinfo::read_dir(parent, false).is_ok_and(|items| {
        items.iter().any(|i| {
            i.info
                .as_ref()
                .is_ok_and(|f| f.is_dir && f.link == LinkKind::None)
                && crate::revision_of(&i.name.to_string_lossy())
                    .is_some_and(|(n, r)| n == name && r > rev)
        })
    })
}

/// Whether `text` (a `/`-separated path) is matched by `p` as a whole: a candidate it
/// would propose, or anything inside one.
fn full_match(p: &CompiledPath, text: &str) -> bool {
    let base = slash(&p.base);
    let base = base.trim_end_matches('/');
    let same = |a: &str, b: &str| {
        if cfg!(windows) {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    if text.len() < base.len() || !same(&text[..base.len()], base) {
        return false;
    }
    let tail = &text[base.len()..];
    if !tail.is_empty() && !tail.starts_with('/') {
        return false;
    }
    let names: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();
    match_parts(&p.rest, &names)
}

fn match_parts(parts: &[Part], names: &[&str]) -> bool {
    match parts.split_first() {
        // the pattern is used up: the candidate itself or something inside it
        None => true,
        Some((Part::Any, rest)) => {
            if rest.is_empty() {
                return !names.is_empty();
            }
            (0..names.len()).any(|skip| match_parts(rest, &names[skip..]))
        }
        Some((Part::Name(g), rest)) => match names.split_first() {
            Some((first, more)) => g.is_match(first) && match_parts(rest, more),
            None => false,
        },
    }
}

struct Walk<'a> {
    rule: &'a CompiledRule,
    keep: &'a [GlobMatcher],
    opts: &'a MatchOptions,
    gate: &'a SafetyGate,
    cutoff: i64,
    out: Vec<Candidate>,
    summary: &'a mut RuleSummary,
}

/// Size of a folder that may go as a whole, or `None` when it must be split.
struct Agg {
    bytes: u64,
    files: u64,
    newest: i64,
}

impl Walk<'_> {
    fn start(&mut self, path: &CompiledPath) {
        let Ok(info) = fsinfo::file_info(&path.base) else {
            return;
        };
        if path.rest.is_empty() {
            self.offer(&path.base, &info, 0);
            return;
        }
        if !info.is_dir || info.link != LinkKind::None {
            return;
        }
        self.step(&path.base, &path.rest, 0);
    }

    fn full(&mut self) -> bool {
        if self.out.len() >= self.opts.max_candidates {
            self.summary.truncated = true;
            return true;
        }
        false
    }

    fn children(&mut self, dir: &Path) -> Vec<(PathBuf, FileInfo)> {
        match fsinfo::read_dir(dir, false) {
            Ok(items) => items
                .into_iter()
                .filter_map(|i| match i.info {
                    Ok(info) => Some((dir.join(&i.name), info)),
                    Err(_) => {
                        self.summary.skipped += 1;
                        None
                    }
                })
                .collect(),
            Err(_) => {
                self.summary.skipped += 1;
                Vec::new()
            }
        }
    }

    /// Match `parts` against the children of `dir`.
    fn step(&mut self, dir: &Path, parts: &[Part], depth: usize) {
        if depth > self.opts.max_depth || self.full() {
            return;
        }
        let Some((part, rest)) = parts.split_first() else {
            return;
        };
        let children = self.children(dir);
        match part {
            Part::Any if rest.is_empty() => {
                for (path, info) in children {
                    self.offer(&path, &info, depth + 1);
                }
            }
            Part::Any => {
                // zero folders…
                self.step_children(&children, rest, depth);
                // …or one more, recursively
                for (path, info) in &children {
                    if info.is_dir && info.link == LinkKind::None {
                        self.step(path, parts, depth + 1);
                    }
                }
            }
            Part::Name(_) => self.step_children(&children, parts, depth),
        }
    }

    fn step_children(&mut self, children: &[(PathBuf, FileInfo)], parts: &[Part], depth: usize) {
        let Some((Part::Name(g), rest)) = parts.split_first() else {
            if let Some((Part::Any, _)) = parts.split_first() {
                // `**/**` collapses
                for (path, info) in children {
                    if info.is_dir && info.link == LinkKind::None {
                        self.step(path, parts, depth + 1);
                    }
                }
            }
            return;
        };
        for (path, info) in children {
            let Some(name) = path.file_name() else {
                continue;
            };
            if !g.is_match(name) {
                continue;
            }
            if rest.is_empty() {
                self.offer(path, info, depth + 1);
            } else if info.is_dir && info.link == LinkKind::None {
                self.step(path, rest, depth + 1);
            }
        }
    }

    fn excluded(&self, path: &Path) -> bool {
        let text = slash(path);
        self.rule
            .exclude
            .iter()
            .chain(self.keep)
            .any(|e| e.is_match(&text))
    }

    /// Propose `path` (a match): as a whole if possible, else split.
    fn offer(&mut self, path: &Path, info: &FileInfo, depth: usize) {
        if self.full() {
            return;
        }
        if info.link != LinkKind::None || self.excluded(path) {
            self.summary.skipped += 1;
            return;
        }
        if !info.is_dir {
            if info.mtime > self.cutoff {
                self.summary.skipped += 1;
                return;
            }
            self.push(path, info, info.on_disk, 1, info.mtime);
            return;
        }
        match self.aggregate(path, depth) {
            Some(agg) if agg.files > 0 || info.mtime <= self.cutoff => {
                self.push(path, info, agg.bytes, agg.files, agg.newest.max(info.mtime));
            }
            Some(_) => self.summary.skipped += 1,
            None => {
                // something inside must stay: offer the children one by one
                if depth > self.opts.max_depth {
                    self.summary.skipped += 1;
                    return;
                }
                for (child, child_info) in self.children(path) {
                    self.offer(&child, &child_info, depth + 1);
                }
            }
        }
    }

    /// Total of a folder that may go as a whole; `None` if anything inside is a
    /// link, excluded, too new, unreadable or too deep.
    fn aggregate(&mut self, dir: &Path, depth: usize) -> Option<Agg> {
        if depth > self.opts.max_depth {
            return None;
        }
        let items = fsinfo::read_dir(dir, false).ok()?;
        let mut agg = Agg {
            bytes: 0,
            files: 0,
            newest: i64::MIN,
        };
        for item in items {
            let info = item.info.ok()?;
            let path = dir.join(&item.name);
            // a folder's own date changes with its contents: only files count
            if info.link != LinkKind::None
                || self.excluded(&path)
                || (!info.is_dir && info.mtime > self.cutoff)
            {
                return None;
            }
            agg.newest = agg.newest.max(info.mtime);
            if info.is_dir {
                let inner = self.aggregate(&path, depth + 1)?;
                agg.bytes += inner.bytes;
                agg.files += inner.files;
                agg.newest = agg.newest.max(inner.newest);
            } else {
                agg.bytes += info.on_disk;
                agg.files += 1;
            }
        }
        Some(agg)
    }

    fn push(&mut self, path: &Path, info: &FileInfo, bytes: u64, files: u64, newest: i64) {
        let origin = Origin::Rule {
            level: self.rule.rule.level,
        };
        if self.gate.check_path(path, origin).is_err() {
            self.summary.refused += 1;
            return;
        }
        self.out.push(Candidate {
            rule_id: self.rule.rule.id.clone(),
            pack_id: self.rule.pack.clone(),
            path: path.to_path_buf(),
            is_dir: info.is_dir,
            bytes,
            size: if info.is_dir { 0 } else { info.logical },
            files,
            newest_mtime: newest,
            mtime: info.mtime,
            level: self.rule.rule.level,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::parse_pack;

    fn pack(paths: &str, extra: &str) -> Pack {
        let text = format!(
            r#"
[pack]
id = "t"
name = "test"
version = "0.0.1"

[[rule]]
id = "t.cache"
vendor = "V"
app = "App"
category = "cache"
platform = ["windows", "linux", "macos"]
paths = [{paths}]
level = "safe"
regenerates = true
impact = "Rebuilt automatically on next start."
verified_on = 2026-10-16
{extra}
"#
        );
        parse_pack(&text, None).expect("pack")
    }

    fn run(root: &Path, paths: &str, extra: &str) -> MatchReport {
        let root_text = slash(root);
        let resolve = move |v: &str| (v == "LOCALAPPDATA").then(|| root_text.clone());
        let matcher = Matcher::new(&[pack(paths, extra)], &resolve, std::env::consts::OS);
        matcher.scan(&|_, _| true, &MatchOptions::default(), &[])
    }

    fn names(report: &MatchReport, root: &Path) -> Vec<String> {
        let mut v: Vec<String> = report
            .candidates
            .iter()
            .map(|c| {
                slash(c.path.strip_prefix(root).expect("under root"))
                    .trim_start_matches('/')
                    .to_owned()
            })
            .collect();
        v.sort();
        v
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, vec![b'x'; bytes]).expect("write");
    }

    #[test]
    fn trailing_double_star_offers_each_child_whole() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        write(&root.join("App/Cache/a/1.bin"), 10);
        write(&root.join("App/Cache/a/2.bin"), 10);
        write(&root.join("App/Cache/b.bin"), 10);
        write(&root.join("App/Keep/c.bin"), 10);
        let report = run(root, r#""${LOCALAPPDATA}/App/Cache/**""#, "");
        assert_eq!(names(&report, root), ["App/Cache/a", "App/Cache/b.bin"]);
        let a = &report.candidates.iter().find(|c| c.is_dir).expect("dir");
        assert_eq!(a.files, 2);
        assert_eq!(report.rules[0].files, 3);
        assert_eq!(report.rules[0].candidates, 2);
    }

    #[test]
    fn single_star_and_file_patterns() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        write(&root.join("Br/User Data/Default/Cache/x"), 5);
        write(&root.join("Br/User Data/Profile 1/Cache/y"), 5);
        write(&root.join("Br/User Data/Default/Cookies"), 5);
        write(&root.join("Logs/a.log"), 5);
        write(&root.join("Logs/a.txt"), 5);
        let report = run(
            root,
            r#""${LOCALAPPDATA}/Br/User Data/*/Cache/**", "${LOCALAPPDATA}/Logs/*.log""#,
            "",
        );
        assert_eq!(
            names(&report, root),
            [
                "Br/User Data/Default/Cache/x",
                "Br/User Data/Profile 1/Cache/y",
                "Logs/a.log"
            ]
        );
    }

    #[test]
    fn braces_and_middle_double_star() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        write(&root.join("P/a/b/GPUCache/1"), 1);
        write(&root.join("P/c/Code Cache/2"), 1);
        write(&root.join("P/c/IndexedDB/3"), 1);
        let report = run(root, r#""${LOCALAPPDATA}/P/**/{GPUCache,Code Cache}""#, "");
        assert_eq!(names(&report, root), ["P/a/b/GPUCache", "P/c/Code Cache"]);
    }

    #[test]
    fn exclude_splits_the_folder() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        write(&root.join("C/x/keep.db"), 1);
        write(&root.join("C/x/junk.bin"), 1);
        write(&root.join("C/y.bin"), 1);
        let report = run(
            root,
            r#""${LOCALAPPDATA}/C/**""#,
            r#"exclude = ["${LOCALAPPDATA}/C/**/*.db"]"#,
        );
        assert_eq!(names(&report, root), ["C/x/junk.bin", "C/y.bin"]);
        assert!(report.rules[0].skipped >= 1);
    }

    #[test]
    fn keep_newest_offers_only_older_revisions() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        for d in [
            "pw/chromium-1223",
            "pw/chromium-1248",
            "pw/chromium_headless_shell-1223",
            "pw/chromium_headless_shell-1248",
            "pw/ffmpeg-1011",
            "pw/ffmpeg-1013",
            "pw/webkit-2083",
            "pw/mcp-chrome-1234",
            "pw/b",
        ] {
            write(&root.join(d).join("x.bin"), 1);
        }
        let report = run(root, r#""${LOCALAPPDATA}/pw/*""#, "keep_newest = true");
        assert_eq!(
            names(&report, root),
            [
                "pw/chromium-1223",
                "pw/chromium_headless_shell-1223",
                "pw/ffmpeg-1011"
            ]
        );
        assert_eq!(report.rules[0].candidates, 3);
        assert_eq!(
            crate::revision_of("Chromium_Headless_Shell-1248"),
            Some(("chromium_headless_shell".into(), 1248))
        );
        assert_eq!(crate::revision_of("mcp-chrome-1234"), None);
        assert_eq!(crate::revision_of("chromium"), None);
    }

    #[test]
    fn min_age_skips_new_files_and_splits_folders() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        write(&root.join("T/new.bin"), 1);
        write(&root.join("T/d/new2.bin"), 1);
        let report = run(root, r#""${LOCALAPPDATA}/T/**""#, "min_age_days = 1");
        assert!(report.candidates.is_empty(), "{:?}", report.candidates);
        assert_eq!(report.rules[0].skipped, 2);
    }

    #[test]
    fn rules_matching_explains_a_path() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        let root_text = slash(root);
        let resolve = move |v: &str| (v == "LOCALAPPDATA").then(|| root_text.clone());
        let matcher = Matcher::new(
            &[pack(r#""${LOCALAPPDATA}/App/*/Cache/**""#, "")],
            &resolve,
            std::env::consts::OS,
        );
        assert_eq!(
            matcher
                .rules_matching(&root.join("App/Default/Cache/x/y"))
                .len(),
            1
        );
        assert!(
            matcher
                .rules_matching(&root.join("App/Default/Cache"))
                .is_empty()
        );
        assert!(
            matcher
                .rules_matching(&root.join("App/Default/Cookies"))
                .is_empty()
        );
    }

    #[cfg(windows)]
    #[test]
    fn junctions_are_never_followed_or_offered() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        write(&root.join("C/a.bin"), 1);
        write(&root.join("target/precious.txt"), 1);
        let link = root.join("C/link");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(root.join("target"))
            .output()
            .is_ok_and(|o| o.status.success());
        if !ok {
            return;
        }
        let report = run(root, r#""${LOCALAPPDATA}/C/**""#, "");
        assert_eq!(names(&report, root), ["C/a.bin"]);
        // a folder holding the junction is split, never offered whole
        write(&root.join("D/sub/b.bin"), 1);
        let inner = root.join("D/sub/link");
        let _ = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&inner)
            .arg(root.join("target"))
            .output();
        let report = run(root, r#""${LOCALAPPDATA}/D/**""#, "");
        assert_eq!(names(&report, root), ["D/sub/b.bin"]);
        fs::remove_dir(&link).expect("rm link");
        fs::remove_dir(&inner).expect("rm link");
    }

    #[test]
    fn protected_rules_are_kept_out_of_every_rule() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        write(&root.join("A/cache/x.bin"), 1);
        write(&root.join("A/settings.json"), 1);
        write(&root.join("A/memory/notes.md"), 1);
        let mut packs = vec![pack(r#""${LOCALAPPDATA}/A/**""#, "")];
        let mut keep = packs[0].rules[0].clone();
        keep.id = "t.keep".into();
        keep.level = RiskLevel::Protected;
        keep.paths = vec![
            "${LOCALAPPDATA}/A/settings.json".into(),
            "${LOCALAPPDATA}/A/memory/**".into(),
        ];
        packs[0].rules.push(keep);
        let root_text = slash(root);
        let resolve = move |v: &str| (v == "LOCALAPPDATA").then(|| root_text.clone());
        let report = Matcher::new(&packs, &resolve, std::env::consts::OS).scan(
            &|_, _| true,
            &MatchOptions::default(),
            &[],
        );
        assert_eq!(names(&report, root), ["A/cache"]);
        assert!(report.rules[0].skipped >= 2);
    }

    #[test]
    fn protected_rules_are_listed_but_never_walked() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        write(&root.join("P/x"), 1);
        let p = pack(r#""${LOCALAPPDATA}/P/**""#, "").rules[0].clone();
        let mut prot = pack(r#""${LOCALAPPDATA}/P/**""#, "");
        prot.rules[0] = Rule {
            level: RiskLevel::Protected,
            ..p
        };
        let root_text = slash(root);
        let resolve = move |v: &str| (v == "LOCALAPPDATA").then(|| root_text.clone());
        let report = Matcher::new(&[prot], &resolve, std::env::consts::OS).scan(
            &|_, _| true,
            &MatchOptions::default(),
            &[],
        );
        assert!(report.candidates.is_empty());
        assert_eq!(report.rules.len(), 1);
        assert_eq!(report.rules[0].bytes, 0);
    }
}
