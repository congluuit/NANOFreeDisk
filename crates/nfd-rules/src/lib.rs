//! Rule packs: declarative TOML files (`rules/*.toml`) describing which folders are junk
//! or developer caches, how risky deleting them is, and what the user loses.
//!
//! This crate owns the schema, the loader and the validator, the packs shipped inside
//! the binary ([`builtin_packs`]) and the [`matcher`] that finds what a rule would
//! clean on this machine.

pub mod elevated;
pub mod encyclopedia;
pub mod matcher;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use nfd_core::RiskLevel;
use nfd_core::safety::is_protected_template;
use serde::{Deserialize, Serialize};

/// Path variables a rule may use. Anything else is rejected by the validator.
pub const ALLOWED_VARS: &[&str] = &[
    "HOME",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES_X86",
    "TEMP",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
];

/// Header of a rule pack file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Developer caches (AI tools, IDEs, package managers, game engines): shown in the
    /// DevCache view grouped by vendor (PHASE4_DEVCACHE §2.1).
    #[serde(default)]
    pub devcache: bool,
}

/// One rule: a set of path templates plus everything the UI needs to explain it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub vendor: String,
    pub app: String,
    pub category: String,
    #[serde(default = "default_platforms")]
    pub platform: Vec<String>,
    pub paths: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub level: RiskLevel,
    #[serde(default)]
    pub regenerates: bool,
    #[serde(default)]
    pub min_age_days: u32,
    /// Versioned tool folders (`chromium-1248`, `ffmpeg-1013`): only older revisions
    /// are offered — of the `<name>-<number>` folders next to each other on disk, the
    /// highest number of each name stays, and anything not named like a revision is
    /// never offered ([`revision_of`]).
    #[serde(default)]
    pub keep_newest: bool,
    #[serde(default)]
    pub requires_closed: Vec<String>,
    pub impact: String,
    /// Vietnamese translation of `impact`.
    #[serde(default)]
    pub impact_vi: Option<String>,
    #[serde(default)]
    pub docs: Option<String>,
    pub verified_on: toml::value::Datetime,
    /// Cleaning needs administrator rights (`%SystemRoot%\Temp`, CBS logs, kernel
    /// dumps…). What the normal run could not remove may then go through the one
    /// elevated run (`--elevated-clean`, [`elevated`]), which loads the packs itself
    /// and only cleans what such a rule matches. Only machine-wide locations
    /// ([`MACHINE_VARS`]) are allowed in an `admin` rule.
    #[serde(default)]
    pub admin: bool,
    /// The Windows tool that frees this space properly ("use the proper tool" items
    /// such as WinSxS or Windows.old). A closed list: the app opens a fixed target
    /// for each value and never runs a command line from a pack.
    #[serde(default)]
    pub tool: Option<Tool>,
}

/// The rule-pack variables that name machine-wide folders. An `admin` rule may use
/// only these: the elevated run resolves them from Windows itself, never from the
/// user's environment, and it never touches a user profile.
pub const MACHINE_VARS: &[&str] = &[
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES_X86",
];

/// A Windows tool a rule can point to instead of deleting anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    /// Disk Cleanup (`cleanmgr.exe`). "Clean up system files" there covers Windows
    /// Update Cleanup (the component store), Delivery Optimization files and previous
    /// Windows installations, with its own progress.
    DiskCleanup,
    /// Settings, System, Storage (`ms-settings:storagesense`): temporary files,
    /// previous Windows installations, Delivery Optimization files.
    StorageSettings,
    /// System Protection (`SystemPropertiesProtection.exe`): restore points and the
    /// space they may use.
    SystemProtection,
    /// Performance Options (`SystemPropertiesPerformance.exe`): the page file size
    /// (Advanced, Virtual memory).
    PerformanceOptions,
}

