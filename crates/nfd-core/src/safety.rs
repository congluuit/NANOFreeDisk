//! Hard-coded protected locations.
//!
//! This module is the first line of the SafetyGate (see `docs/PLAN.md` §5). Paths are
//! expressed as *templates* using the same variables rule packs use (`${SYSTEMROOT}`,
//! `${HOME}`, …) so that rule packs can be validated **before** any variable is expanded
//! and before the application ever touches a real filesystem.
//!
//! [`check_manual_delete`] is the runtime check behind the Analyze view's "Move to
//! Recycle Bin" action (Phase 2): it resolves the templates on this machine and compares
//! them with a real path. The full SafetyGate (Restart Manager, Authenticode, rule
//! levels) is added in Phase 3 on top of both.

pub mod gate;
pub mod macos;

use std::fmt;
use std::path::{Component, Path, Prefix};

pub use gate::{GateReport, LiveProbe, Origin, Probe, SafetyGate, Verdict, Warning};

/// Locations that must never be deleted through the application, in template form.
///
/// A template protects itself and everything below it. Matching is case-insensitive and
/// separator-agnostic (`/` and `\` are equivalent).
pub const PROTECTED_TEMPLATES: &[&str] = &[
    // Windows itself
    "${SYSTEMROOT}/System32",
    "${SYSTEMROOT}/SysWOW64",
    "${SYSTEMROOT}/WinSxS",
    "${SYSTEMROOT}/Installer",
    "${SYSTEMROOT}/servicing",
    "${SYSTEMROOT}/Boot",
    "${SYSTEMROOT}/Fonts",
    "${SYSTEMROOT}/Prefetch",
    "${SYSTEMROOT}/System",
    "${SYSTEMROOT}/SystemApps",
    "${SYSTEMROOT}/SystemResources",
    "${SYSTEMROOT}/WindowsApps",
    "${SYSTEMROOT}/assembly",
    "${SYSTEMROOT}/Microsoft.NET",
    "${SYSTEMDRIVE}/pagefile.sys",
    "${SYSTEMDRIVE}/hiberfil.sys",
    "${SYSTEMDRIVE}/swapfile.sys",
    "${SYSTEMDRIVE}/DumpStack.log.tmp",
    "${SYSTEMDRIVE}/System Volume Information",
    "${SYSTEMDRIVE}/$Recycle.Bin",
    "${SYSTEMDRIVE}/Recovery",
    "${SYSTEMDRIVE}/EFI",
    "${SYSTEMDRIVE}/Windows.old",
    "${SYSTEMDRIVE}/Program Files",
    "${SYSTEMDRIVE}/Program Files (x86)",
    "${PROGRAMFILES}",
    "${PROGRAMFILES_X86}",
    "${PROGRAMDATA}/Microsoft",
    // User data — only ever deleted file-by-file through Analyze, never through a rule
    "${HOME}/Documents",
    "${HOME}/Desktop",
    "${HOME}/Pictures",
    "${HOME}/Videos",
    "${HOME}/Music",
    "${HOME}/Downloads",
    "${HOME}/OneDrive",
    // Credentials and tool configuration
    "${HOME}/.ssh",
    "${HOME}/.gnupg",
    "${HOME}/.aws",
    "${HOME}/.azure",
    "${HOME}/.kube",
    "${HOME}/.git-credentials",
    "${HOME}/.npmrc",
    "${HOME}/.netrc",
    "${HOME}/.docker/config.json",
    "${HOME}/.cargo/credentials",
    "${HOME}/.cargo/credentials.toml",
    "${HOME}/.cargo/config.toml",
    "${HOME}/.cargo/bin",
    "${HOME}/.claude.json",
    "${HOME}/.claude/settings.json",
    "${HOME}/.claude/settings.local.json",
    "${HOME}/.codex/auth.json",
    "${HOME}/.codex/config.toml",
    "${HOME}/.gemini/oauth_creds.json",
    "${HOME}/.gemini/settings.json",
];

