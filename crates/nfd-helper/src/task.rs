//! "Start without the Windows prompt" (ADR 0003): a scheduled task, set up at the
//! user's request, that starts an administrator-only copy of the helper on demand.
//! This module holds the names, the task definition and the argument check; the
//! Windows side (copy, ACLs, `schtasks.exe`) is in `win_task`.

use serde::{Deserialize, Serialize};

/// Command-line switch of the helper the task starts. The task passes nothing else.
pub const TASK_FLAG: &str = "--elevated-helper-task";

/// Folder under Program Files holding one administrator-only copy per account.
pub const COPY_ROOT: &str = "NANOFreeDisk Helper";

/// File name of the copy.
pub const COPY_EXE: &str = "nanofreedisk.exe";

/// ACL of [`COPY_ROOT`]: SYSTEM and Administrators full control, Users may list it.
/// Protected: nothing is inherited from Program Files.
pub const COPY_ROOT_SDDL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;;0x1200a9;;;BU)";

/// Whether the task is set up for this account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    /// Not set up (the default): every start goes through UAC.
    Off,
    /// Set up, and the copy is this build: starts without a prompt.
    On,
    /// Set up for another build (the app was updated): the next start goes through
    /// UAC and renews the copy.
    Stale,
}

/// A string SID as Windows prints it (`S-1-5-21-…`), the only form used in names,
/// paths and ACLs here.
pub fn valid_sid(sid: &str) -> bool {
    sid.len() <= 184
        && sid.starts_with("S-1-")
        && sid[2..]
            .split('-')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Task name in the Task Scheduler root folder.
pub fn task_name(sid: &str) -> String {
    format!("NANOFreeDisk Helper-{sid}")
}

/// The pipe the app creates before it runs the task; the helper connects to it.
pub fn task_pipe_name(sid: &str) -> String {
    format!(r"\\.\pipe\nanofreedisk-helper-task-{sid}")
}

/// `<exe> --elevated-helper-task` and nothing else.
pub fn task_args(args: &[String]) -> bool {
    args.len() == 2 && args[1] == TASK_FLAG
}

/// ACL of the account's folder (and, inherited, of the copy): SYSTEM and
/// Administrators full control, the account read & execute.
pub fn copy_dir_sddl(sid: &str) -> String {
    format!("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;{sid})")
}

/// Security descriptor of the task: the account may read and run it, never change
/// or delete it.
pub fn task_sddl(sid: &str) -> String {
    format!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;{sid})")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The task definition (Task Scheduler schema 1.2). No trigger: it only runs when
/// started. The action is the absolute path of the copy with the one fixed argument:
/// Task Scheduler expands `%VAR%` from the user's own environment and `$(ArgN)` from
/// whoever runs the task, so a path that would contain either (or a quote, or a
/// control character) is refused — `None`.
pub fn task_xml(sid: &str, exe: &str, dir: &str) -> Option<String> {
    let plain = |s: &str| {
        s.len() > 3
            && s.as_bytes()[1..3] == *b":\\"
            && !s
                .chars()
                .any(|c| matches!(c, '%' | '$' | '"') || c.is_control())
    };
    if !valid_sid(sid) || !plain(exe) || !plain(dir) {
        return None;
    }
    let (exe, dir) = (xml_escape(exe), xml_escape(dir));
    let sddl = task_sddl(sid);
    let flag = TASK_FLAG;
    Some(format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Author>NANOFreeDisk</Author>
    <Description>Starts the read-only administrator helper of NANOFreeDisk (Advanced sensors, drive health, disk activity) from its protected copy, without a Windows prompt. Set up at your request; turn off "Start without the Windows prompt" in NANOFreeDisk to remove it.</Description>
    <SecurityDescriptor>{sddl}</SecurityDescriptor>
  </RegistrationInfo>
  <Triggers />
  <Principals>
    <Principal id="Author">
      <UserId>{sid}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>Parallel</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>"{exe}"</Command>
      <Arguments>{flag}</Arguments>
      <WorkingDirectory>{dir}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "S-1-5-21-870536905-3874548793-3431079483-1001";
    const EXE: &str = r"C:\Program Files\NANOFreeDisk Helper\S-1-5-21-1-2-3-1001\nanofreedisk.exe";
    const DIR: &str = r"C:\Program Files\NANOFreeDisk Helper\S-1-5-21-1-2-3-1001";

    #[test]
    fn sids_are_strict() {
        assert!(valid_sid(SID));
        assert!(valid_sid("S-1-5-18"));
        for bad in [
            "",
            "S-1-",
            "S-1-5-",
            "S-1-5-21-x",
            "S-1-5 -18",
            r"S-1-5-18\..\x",
            "S-1-5-18;(A;;FA;;;WD)",
            "s-1-5-18",
        ] {
            assert!(!valid_sid(bad), "{bad}");
        }
        assert!(!valid_sid(&format!("S-1-{}", "1-".repeat(100))));
    }

    #[test]
    fn only_the_fixed_argument() {
        let a = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(task_args(&a(&["x.exe", TASK_FLAG])));
        assert!(!task_args(&a(&["x.exe", TASK_FLAG, "--uninstall-cleanup"])));
        assert!(!task_args(&a(&["x.exe", "--uninstall-cleanup", TASK_FLAG])));
        assert!(!task_args(&a(&["x.exe"])));
        assert_eq!(
            task_pipe_name(SID),
            format!(r"\\.\pipe\nanofreedisk-helper-task-{SID}")
        );
    }

    #[test]
    fn task_definition() {
        let xml = task_xml(SID, EXE, DIR).expect("plain paths");
        assert!(xml.contains("<Triggers />"), "started on demand only");
        assert!(xml.contains(&format!("<Command>\"{EXE}\"</Command>")));
        assert!(xml.contains(&format!("<Arguments>{TASK_FLAG}</Arguments>")));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(
            xml.contains(&format!("(A;;FRFX;;;{SID})")),
            "run, not change"
        );
        assert!(
            !xml.contains("(A;;FA;;;S-1-5-21"),
            "the account never gets full control"
        );
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        // environment variables, placeholders, quotes, relative paths: refused
        for exe in [
            r"%ProgramFiles%\x\nanofreedisk.exe",
            r"C:\x\$(Arg0)\nanofreedisk.exe",
            "C:\\x\\a\" --uninstall-cleanup \"b.exe",
            r"x\nanofreedisk.exe",
            r"\\server\share\nanofreedisk.exe",
            "C:\\x\n\\nanofreedisk.exe",
        ] {
            assert_eq!(task_xml(SID, exe, DIR), None, "{exe}");
        }
        assert_eq!(task_xml("S-1-5-21-x", EXE, DIR), None);
        // markup in a folder name is escaped
        let odd = task_xml(SID, r"C:\A&B <x>\nanofreedisk.exe", DIR).expect("escaped");
        assert!(odd.contains(r"C:\A&amp;B &lt;x&gt;\nanofreedisk.exe"));
    }

    #[test]
    fn acls_never_let_the_account_write() {
        assert_eq!(
            copy_dir_sddl(SID),
            format!("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;{SID})")
        );
        // 0x1200a9 = read & execute (FILE_GENERIC_READ | FILE_GENERIC_EXECUTE);
        // Users only on the parent folder itself, not inherited
        assert!(COPY_ROOT_SDDL.starts_with("O:BAD:P"));
        assert!(COPY_ROOT_SDDL.ends_with("(A;;0x1200a9;;;BU)"));
        let ser = serde_json::to_string(&[TaskState::Off, TaskState::On, TaskState::Stale])
            .expect("json");
        assert_eq!(ser, r#"["off","on","stale"]"#);
    }
}
