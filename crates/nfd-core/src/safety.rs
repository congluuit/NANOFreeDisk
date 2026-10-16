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

use std::fmt;
use std::path::{Component, Path, Prefix};

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
const VOLUME_ROOT_PROTECTED: &[&str] = &[
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
    let Some(cand) = comparable(path) else {
        return Err(Blocked::Unresolvable(path.display().to_string()));
    };
    let parts: Vec<&str> = cand.split('/').filter(|p| !p.is_empty()).collect();
    // `c:` is one component, `//server/share` two.
    let volume_depth = if cand.starts_with("//") { 2 } else { 1 };
    if parts.len() <= volume_depth {
        return Err(Blocked::VolumeRoot);
    }
    if let Some(root) = expand_template("${SYSTEMROOT}", resolve) {
        if is_within(&cand, &root) {
            return Err(Blocked::SystemDir);
        }
    }
    let top = parts[volume_depth];
    if VOLUME_ROOT_PROTECTED.contains(&top) {
        return Err(Blocked::Protected {
            template: format!("<volume>/{top}"),
        });
    }
    if top == "users" && parts.len() == volume_depth + 1 {
        return Err(Blocked::ContainsProtected {
            template: "<volume>/Users".to_owned(),
        });
    }

    let subtree = PROTECTED_TEMPLATES
        .iter()
        .filter(|t| !USER_DATA_TEMPLATES.contains(t));
    for template in subtree {
        let Some(root) = expand_template(template, resolve) else {
            continue;
        };
        if is_within(&cand, &root) {
            return Err(Blocked::Protected {
                template: (*template).to_owned(),
            });
        }
        if is_within(&root, &cand) {
            return Err(Blocked::ContainsProtected {
                template: (*template).to_owned(),
            });
        }
    }
    for template in USER_DATA_TEMPLATES.iter().chain(CONTAINER_TEMPLATES) {
        let Some(root) = expand_template(template, resolve) else {
            continue;
        };
        if is_within(&root, &cand) {
            return Err(Blocked::ContainsProtected {
                template: (*template).to_owned(),
            });
        }
    }
    Ok(())
}

/// `true` when deleting `bytes` deserves an extra, louder confirmation: more than
/// 50 GiB, or more than 20 % of a volume of `volume_size` bytes (PLAN §5.2).
#[must_use]
pub const fn needs_extra_confirmation(bytes: u64, volume_size: u64) -> bool {
    bytes > 50 << 30 || (volume_size > 0 && bytes > volume_size / 5)
}

/// `true` when `inner` equals `outer` or lies below it (both [`comparable`] forms).
fn is_within(inner: &str, outer: &str) -> bool {
    inner == outer
        || (inner.len() > outer.len()
            && inner.starts_with(outer)
            && (outer.ends_with('/') || inner.as_bytes()[outer.len()] == b'/'))
}

/// Expand a `${VAR}/rest` template into its [`comparable`] form.
fn expand_template(template: &str, resolve: &impl Fn(&str) -> Option<String>) -> Option<String> {
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
fn comparable(path: &Path) -> Option<String> {
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
    let cand = strip_glob_tail(&normalize(candidate));
    PROTECTED_TEMPLATES.iter().any(|root| {
        let root = normalize(root);
        cand == root || cand.starts_with(&format!("{root}/"))
    })
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
        assert!(!is_protected_template("${HOME}/.claude/projects/*/*.jsonl"));
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
    fn prefix_does_not_overmatch_sibling_names() {
        // "System32x" must not be treated as inside "System32"
        assert!(!is_protected_template("${SYSTEMROOT}/System32x/**"));
        assert!(!is_protected_template("${HOME}/DocumentsBackup/**"));
    }
}