/// Where AI coding agents and chat apps keep their conversation / session history
/// (PLAN §5.1). Protected like [`PROTECTED_TEMPLATES`] for **every** origin — rules,
/// DevCache, Duplicates and items picked by hand — so a conversation is never lost
/// through the application. Also listed (with the reason) as 🔴 "Keep" rules in
/// `rules/ai-tools.toml`.
///
/// Only the **last** component may contain `*` / `?` wildcards (versioned database
/// names such as `thread_history_1.sqlite`), which keeps the "contains a protected
/// location" check for parent folders exact.
pub const AGENT_HISTORY_TEMPLATES: &[&str] = &[
    // Claude Code: transcripts (`claude --resume`), per-project memory, prompt history
    "${HOME}/.claude/projects",
    "${HOME}/.claude/sessions",
    "${HOME}/.claude/history.jsonl",
    // Claude Desktop: agent / Claude Code sessions, session notes, chat state
    "${APPDATA}/Claude/local-agent-mode-sessions",
    "${APPDATA}/Claude/claude-code-sessions",
    "${APPDATA}/Claude/side-session-notes",
    "${APPDATA}/Claude/IndexedDB",
    "${APPDATA}/Claude/Local Storage",
    // working folders of the app's project-less ("scratch") sessions: the app removes
    // them itself with the session
    "${APPDATA}/Claude/scratch-workspaces",
    // …and the same folders of the Microsoft Store (MSIX) build of the desktop app
    "${LOCALAPPDATA}/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude/claude-code-sessions",
    "${LOCALAPPDATA}/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude/local-agent-mode-sessions",
    "${LOCALAPPDATA}/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude/side-session-notes",
    "${LOCALAPPDATA}/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude/IndexedDB",
    "${LOCALAPPDATA}/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude/Local Storage",
    "${LOCALAPPDATA}/Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude/scratch-workspaces",
    // OpenAI Codex
    "${HOME}/.codex/sessions",
    "${HOME}/.codex/archived_sessions",
    "${HOME}/.codex/history.jsonl",
    "${HOME}/.codex/session_index.jsonl",
    "${HOME}/.codex/attachments",
    "${HOME}/.codex/memories",
    "${HOME}/.codex/thread_history_*",
    "${HOME}/.codex/state_*",
    "${HOME}/.codex/memories_*",
    // Gemini CLI (chats and checkpoints live in tmp/<project>/), Qwen Code
    "${HOME}/.gemini/tmp",
    "${HOME}/.gemini/history",
    "${HOME}/.qwen/tmp",
    // Antigravity (agent, CLI and IDE: conversations, brain, knowledge)
    "${HOME}/.gemini/antigravity*",
    "${APPDATA}/Antigravity/User/workspaceStorage",
    "${APPDATA}/Antigravity/User/globalStorage",
    "${APPDATA}/Antigravity IDE/User/workspaceStorage",
    "${APPDATA}/Antigravity IDE/User/globalStorage",
    // VS Code and its forks keep AI chats per workspace and per extension
    // (Copilot Chat, Cline, Roo Code…)
    "${APPDATA}/Code/User/workspaceStorage",
    "${APPDATA}/Code/User/globalStorage",
    "${APPDATA}/Code - Insiders/User/workspaceStorage",
    "${APPDATA}/Code - Insiders/User/globalStorage",
    "${APPDATA}/VSCodium/User/workspaceStorage",
    "${APPDATA}/VSCodium/User/globalStorage",
    "${APPDATA}/Cursor/User/workspaceStorage",
    "${APPDATA}/Cursor/User/globalStorage",
    "${HOME}/.cursor/chats",
    "${APPDATA}/Windsurf/User/workspaceStorage",
    "${APPDATA}/Windsurf/User/globalStorage",
    "${HOME}/.codeium/windsurf/cascade",
    "${HOME}/.codeium/windsurf/brain",
    "${HOME}/.codeium/windsurf/memories",
    "${APPDATA}/Trae/User/workspaceStorage",
    "${APPDATA}/Trae/User/globalStorage",
    "${APPDATA}/Trae/ModularData/ai-agent",
    "${HOME}/.trae/memory",
    "${APPDATA}/Kiro/User/workspaceStorage",
    "${APPDATA}/Kiro/User/globalStorage",
    // Devin (Windsurf's successor) and its CLI
    "${APPDATA}/Devin/User/workspaceStorage",
    "${APPDATA}/Devin/User/globalStorage",
    "${APPDATA}/Devin/User/acp-messages",
    "${APPDATA}/Devin/summaries",
    "${APPDATA}/Devin/cli/summaries",
    "${APPDATA}/Devin/cli/sessions.db*",
    "${APPDATA}/Devin/windsurfSessionHandoff.db*",
    // ZCode (Z.ai)
    "${HOME}/.zcode",
    "${APPDATA}/ZCode/session/IndexedDB",
    "${APPDATA}/ZCode/session/Local Storage",
    // GitHub Copilot CLI
    "${HOME}/.copilot/session-state",
    "${HOME}/.copilot/history-session-state",
    // opencode, Continue, LM Studio, Jan
    "${HOME}/.local/share/opencode",
    "${APPDATA}/ai.opencode.desktop",
    "${HOME}/.continue/sessions",
    "${HOME}/.lmstudio/conversations",
    "${APPDATA}/Jan/data/threads",
    // macOS only (`${MACOS_HOME}` resolves nowhere else): Claude Desktop, VS Code,
    // Cursor, Windsurf, Antigravity… are covered above (`${APPDATA}` is
    // `~/Library/Application Support` there); these apps exist on macOS only
    "${MACOS_HOME}/Library/Application Support/com.openai.chat",
    "${MACOS_HOME}/Library/Application Support/Codex",
    "${MACOS_HOME}/Library/Application Support/com.github.CopilotForXcode",
    "${MACOS_HOME}/Library/Application Support/Ollama",
    "${MACOS_HOME}/Library/Application Support/Zed/threads",
    "${MACOS_HOME}/Library/Application Support/com.raycast.macos",
    "${MACOS_HOME}/.config/zed/conversations",
];