/// What opening a [`Tool`] starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolTarget {
    /// A Settings page URI.
    Settings(&'static str),
    /// A program of `System32` (file name only; the caller joins it to the system
    /// folder it resolved itself, never to `PATH`).
    System32(&'static str),
}

impl Tool {
    /// The fixed target of this tool.
    #[must_use]
    pub const fn target(self) -> ToolTarget {
        match self {
            Self::DiskCleanup => ToolTarget::System32("cleanmgr.exe"),
            Self::StorageSettings => ToolTarget::Settings("ms-settings:storagesense"),
            Self::SystemProtection => ToolTarget::System32("SystemPropertiesProtection.exe"),
            Self::PerformanceOptions => ToolTarget::System32("SystemPropertiesPerformance.exe"),
        }
    }

    /// The pack / wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DiskCleanup => "disk-cleanup",
            Self::StorageSettings => "storage-settings",
            Self::SystemProtection => "system-protection",
            Self::PerformanceOptions => "performance-options",
        }
    }
}

/// `(name, revision)` of a versioned tool folder (`chromium_headless_shell-1248` →
/// `("chromium_headless_shell", 1248)`); `None` for anything else, including the
/// signed-in browser profiles of Playwright MCP (`mcp-chrome-…`) and hidden folders.
#[must_use]
pub fn revision_of(name: &str) -> Option<(String, u64)> {
    let lower = name.to_lowercase();
    if lower.starts_with("mcp-") || lower.starts_with('.') {
        return None;
    }
    let (base, rev) = lower.rsplit_once('-')?;
    let ok = !base.is_empty()
        && base
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c == '-')
        && !rev.is_empty()
        && rev.chars().all(|c| c.is_ascii_digit());
    if !ok {
        return None;
    }
    Some((base.to_owned(), rev.parse().ok()?))
}

fn default_platforms() -> Vec<String> {
    vec!["windows".into()]
}

/// A parsed rule pack file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub pack: PackMeta,
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
    /// Where the pack was loaded from (not part of the file).
    #[serde(skip)]
    pub source: Option<PathBuf>,
}

/// Loader / validation errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
}

/// A single validation finding. `error == true` means the pack must be rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub pack: String,
    pub rule: Option<String>,
    pub error: bool,
    pub message: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = if self.error { "error" } else { "warning" };
        match &self.rule {
            Some(r) => write!(f, "[{kind}] {}::{r}: {}", self.pack, self.message),
            None => write!(f, "[{kind}] {}: {}", self.pack, self.message),
        }
    }
}

/// Parse a single pack from a TOML string.
pub fn parse_pack(text: &str, source: Option<&Path>) -> Result<Pack, Error> {
    let mut pack: Pack = toml::from_str(text).map_err(|e| Error::Parse {
        path: source.map(Path::to_path_buf).unwrap_or_default(),
        source: Box::new(e),
    })?;
    pack.source = source.map(Path::to_path_buf);
    Ok(pack)
}

/// Load one `*.toml` pack file.
pub fn load_pack(path: &Path) -> Result<Pack, Error> {
    let text = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_pack(&text, Some(path))
}

/// Load every `*.toml` file directly inside `dir`, sorted by file name.
pub fn load_dir(dir: &Path) -> Result<Vec<Pack>, Error> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        })
        .collect();
    files.sort();
    files.iter().map(|p| load_pack(p)).collect()
}

/// The packs shipped in `rules/`, compiled into the binary (file name, TOML text).
pub const BUILTIN_PACK_FILES: &[(&str, &str)] = &[
    (
        "ai-tools.toml",
        include_str!("../../../rules/ai-tools.toml"),
    ),
    (
        "browsers.toml",
        include_str!("../../../rules/browsers.toml"),
    ),
    (
        "game-engines.toml",
        include_str!("../../../rules/game-engines.toml"),
    ),
    (
        "gpu-shader-cache.toml",
        include_str!("../../../rules/gpu-shader-cache.toml"),
    ),
    (
        "ide-editors.toml",
        include_str!("../../../rules/ide-editors.toml"),
    ),
    (
        "package-managers.toml",
        include_str!("../../../rules/package-managers.toml"),
    ),
    (
        "windows-system.toml",
        include_str!("../../../rules/windows-system.toml"),
    ),
];

