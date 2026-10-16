//! Which processes the user may end (docs/HARDWARE_MONITOR.md §5). Pure: the facts
//! come from the OS, the decision is made here and unit-tested for every entry.
//!
//! The app never ends a process on its own. These rules only decide whether the
//! End-task button exists (hard block) and how many confirmations it needs.

use serde::Serialize;

/// Facts about one process, gathered right before deciding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    pub pid: u32,
    /// Image name as Windows reports it ("svchost.exe").
    pub name: String,
    /// Full image path when readable.
    pub path: Option<String>,
    /// `IsProcessCritical` / `ProcessBreakOnTermination`.
    pub critical: bool,
    /// Protected or protected-light (PPL) process (Defender, lsass with RunAsPPL…).
    pub protected: bool,
    /// This app or its elevated helper.
    pub own: bool,
    /// `%SystemRoot%` with a trailing backslash, lowercase (`c:\windows\`).
    pub system_root: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockReason {
    /// PID 0 / 4 (Idle, System).
    Kernel,
    /// One of the processes Windows cannot run without.
    SystemCore,
    Critical,
    Protected,
    /// NANOFreeDisk itself or its helper.
    Own,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarnReason {
    /// Hosts several Windows services at once.
    ServiceHost,
    /// The shell: taskbar, Start, desktop. Restarting it is the better choice.
    Explorer,
    /// Program inside the Windows folder.
    WindowsComponent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "verdict", content = "reasons")]
pub enum Verdict {
    /// No End-task action at all.
    Blocked(BlockReason),
    /// Allowed after a second confirmation that names these reasons.
    Warn(Vec<WarnReason>),
    /// Allowed after the normal confirmation.
    Allowed,
}

/// Image names that are never ended, whatever their path (lowercase, with `.exe`
/// where Windows reports one). A user program that copies one of these names is
/// blocked too — the safe side.
pub const HARD_BLOCK: &[&str] = &[
    "system",
    "idle",
    "system idle process",
    "registry",
    "memory compression",
    "secure system",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "fontdrvhost.exe",
    "dwm.exe",
];

pub fn classify(f: &Facts) -> Verdict {
    if f.pid == 0 || f.pid == 4 {
        return Verdict::Blocked(BlockReason::Kernel);
    }
    if f.own {
        return Verdict::Blocked(BlockReason::Own);
    }
    let name = f.name.trim().to_ascii_lowercase();
    if HARD_BLOCK.contains(&name.as_str()) {
        return Verdict::Blocked(BlockReason::SystemCore);
    }
    if f.critical {
        return Verdict::Blocked(BlockReason::Critical);
    }
    if f.protected {
        return Verdict::Blocked(BlockReason::Protected);
    }
    let mut warn = Vec::new();
    if name == "svchost.exe" {
        warn.push(WarnReason::ServiceHost);
    }
    if name == "explorer.exe" {
        warn.push(WarnReason::Explorer);
    }
    let in_windows = f.path.as_deref().is_some_and(|p| {
        !f.system_root.is_empty() && p.to_ascii_lowercase().starts_with(&f.system_root)
    });
    if in_windows {
        warn.push(WarnReason::WindowsComponent);
    }
    if warn.is_empty() {
        Verdict::Allowed
    } else {
        Verdict::Warn(warn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(pid: u32, name: &str) -> Facts {
        Facts {
            pid,
            name: name.into(),
            path: Some(format!(r"C:\Users\nano\AppData\Local\{name}")),
            system_root: r"c:\windows\".into(),
            ..Facts::default()
        }
    }

    #[test]
    fn every_hard_blocked_name_is_blocked() {
        for name in HARD_BLOCK {
            for variant in [name.to_string(), name.to_uppercase(), format!(" {name} ")] {
                assert_eq!(
                    classify(&facts(1234, &variant)),
                    Verdict::Blocked(BlockReason::SystemCore),
                    "{variant}"
                );
            }
        }
        // the exact list from docs/HARDWARE_MONITOR.md §5
        for name in [
            "Registry",
            "Memory Compression",
            "Secure System",
            "smss.exe",
            "csrss.exe",
            "wininit.exe",
            "winlogon.exe",
            "services.exe",
            "lsass.exe",
            "LsaIso.exe",
            "fontdrvhost.exe",
            "dwm.exe",
        ] {
            assert!(
                matches!(classify(&facts(999, name)), Verdict::Blocked(_)),
                "{name}"
            );
        }
    }

    #[test]
    fn kernel_pids_are_blocked() {
        assert_eq!(
            classify(&facts(0, "anything")),
            Verdict::Blocked(BlockReason::Kernel)
        );
        assert_eq!(
            classify(&facts(4, "anything")),
            Verdict::Blocked(BlockReason::Kernel)
        );
    }

    #[test]
    fn critical_protected_and_own_are_blocked() {
        let mut f = facts(500, "agent.exe");
        f.critical = true;
        assert_eq!(classify(&f), Verdict::Blocked(BlockReason::Critical));
        let mut f = facts(501, "MsMpEng.exe");
        f.protected = true;
        assert_eq!(classify(&f), Verdict::Blocked(BlockReason::Protected));
        let mut f = facts(502, "nanofreedisk.exe");
        f.own = true;
        assert_eq!(classify(&f), Verdict::Blocked(BlockReason::Own));
    }

    #[test]
    fn warnings_need_a_second_confirmation() {
        let mut svc = facts(600, "svchost.exe");
        svc.path = Some(r"C:\Windows\System32\svchost.exe".into());
        assert_eq!(
            classify(&svc),
            Verdict::Warn(vec![WarnReason::ServiceHost, WarnReason::WindowsComponent])
        );
        let mut ex = facts(601, "explorer.exe");
        ex.path = Some(r"C:\WINDOWS\explorer.exe".into());
        assert_eq!(
            classify(&ex),
            Verdict::Warn(vec![WarnReason::Explorer, WarnReason::WindowsComponent])
        );
        let mut notepad = facts(602, "notepad.exe");
        notepad.path = Some(r"C:\Windows\System32\notepad.exe".into());
        assert_eq!(
            classify(&notepad),
            Verdict::Warn(vec![WarnReason::WindowsComponent])
        );
    }

    #[test]
    fn ordinary_apps_are_allowed() {
        assert_eq!(classify(&facts(700, "chrome.exe")), Verdict::Allowed);
        // a folder that merely starts like the Windows folder is not it
        let mut f = facts(701, "tool.exe");
        f.path = Some(r"C:\Windows.old\tool.exe".into());
        assert_eq!(classify(&f), Verdict::Allowed);
        let mut f = facts(702, "tool.exe");
        f.path = None;
        assert_eq!(classify(&f), Verdict::Allowed);
    }

    #[test]
    fn verdict_serializes_for_the_ui() {
        let j = serde_json::to_value(Verdict::Warn(vec![WarnReason::ServiceHost])).expect("json");
        assert_eq!(
            j,
            serde_json::json!({"verdict": "warn", "reasons": ["service-host"]})
        );
        let j = serde_json::to_value(Verdict::Blocked(BlockReason::SystemCore)).expect("json");
        assert_eq!(
            j,
            serde_json::json!({"verdict": "blocked", "reasons": "system-core"})
        );
        let j = serde_json::to_value(Verdict::Allowed).expect("json");
        assert_eq!(j, serde_json::json!({"verdict": "allowed"}));
    }
}