/// File and folder names that hold AI chat history wherever they are (inside
/// projects): Aider's chat log, input history and LLM log, SpecStory's saved chats.
pub const AGENT_HISTORY_NAMES: &[&str] = &[
    ".aider.chat.history.md",
    ".aider.input.history",
    ".aider.llm.history",
    ".specstory",
];

/// `(folder, name)` pairs that hold AI chat history wherever the folder is: the
/// agents' home folders moved with `CODEX_HOME`, `CLAUDE_CONFIG_DIR`,
/// `GEMINI_CLI_HOME`… keep their sessions under the same names (lower case).
pub const AGENT_HISTORY_SUFFIXES: &[(&str, &str)] = &[
    (".codex", "sessions"),
    (".codex", "archived_sessions"),
    (".codex", "history.jsonl"),
    (".codex", "session_index.jsonl"),
    (".codex", "memories"),
    (".codex", "attachments"),
    (".claude", "projects"),
    (".claude", "sessions"),
    (".claude", "history.jsonl"),
    (".gemini", "tmp"),
    (".gemini", "history"),
    (".qwen", "tmp"),
    (".continue", "sessions"),
    (".copilot", "session-state"),
    (".copilot", "history-session-state"),
];

/// The user-data subset of [`PROTECTED_TEMPLATES`]: never touched by a rule, but the
/// user may pick files *inside* them by hand in Analyze. The folders themselves stay
/// protected.
pub const USER_DATA_TEMPLATES: &[&str] = &[
    "${HOME}/Documents",
    "${HOME}/Desktop",
    "${HOME}/Pictures",
    "${HOME}/Videos",
    "${HOME}/Music",
    "${HOME}/Downloads",
    "${HOME}/OneDrive",
];

/// Folders whose *contents* may be deleted by hand but which must never be deleted
/// themselves (nor any of their ancestors).
pub const CONTAINER_TEMPLATES: &[&str] = &[
    "${HOME}",
    "${APPDATA}",
    "${LOCALAPPDATA}",
    "${PROGRAMDATA}",
    "${TEMP}",
    "${SYSTEMDRIVE}/Users",
];