/// The DevCache catalog (packs with `devcache = true`) must keep at least this many
/// rules (PLAN §8, Phase 4 DoD).
pub const DEVCACHE_MIN_RULES: usize = 60;

/// Parse the [`BUILTIN_PACK_FILES`].
///
/// # Errors
/// A shipped pack does not parse (caught by the tests before release).
pub fn builtin_packs() -> Result<Vec<Pack>, Error> {
    BUILTIN_PACK_FILES
        .iter()
        .map(|(name, text)| parse_pack(text, Some(Path::new(name))))
        .collect()
}

/// Validate a set of packs. Returns all findings; use [`has_errors`] to decide pass/fail.
#[must_use]
pub fn validate(packs: &[Pack]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut seen_ids: HashSet<&str> = HashSet::new();
    let mut seen_packs: HashSet<&str> = HashSet::new();

    for pack in packs {
        let pname = pack.pack.id.as_str();
        if !seen_packs.insert(pname) {
            findings.push(Finding {
                pack: pname.into(),
                rule: None,
                error: true,
                message: "duplicate pack id".into(),
            });
        }
        if pack.rules.is_empty() {
            findings.push(Finding {
                pack: pname.into(),
                rule: None,
                error: false,
                message: "pack has no rules".into(),
            });
        }

        for rule in &pack.rules {
            let mut push = |error: bool, message: String| {
                findings.push(Finding {
                    pack: pname.into(),
                    rule: Some(rule.id.clone()),
                    error,
                    message,
                });
            };

            if !seen_ids.insert(rule.id.as_str()) {
                push(true, "duplicate rule id".into());
            }
            if !rule.id.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_')
            }) {
                push(true, "id must be lowercase [a-z0-9.-_]".into());
            }
            if rule.paths.is_empty() {
                push(true, "rule has no paths".into());
            }
            if rule.impact.trim().len() < 15 {
                push(true, "impact text is too short to be useful".into());
            }
            match &rule.impact_vi {
                None => push(false, "impact_vi (Vietnamese) is missing".into()),
                Some(vi) if vi.trim().len() < 15 => {
                    push(true, "impact_vi text is too short to be useful".into());
                }
                Some(_) => {}
            }
            for p in &rule.platform {
                if !matches!(p.as_str(), "windows" | "macos" | "linux") {
                    push(true, format!("unknown platform `{p}`"));
                }
            }

            for path in rule.paths.iter().chain(rule.exclude.iter()) {
                for var in extract_vars(path) {
                    if !ALLOWED_VARS.contains(&var.as_str()) {
                        push(true, format!("unknown variable `${{{var}}}` in `{path}`"));
                    }
                }
                if !path.starts_with("${") {
                    push(true, format!("path must start with a variable: `{path}`"));
                }
                if path.contains("..") {
                    push(true, format!("path must not contain `..`: `{path}`"));
                }
            }

            // The core guarantee: nothing deletable may live under a protected root.
            if rule.level.is_deletable() {
                for path in &rule.paths {
                    if is_protected_template(path) {
                        push(
                            true,
                            format!(
                                "`{path}` is inside a protected location but level is `{}`",
                                rule.level
                            ),
                        );
                    }
                }
                if rule.level == RiskLevel::Safe && !rule.regenerates {
                    push(
                        false,
                        "safe rule should normally set regenerates = true".into(),
                    );
                }
            }

            for (error, message) in admin_and_tool_findings(rule) {
                push(error, message);
            }
        }
    }
    findings
}

