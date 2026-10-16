//! Platform abstraction. Everything OS-specific in NANOFreeDisk goes behind the
//! [`Platform`] trait so the core stays portable. Windows is implemented first;
//! macOS and Linux follow in Phase 10 (see `docs/PLAN.md`).
//!
//! Phase 0 provides known-folder resolution for rule-pack variables; Phase 1 adds
//! [`fsinfo`], the per-entry metadata query used by the scanner; Phase 2 adds
//! [`volumes`] (drive overview), [`trash`] (Recycle Bin) and [`appearance`] (accent
//! color). Restart-Manager lock detection and elevation follow in Phase 3.

pub mod appearance;
pub mod fsinfo;
pub mod trash;
pub mod volumes;

use std::env;
use std::path::PathBuf;

/// OS services the core needs.
pub trait Platform: Send + Sync {
    /// Resolve a rule-pack variable (`HOME`, `LOCALAPPDATA`, …) to an absolute path.
    fn known_folder(&self, var: &str) -> Option<PathBuf>;

    /// Convenience: resolver closure usable with `nfd_rules::expand`.
    fn resolver(&self) -> impl Fn(&str) -> Option<String> + '_ {
        move |v| {
            self.known_folder(v)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        }
    }
}

/// The platform the binary was built for.
#[derive(Debug, Default, Clone, Copy)]
pub struct Native;

impl Platform for Native {
    #[cfg(windows)]
    fn known_folder(&self, var: &str) -> Option<PathBuf> {
        let env_name = match var {
            "HOME" | "USERPROFILE" => "USERPROFILE",
            "APPDATA" => "APPDATA",
            "LOCALAPPDATA" => "LOCALAPPDATA",
            "PROGRAMDATA" => "ProgramData",
            "PROGRAMFILES" => "ProgramFiles",
            "PROGRAMFILES_X86" => "ProgramFiles(x86)",
            "TEMP" => "TEMP",
            "SYSTEMROOT" => "SystemRoot",
            "SYSTEMDRIVE" => "SystemDrive",
            _ => return None,
        };
        env::var_os(env_name).map(PathBuf::from)
    }

    #[cfg(not(windows))]
    fn known_folder(&self, var: &str) -> Option<PathBuf> {
        let home = env::var_os("HOME").map(PathBuf::from)?;
        Some(match var {
            "HOME" | "USERPROFILE" => home,
            // XDG-ish approximations; refined in the macOS/Linux phase.
            "APPDATA" => home.join(".config"),
            "LOCALAPPDATA" => home.join(".local/share"),
            "TEMP" => env::temp_dir(),
            "PROGRAMDATA" | "PROGRAMFILES" | "PROGRAMFILES_X86" | "SYSTEMROOT" | "SYSTEMDRIVE" => {
                return None;
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_resolves_on_every_platform() {
        let p = Native;
        assert!(p.known_folder("HOME").is_some());
        assert!(p.known_folder("NOT_A_VAR").is_none());
    }
}