/// Names protected (with everything below them) directly under the root of *any*
/// volume, not only the system drive.
pub(crate) const VOLUME_ROOT_PROTECTED: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    "$extend",
    "$mft",
    "$logfile",
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "dumpstack.log.tmp",
    "recovery",
    "boot",
    "efi",
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
];

/// Why the Analyze view refuses to delete a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// The root of a volume (`C:\`, `\\server\share`).
    VolumeRoot,
    /// Inside the Windows directory. Junk there is cleaned through rules (Phase 3).
    SystemDir,
    /// Equal to or inside a protected location.
    Protected { template: String },
    /// A protected location (or a folder whose contents may only be deleted item by
    /// item) is this path or lies below it.
    ContainsProtected { template: String },
    /// Symlinks, junctions and mount points are removed in Explorer, not here.
    Link,
    /// A volume mount point lies below this directory.
    ContainsMountPoint,
    /// The path no longer exists or cannot be resolved.
    Unresolvable(String),
    /// Names an NTFS alternate data stream (`file.txt:stream`).
    Stream,
    /// A device path (`\.\PhysicalDrive0`) or a reserved DOS device name (`NUL`, `COM1`).
    Device,
    /// A link among the parent folders leads onto another volume.
    CrossVolume,
    /// Carries the SYSTEM attribute where Windows or programs keep their files.
    SystemFile,
    /// A program image (`.exe`, `.dll`, `.sys`…) inside the Windows folder.
    SystemBinary,
    /// A cloud placeholder: deleting it here deletes it in the cloud as well.
    CloudFile,
    /// The item changed (type, size or date) since the plan was made.
    Changed,
    /// The rule marks this location as protected: the app only explains it.
    ProtectedLevel,
    /// Another item of the same plan already contains this one.
    Nested,
}

impl Blocked {
    /// Stable identifier for UIs that localize the message.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::VolumeRoot => "volume-root",
            Self::SystemDir => "system-dir",
            Self::Protected { .. } => "protected",
            Self::ContainsProtected { .. } => "contains-protected",
            Self::Link => "link",
            Self::ContainsMountPoint => "contains-mount-point",
            Self::Unresolvable(_) => "unresolvable",
            Self::Stream => "stream",
            Self::Device => "device",
            Self::CrossVolume => "cross-volume",
            Self::SystemFile => "system-file",
            Self::SystemBinary => "system-binary",
            Self::CloudFile => "cloud-file",
            Self::Changed => "changed",
            Self::ProtectedLevel => "protected-level",
            Self::Nested => "nested",
        }
    }

    /// The template involved, if any.
    #[must_use]
    pub fn template(&self) -> Option<&str> {
        match self {
            Self::Protected { template } | Self::ContainsProtected { template } => Some(template),
            _ => None,
        }
    }
}

impl fmt::Display for Blocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VolumeRoot => f.write_str("the root of a volume cannot be deleted"),
            Self::SystemDir => {
                f.write_str("files inside the Windows directory are never deleted by hand")
            }
            Self::Protected { template } => write!(f, "protected location ({template})"),
            Self::ContainsProtected { template } => {
                write!(f, "contains a protected location ({template})")
            }
            Self::Link => f.write_str("links, junctions and mount points are not deleted here"),
            Self::ContainsMountPoint => f.write_str("contains a volume mount point"),
            Self::Unresolvable(why) => write!(f, "cannot resolve path: {why}"),
            Self::Stream => f.write_str("alternate data streams are not deleted"),
            Self::Device => f.write_str("device paths and reserved device names are refused"),
            Self::CrossVolume => f.write_str("a link in the path leads onto another drive"),
            Self::SystemFile => f.write_str("system file in a Windows or program folder"),
            Self::SystemBinary => f.write_str("program file inside the Windows folder"),
            Self::CloudFile => {
                f.write_str("cloud placeholder: deleting it would delete the cloud copy too")
            }
            Self::Changed => f.write_str("the item changed since it was planned"),
            Self::ProtectedLevel => f.write_str("the rule marks this location as protected"),
            Self::Nested => f.write_str("already included through a containing folder"),
        }
    }
}