/// Checks of the `admin` and `tool` fields: `(error, message)`.
fn admin_and_tool_findings(rule: &Rule) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let windows_only = rule.platform.iter().all(|p| p == "windows");
    if rule.admin {
        if !rule.level.is_deletable() {
            out.push((
                true,
                "admin = true on a rule that cleans nothing (level protected)".into(),
            ));
        }
        if !windows_only {
            out.push((true, "admin = true needs platform = [\"windows\"]".into()));
        }
        for path in rule.paths.iter().chain(rule.exclude.iter()) {
            let first = extract_vars(path).into_iter().next();
            let machine = path.starts_with("${")
                && first.is_some_and(|v| MACHINE_VARS.contains(&v.as_str()))
                && extract_vars(path).len() == 1;
            if !machine {
                out.push((
                    true,
                    format!(
                        "admin rule paths must start with one machine-wide variable ({}): `{path}`",
                        MACHINE_VARS.join(", ")
                    ),
                ));
            }
        }
    } else if rule.level.is_deletable() && rule.paths.iter().any(|p| p.starts_with("${SYSTEMROOT}"))
    {
        out.push((
            false,
            "a rule inside ${SYSTEMROOT} normally needs admin = true".into(),
        ));
    }
    if rule.tool.is_some() && !windows_only {
        out.push((true, "tool needs platform = [\"windows\"]".into()));
    }
    out
}

/// `true` when any finding is an error.
#[must_use]
pub fn has_errors(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.error)
}