/// Decide whether the user may move `path` to the Recycle Bin from the Analyze view.
///
/// `path` must be the real location (no link among its parents), as produced by the
/// scanner; `resolve` maps template variables (`HOME`, `SYSTEMROOT`, …) to absolute
/// paths on this machine (`nfd_platform::Platform::resolver`). Variables that do not
/// resolve are skipped. The caller still checks the entry itself for links and for
/// mount points below it ([`Blocked::Link`], [`Blocked::ContainsMountPoint`]).
///
/// # Errors
/// The reason the path is refused.
pub fn check_manual_delete(
    path: &Path,
    resolve: &impl Fn(&str) -> Option<String>,
) -> Result<(), Blocked> {
    SafetyGate::new(resolve).check_path(path, Origin::Manual)
}

/// `true` when deleting `bytes` deserves an extra, louder confirmation: more than
/// 50 GiB, or more than 20 % of a volume of `volume_size` bytes (PLAN §5.2).
#[must_use]
pub const fn needs_extra_confirmation(bytes: u64, volume_size: u64) -> bool {
    bytes > 50 << 30 || (volume_size > 0 && bytes > volume_size / 5)
}

/// `true` when `inner` equals `outer` or lies below it (both [`comparable`] forms).
pub(crate) fn is_within(inner: &str, outer: &str) -> bool {
    inner == outer
        || (inner.len() > outer.len()
            && inner.starts_with(outer)
            && (outer.ends_with('/') || inner.as_bytes()[outer.len()] == b'/'))
}

/// `true` when `pattern` (a [`comparable`] path whose last component may contain
/// `*` / `?`) names `cand` or one of its ancestors.
pub(crate) fn within_pattern(cand: &str, pattern: &str) -> bool {
    match split_glob(pattern) {
        None => is_within(cand, pattern),
        Some((parent, last)) => {
            if !is_within(cand, parent) || cand.len() == parent.len() {
                return false;
            }
            let rest = cand[parent.len()..].trim_start_matches('/');
            let first = rest.split('/').next().unwrap_or_default();
            wildcard(last, first)
        }
    }
}

/// `true` when `cand` is a folder that holds (or may hold) what `pattern` names:
/// `cand` is the pattern itself or an ancestor of it.
pub(crate) fn contains_pattern(cand: &str, pattern: &str) -> bool {
    match split_glob(pattern) {
        None => is_within(pattern, cand),
        Some((parent, _)) => is_within(parent, cand),
    }
}

/// `(parent, last)` of a pattern whose last component has a wildcard.
fn split_glob(pattern: &str) -> Option<(&str, &str)> {
    let (parent, last) = pattern.rsplit_once('/')?;
    (last.contains(['*', '?']) && !parent.is_empty()).then_some((parent, last))
}

/// Shell-style match of one name: `*` any run, `?` one character.
pub(crate) fn wildcard(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// The chat-history entry ([`AGENT_HISTORY_NAMES`], [`AGENT_HISTORY_SUFFIXES`]) a
/// folder entry `name` inside a folder named `parent` is, if any (case-insensitive).
/// Used on paths and by walkers that look for history *below* a folder.
#[must_use]
pub fn agent_history_entry(parent: Option<&str>, name: &str) -> Option<&'static str> {
    let name = name.to_lowercase();
    if let Some(n) = AGENT_HISTORY_NAMES.iter().find(|n| **n == name) {
        return Some(n);
    }
    let parent = parent?.to_lowercase();
    AGENT_HISTORY_SUFFIXES
        .iter()
        .find(|(p, n)| *p == parent && *n == name)
        .map(|(p, _)| *p)
}

/// The chat-history entry among the components of `cand` ([`agent_history_entry`]),
/// as `name` or `folder/name`.
pub(crate) fn agent_history_name(cand: &str) -> Option<String> {
    let parts: Vec<&str> = cand.split('/').collect();
    parts.iter().enumerate().find_map(|(i, part)| {
        let parent = i.checked_sub(1).map(|j| parts[j]);
        agent_history_entry(parent, part).map(|hit| {
            if hit.eq_ignore_ascii_case(part) {
                hit.to_owned()
            } else {
                format!("{hit}/{part}")
            }
        })
    })
}

/// Expand a `${VAR}/rest` template into its [`comparable`] form.
pub(crate) fn expand_template(
    template: &str,
    resolve: &impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let rest = template.strip_prefix("${")?;
    let (var, tail) = rest.split_once('}')?;
    let base = resolve(var)?;
    if base.trim().is_empty() && tail.is_empty() {
        return None;
    }
    comparable(Path::new(&format!("{base}{tail}")))
}

/// Lower-case, `/`-separated absolute form of a path with verbatim prefixes removed
/// (`\\?\C:\x` → `c:/x`, `\\?\UNC\srv\share` → `//srv/share`), `.` / `..` resolved
/// lexically and, on Windows, trailing dots and spaces trimmed from every name (the
/// Win32 layer ignores them). `None` for relative paths.
pub(crate) fn comparable(path: &Path) -> Option<String> {
    // On Windows `\Windows` is relative to the current drive: not absolute.
    if !path.is_absolute() {
        return None;
    }
    let mut prefix = String::new();
    let mut parts: Vec<String> = Vec::new();
    let mut absolute = false;
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => {
                prefix = match p.kind() {
                    Prefix::Disk(d) | Prefix::VerbatimDisk(d) => format!("{}:", char::from(d)),
                    Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                        format!("//{}/{}", server.to_string_lossy(), share.to_string_lossy())
                    }
                    Prefix::Verbatim(s) | Prefix::DeviceNS(s) => {
                        format!("//./{}", s.to_string_lossy())
                    }
                };
                // UNC prefixes carry their own root.
                absolute |= prefix.starts_with("//");
            }
            Component::RootDir => absolute = true,
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(n) => {
                let n = n.to_string_lossy();
                let n = if cfg!(windows) {
                    n.trim_end_matches(['.', ' '])
                } else {
                    &n
                };
                if !n.is_empty() {
                    parts.push(n.to_owned());
                }
            }
        }
    }
    if !absolute {
        return None;
    }
    let mut out = prefix;
    out.push('/');
    out.push_str(&parts.join("/"));
    Some(normalize(&out))
}