/// Extract `${VAR}` names from a template.
#[must_use]
pub fn extract_vars(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

/// Expand `${VAR}` occurrences using `resolve`; unresolved variables yield `None`.
pub fn expand<F>(template: &str, resolve: F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find('}')?;
        out.push_str(&resolve(&after[..end])?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
[pack]
id = "t"
name = "test"
version = "0.0.1"

[[rule]]
id = "vendor.app.cache"
vendor = "Vendor"
app = "App"
category = "ide"
paths = ["${LOCALAPPDATA}/App/Cache/**"]
level = "safe"
regenerates = true
impact = "Rebuilt automatically on next start."
verified_on = 2026-10-16
"#;

    #[test]
    fn parses_minimal_pack() {
        let pack = parse_pack(MINIMAL, None).expect("parse");
        assert_eq!(pack.rules.len(), 1);
        assert_eq!(pack.rules[0].level, RiskLevel::Safe);
        assert_eq!(pack.rules[0].platform, vec!["windows".to_string()]);
        assert!(!has_errors(&validate(&[pack])));
    }

    #[test]
    fn rejects_deletable_rule_under_protected_root() {
        let bad = MINIMAL.replace("${LOCALAPPDATA}/App/Cache/**", "${SYSTEMROOT}/System32/**");
        let pack = parse_pack(&bad, None).expect("parse");
        let findings = validate(&[pack]);
        assert!(has_errors(&findings), "{findings:?}");
    }

    #[test]
    fn protected_rule_may_point_at_protected_root() {
        let ok = MINIMAL
            .replace("${LOCALAPPDATA}/App/Cache/**", "${HOME}/.ssh/**")
            .replace("level = \"safe\"", "level = \"protected\"");
        let pack = parse_pack(&ok, None).expect("parse");
        assert!(!has_errors(&validate(&[pack])));
    }

    #[test]
    fn rejects_unknown_variable_and_dotdot() {
        let bad = MINIMAL.replace("${LOCALAPPDATA}/App/Cache/**", "${WEIRD}/../x/**");
        let pack = parse_pack(&bad, None).expect("parse");
        let findings = validate(&[pack]);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("unknown variable"))
        );
        assert!(findings.iter().any(|f| f.message.contains("..")));
    }

    #[test]
    fn rejects_duplicate_ids_across_packs() {
        let a = parse_pack(MINIMAL, None).expect("a");
        let b = parse_pack(&MINIMAL.replace("id = \"t\"", "id = \"u\""), None).expect("b");
        let findings = validate(&[a, b]);
        assert!(findings.iter().any(|f| f.message == "duplicate rule id"));
    }

    #[test]
    fn rejects_unknown_fields() {
        let bad = MINIMAL.replace("regenerates = true", "regenerates = true\nfoo = 1");
        assert!(parse_pack(&bad, None).is_err());
    }

    #[test]
    fn admin_rules_stay_in_machine_locations() {
        let admin = MINIMAL
            .replace("${LOCALAPPDATA}/App/Cache/**", "${SYSTEMROOT}/Temp/**")
            .replace("regenerates = true", "regenerates = true\nadmin = true");
        let pack = parse_pack(&admin, None).expect("parse");
        assert!(pack.rules[0].admin);
        assert!(!has_errors(&validate(&[pack])));

        // a user folder in an admin rule: the elevated run would act on whichever
        // profile the administrator account has
        let user = MINIMAL.replace("regenerates = true", "regenerates = true\nadmin = true");
        let findings = validate(&[parse_pack(&user, None).expect("parse")]);
        assert!(has_errors(&findings), "{findings:?}");

        // two variables (the second one could be anything)
        let two = admin.replace("${SYSTEMROOT}/Temp/**", "${SYSTEMROOT}/${TEMP}/**");
        assert!(has_errors(&validate(&[
            parse_pack(&two, None).expect("parse")
        ])));

        // an excluded user path counts too
        let excl = admin.replace("admin = true", "admin = true\nexclude = [\"${HOME}/x/**\"]");
        assert!(has_errors(&validate(&[
            parse_pack(&excl, None).expect("parse")
        ])));

        // nothing to clean, nothing to elevate
        let prot = admin.replace("level = \"safe\"", "level = \"protected\"");
        assert!(has_errors(&validate(&[
            parse_pack(&prot, None).expect("parse")
        ])));

        // Windows only
        let mac = admin.replace(
            "regenerates = true",
            "regenerates = true\nplatform = [\"windows\", \"macos\"]",
        );
        assert!(has_errors(&validate(&[
            parse_pack(&mac, None).expect("parse")
        ])));

        // a SYSTEMROOT rule without the flag is only a warning
        let plain = MINIMAL.replace("${LOCALAPPDATA}/App/Cache/**", "${SYSTEMROOT}/Temp/**");
        let findings = validate(&[parse_pack(&plain, None).expect("parse")]);
        assert!(!has_errors(&findings));
        assert!(findings.iter().any(|f| f.message.contains("admin = true")));
    }

    #[test]
    fn tools_are_a_closed_list() {
        let ok = MINIMAL
            .replace("level = \"safe\"", "level = \"protected\"")
            .replace("regenerates = true", "tool = \"disk-cleanup\"");
        let pack = parse_pack(&ok, None).expect("parse");
        assert_eq!(pack.rules[0].tool, Some(Tool::DiskCleanup));
        assert!(!has_errors(&validate(&[pack])));
        // anything else is refused by the loader, a command line above all
        for bad in ["\"cmd.exe /c del C:\\\\\"", "\"dism\"", "1", "\"\""] {
            let text = ok.replace("\"disk-cleanup\"", bad);
            assert!(parse_pack(&text, None).is_err(), "{bad}");
        }
        let mac = ok.replace("tool = ", "platform = [\"macos\"]\ntool = ");
        assert!(has_errors(&validate(&[
            parse_pack(&mac, None).expect("parse")
        ])));
        for t in [
            Tool::DiskCleanup,
            Tool::StorageSettings,
            Tool::SystemProtection,
            Tool::PerformanceOptions,
        ] {
            match t.target() {
                ToolTarget::Settings(uri) => assert!(uri.starts_with("ms-settings:")),
                ToolTarget::System32(exe) => {
                    assert!(exe.ends_with(".exe") && !exe.contains(['\\', '/', ' ']));
                }
            }
        }
    }

    #[test]
    fn shipped_admin_rules_and_tools() {
        let packs = builtin_packs().expect("builtin");
        let rules: Vec<&Rule> = packs.iter().flat_map(|p| &p.rules).collect();
        let get = |id: &str| {
            rules
                .iter()
                .find(|r| r.id == id)
                .copied()
                .unwrap_or_else(|| panic!("{id}"))
        };
        for id in [
            "windows.temp.system",
            "windows.crashdumps.kernel",
            "windows.logs.cbs",
            "windows.update.download-cache",
        ] {
            assert!(get(id).admin, "{id}");
        }
        assert!(!get("windows.temp.user").admin);
        assert_eq!(get("windows.winsxs").tool, Some(Tool::DiskCleanup));
        assert_eq!(get("windows.old").tool, Some(Tool::StorageSettings));
        assert_eq!(
            get("windows.restore-points").tool,
            Some(Tool::SystemProtection)
        );
        assert_eq!(
            get("windows.pagefile-hiberfil").tool,
            Some(Tool::PerformanceOptions)
        );
        // a tool replaces deleting: never on a rule that cleans
        for r in rules.iter().filter(|r| r.tool.is_some()) {
            assert_eq!(r.level, RiskLevel::Protected, "{}", r.id);
        }
    }

    #[test]
    fn expands_variables() {
        let r = |v: &str| (v == "HOME").then(|| "C:/Users/x".to_string());
        assert_eq!(
            expand("${HOME}/.claude/**", r).as_deref(),
            Some("C:/Users/x/.claude/**")
        );
        assert_eq!(expand("${NOPE}/x", r), None);
    }

    #[test]
    fn builtin_packs_match_the_rules_folder() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules");
        let on_disk = load_dir(&dir).expect("load rules/");
        let builtin = builtin_packs().expect("builtin");
        let ids = |p: &[Pack]| p.iter().map(|p| p.pack.id.clone()).collect::<Vec<_>>();
        assert_eq!(
            ids(&on_disk),
            ids(&builtin),
            "add new packs to BUILTIN_PACK_FILES"
        );
    }

    #[test]
    fn shipped_rules_are_translated() {
        for pack in builtin_packs().expect("builtin") {
            for rule in &pack.rules {
                assert!(
                    rule.impact_vi
                        .as_deref()
                        .is_some_and(|v| v.trim().len() >= 15),
                    "{} has no Vietnamese impact text",
                    rule.id
                );
            }
        }
    }

    /// The browser pack must never reach what users care about in a profile.
    #[test]
    fn browser_pack_spares_profile_data() {
        const NEVER: &[&str] = &[
            "cookies",
            "history",
            "login data",
            "bookmarks",
            "local storage",
            "indexeddb",
            "sessions",
            "extensions",
            "web data",
            "preferences",
            "places.sqlite",
            "key4.db",
            "logins.json",
        ];
        let packs = builtin_packs().expect("builtin");
        let browsers = packs
            .iter()
            .find(|p| p.pack.id == "browsers")
            .expect("browsers pack");
        for rule in &browsers.rules {
            assert_eq!(rule.level, RiskLevel::Safe);
            assert!(!rule.requires_closed.is_empty(), "{}", rule.id);
            for path in &rule.paths {
                let lower = path.to_lowercase();
                assert!(
                    lower.ends_with("/**"),
                    "{path} must clean inside a cache folder"
                );
                assert!(
                    !NEVER.iter().any(|n| lower.contains(n)),
                    "{path} touches profile data"
                );
                // never a whole profile: the cache folder name follows the profile
                let parts = lower
                    .trim_end_matches("/**")
                    .rsplit('/')
                    .next()
                    .unwrap_or("");
                assert!(
                    [
                        "cache",
                        "code cache",
                        "gpucache",
                        "cachestorage",
                        "shadercache",
                        "grshadercache",
                        "cache2",
                        "startupcache",
                        "shader-cache",
                        "jumplistcache"
                    ]
                    .contains(&parts),
                    "{path}"
                );
            }
        }
    }

    #[test]
    fn gpu_pack_is_all_safe_and_regenerating() {
        let packs = builtin_packs().expect("builtin");
        let gpu = packs
            .iter()
            .find(|p| p.pack.id == "gpu-shader-cache")
            .expect("gpu pack");
        assert!(gpu.rules.len() >= 5);
        assert!(
            gpu.rules
                .iter()
                .all(|r| r.level == RiskLevel::Safe && r.regenerates)
        );
    }

    fn devcache_rules() -> Vec<Rule> {
        builtin_packs()
            .expect("builtin")
            .into_iter()
            .filter(|p| p.pack.devcache)
            .flat_map(|p| p.rules)
            .collect()
    }

    /// PHASE4_DEVCACHE DoD: the DevCache catalog has at least 60 rules.
    #[test]
    fn devcache_catalog_has_sixty_rules() {
        let packs = builtin_packs().expect("builtin");
        let ids: Vec<&str> = packs
            .iter()
            .filter(|p| p.pack.devcache)
            .map(|p| p.pack.id.as_str())
            .collect();
        for want in [
            "ai-tools",
            "ide-editors",
            "package-managers",
            "game-engines",
        ] {
            assert!(ids.contains(&want), "{want} is not a devcache pack");
        }
        let rules = devcache_rules();
        assert!(rules.len() >= DEVCACHE_MIN_RULES, "{} rules", rules.len());
        // every vendor keeps something visible (🔒) or offers something to clean
        assert!(rules.iter().any(|r| r.level == RiskLevel::Protected));
    }

    /// No DevCache rule that deletes anything may reach config, credentials, installed
    /// extensions or toolchains: those are 🔴 Protected rules ("kept").
    #[test]
    fn devcache_rules_spare_config_and_credentials() {
        const NEVER: &[&str] = &[
            "settings.json",
            "keybindings",
            "credentials",
            "auth.json",
            "token",
            "/extensions/",
            "/.ssh",
            "config.toml",
            "config.json",
            "mcp",
            "/memory",
            "/bin/",
            "toolchains",
            "/envs/",
            "state.vscdb",
            "keystore",
            "adbkey",
            "/.git/",
            "conversations",
            "threads",
        ];
        for rule in devcache_rules() {
            if !rule.level.is_deletable() {
                continue;
            }
            for path in &rule.paths {
                let lower = path.to_lowercase();
                assert!(
                    !NEVER.iter().any(|n| lower.contains(n)),
                    "{}: {path} reaches something users keep",
                    rule.id
                );
            }
            if rule.level == RiskLevel::Safe {
                assert!(
                    rule.regenerates,
                    "{} is safe but does not regenerate",
                    rule.id
                );
            }
        }
    }

    /// PLAN §5.1: every AI conversation-history location of the SafetyGate is also a
    /// 🔴 "Keep" rule (so DevCache shows it as kept and every rule excludes it), and no
    /// rule that deletes anything reaches into one.
    #[test]
    fn agent_history_is_kept_by_the_rule_packs() {
        use nfd_core::safety::{AGENT_HISTORY_TEMPLATES, normalize};
        let packs = builtin_packs().expect("builtin");
        let rules: Vec<&Rule> = packs.iter().flat_map(|p| &p.rules).collect();
        for template in AGENT_HISTORY_TEMPLATES {
            // rule packs write `${HOME}` for the macOS-only `${MACOS_HOME}` templates
            let want = normalize(template).replace("${macos_home}", "${home}");
            let kept = rules.iter().any(|r| {
                r.level == RiskLevel::Protected
                    && r.paths.iter().any(|p| {
                        let p = normalize(p);
                        p == want || p == format!("{want}/**")
                    })
            });
            assert!(kept, "{template} has no 🔴 Keep rule");
        }
        for rule in rules.iter().filter(|r| r.level.is_deletable()) {
            for path in &rule.paths {
                assert!(
                    !is_protected_template(path),
                    "{}: {path} reaches protected data",
                    rule.id
                );
            }
        }
    }

    /// The real rule packs shipped in `rules/` must always validate.
    #[test]
    fn shipped_rule_packs_are_valid() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules");
        let packs = load_dir(&dir).expect("load rules/");
        assert!(!packs.is_empty(), "no packs found in {}", dir.display());
        let findings = validate(&packs);
        for f in &findings {
            eprintln!("{f}");
        }
        assert!(!has_errors(&findings));
    }
}