/// Normalize a template or path for comparison: forward slashes, lowercase, no trailing slash.
#[must_use]
pub fn normalize(path: &str) -> String {
    let mut s: String = path
        .trim()
        .chars()
        .map(|c| {
            if c == '\\' {
                '/'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    while s.ends_with('/') && s.len() > 1 {
        s.pop();
    }
    s
}

/// Strip trailing glob components (`/**`, `/*`, `/*.log`) so `${SYSTEMROOT}/System32/**`
/// is checked as `${SYSTEMROOT}/System32`.
#[must_use]
pub fn strip_glob_tail(path: &str) -> String {
    let mut parts: Vec<&str> = path.split('/').collect();
    while let Some(last) = parts.last() {
        if last.contains('*') || last.contains('?') || last.contains('{') {
            parts.pop();
        } else {
            break;
        }
    }
    parts.join("/")
}

/// Returns `true` when `candidate` (a template such as `${SYSTEMROOT}/System32/drivers/**`)
/// is equal to or nested inside any entry of [`PROTECTED_TEMPLATES`].
///
/// This is a *template-level* check meant for validating rule packs. It deliberately
/// does **not** try to resolve variables.
#[must_use]
pub fn is_protected_template(candidate: &str) -> bool {
    // rule packs write `${HOME}`: the macOS-only templates are checked as such
    let as_home = |t: &str| normalize(t).replace("${macos_home}", "${home}");
    let cand = as_home(&strip_glob_tail(&normalize(candidate)));
    PROTECTED_TEMPLATES
        .iter()
        .chain(AGENT_HISTORY_TEMPLATES)
        .chain(macos::MACOS_PROTECTED_TEMPLATES)
        .chain(macos::MACOS_USER_DATA_TEMPLATES)
        .any(|root| within_pattern(&cand, &as_home(root)))
        || agent_history_name(&cand).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_separators_and_case() {
        assert_eq!(
            normalize("${SYSTEMROOT}\\System32\\"),
            "${systemroot}/system32"
        );
    }

    #[test]
    fn strips_glob_tail() {
        assert_eq!(
            strip_glob_tail("${home}/.claude/debug/**"),
            "${home}/.claude/debug"
        );
        assert_eq!(
            strip_glob_tail("${systemroot}/logs/cbs/*.log"),
            "${systemroot}/logs/cbs"
        );
        assert_eq!(
            strip_glob_tail("${home}/.claude.json"),
            "${home}/.claude.json"
        );
    }

    #[test]
    fn detects_nested_protected_paths() {
        assert!(is_protected_template("${SYSTEMROOT}/System32/**"));
        assert!(is_protected_template("${SYSTEMROOT}\\WinSxS\\Temp\\**"));
        assert!(is_protected_template("${HOME}/Documents/**"));
        assert!(is_protected_template("${HOME}/.claude.json"));
        assert!(is_protected_template("${HOME}/.ssh/id_ed25519"));
        assert!(is_protected_template("${PROGRAMFILES}/Foo/cache/**"));
    }

    #[test]
    fn allows_legitimate_junk_locations() {
        assert!(!is_protected_template("${SYSTEMROOT}/Temp/**"));
        assert!(!is_protected_template("${SYSTEMROOT}/Logs/CBS/*.log"));
        assert!(!is_protected_template(
            "${SYSTEMROOT}/SoftwareDistribution/Download/**"
        ));
        assert!(!is_protected_template("${HOME}/.claude/debug/**"));
        assert!(is_protected_template("${HOME}/.claude/projects/*/*.jsonl"));
        assert!(!is_protected_template("${HOME}/.cargo/registry/cache/**"));
        assert!(!is_protected_template("${TEMP}/**"));
        assert!(!is_protected_template("${LOCALAPPDATA}/NVIDIA/DXCache/**"));
    }

    fn resolver(var: &str) -> Option<String> {
        let base = if cfg!(windows) { "C:" } else { "" };
        Some(match var {
            "SYSTEMROOT" => format!("{base}/Windows"),
            "SYSTEMDRIVE" => base.to_owned(),
            "HOME" | "USERPROFILE" => format!("{base}/Users/nano"),
            "APPDATA" => format!("{base}/Users/nano/AppData/Roaming"),
            "LOCALAPPDATA" => format!("{base}/Users/nano/AppData/Local"),
            "TEMP" => format!("{base}/Users/nano/AppData/Local/Temp"),
            "PROGRAMDATA" => format!("{base}/ProgramData"),
            "PROGRAMFILES" => format!("{base}/Program Files"),
            "PROGRAMFILES_X86" => format!("{base}/Program Files (x86)"),
            _ => return None,
        })
    }

    /// Check a Windows-style path; on Unix the drive letter is dropped.
    fn manual(p: &str) -> Result<(), Blocked> {
        let p = if cfg!(windows) {
            p.to_owned()
        } else {
            p.trim_start_matches("C:").replace('\\', "/")
        };
        check_manual_delete(Path::new(&p), &resolver)
    }

    fn refused_as_protected(p: &str) -> bool {
        matches!(manual(p), Err(Blocked::Protected { .. }))
    }

    #[test]
    fn manual_delete_allows_ordinary_paths() {
        for p in [
            r"C:\Users\nano\Downloads\big.iso",
            r"C:\Users\nano\Documents\old\x.zip",
            r"C:\Users\nano\AppData\Local\Temp\foo",
            r"C:\Users\nano\AppData\Local\NVIDIA\DXCache",
            r"C:\Users\nano\.cargo\registry",
            r"C:\Users\nano\.claude\debug",
            r"C:\Games\Old",
            r"C:\Users\nano\DocumentsBackup",
        ] {
            assert_eq!(manual(p), Ok(()), "{p} should be allowed");
        }
    }

    #[test]
    fn manual_delete_blocks_system_and_protected() {
        assert_eq!(manual(r"C:\Windows\Temp\x.log"), Err(Blocked::SystemDir));
        assert_eq!(manual(r"C:\WINDOWS\System32"), Err(Blocked::SystemDir));
        assert!(refused_as_protected(r"C:\Program Files\App"));
        assert!(refused_as_protected(r"C:\Program Files (x86)"));
        assert!(refused_as_protected(r"C:\Users\nano\.ssh\id_ed25519"));
        assert!(refused_as_protected(r"C:\Users\nano\.claude.json"));
        assert!(refused_as_protected(r"C:\Users\nano\.cargo\bin\cargo.exe"));
        assert!(refused_as_protected(r"C:\ProgramData\Microsoft\Windows"));
        assert!(refused_as_protected(r"C:\pagefile.sys"));
        assert!(refused_as_protected(r"C:\$Recycle.Bin"));
        assert_eq!(manual(r"C:\"), Err(Blocked::VolumeRoot));
    }

    #[test]
    fn manual_delete_blocks_containers_and_ancestors() {
        for p in [
            r"C:\Users",
            r"C:\Users\nano",
            r"C:\Users\nano\Documents",
            r"C:\Users\nano\Downloads\",
            r"C:\Users\nano\AppData",
            r"C:\Users\nano\AppData\Local",
            r"C:\Users\nano\AppData\Local\Temp",
            r"C:\Users\nano\.claude",
            r"C:\Users\nano\.cargo",
        ] {
            assert!(
                matches!(manual(p), Err(Blocked::ContainsProtected { .. })),
                "{p} should be refused, got {:?}",
                manual(p)
            );
        }
    }

    #[test]
    fn manual_delete_resists_lexical_tricks() {
        assert_eq!(
            manual(r"C:\Games\..\Windows\notepad.exe"),
            Err(Blocked::SystemDir)
        );
        assert!(refused_as_protected(r"C:\Users\nano\Documents\..\.ssh"));
        assert!(manual(r"C:\Users\nano\.\Documents").is_err());
        assert!(matches!(
            manual("relative/path"),
            Err(Blocked::Unresolvable(_))
        ));
    }

    #[cfg(windows)]
    #[test]
    fn manual_delete_windows_forms() {
        assert_eq!(manual(r"\\?\C:\Windows\x"), Err(Blocked::SystemDir));
        assert!(manual(r"C:\Users\nano\Documents.").is_err());
        assert!(manual(r"C:\Users\nano\Documents .").is_err());
        assert!(refused_as_protected(r"D:\$Recycle.Bin\S-1-5"));
        assert!(refused_as_protected(r"D:\System Volume Information"));
        assert!(refused_as_protected(r"D:\Windows\x"));
        assert!(matches!(
            manual(r"D:\Users"),
            Err(Blocked::ContainsProtected { .. })
        ));
        assert_eq!(manual(r"D:\"), Err(Blocked::VolumeRoot));
        assert_eq!(manual(r"\\nas\share"), Err(Blocked::VolumeRoot));
        assert_eq!(manual(r"\\?\UNC\nas\share\"), Err(Blocked::VolumeRoot));
        assert_eq!(manual(r"\\nas\share\movies\a.mkv"), Ok(()));
        assert_eq!(manual(r"D:\Projects\target"), Ok(()));
        assert_eq!(manual(r"D:\Users\old\stuff"), Ok(()));
    }

    #[test]
    fn extra_confirmation_thresholds() {
        assert!(!needs_extra_confirmation(1 << 30, 1 << 40));
        assert!(needs_extra_confirmation(51 << 30, 1 << 42));
        assert!(needs_extra_confirmation(30 << 30, 100 << 30));
        assert!(!needs_extra_confirmation(10 << 30, 0));
    }

    #[test]
    fn macos_templates_check_rule_packs_too() {
        assert!(is_protected_template("${HOME}/Library/Mobile Documents/**"));
        assert!(is_protected_template(
            "${HOME}/Library/Containers/com.x/Data/**"
        ));
        assert!(is_protected_template(
            "${MACOS_HOME}/Library/Application Support/com.openai.chat/**"
        ));
        assert!(!is_protected_template("${HOME}/Library/Caches/com.x/**"));
    }

    #[test]
    fn prefix_does_not_overmatch_sibling_names() {
        // "System32x" must not be treated as inside "System32"
        assert!(!is_protected_template("${SYSTEMROOT}/System32x/**"));
        assert!(!is_protected_template("${HOME}/DocumentsBackup/**"));
    }
}
