//! The command a program registered to remove itself (`UninstallString`), split
//! into the program to start and its arguments (`docs/UNINSTALLER.md` §2.1).
//!
//! Registered strings come in every shape: quoted or not, paths with spaces and no
//! quotes, `%ProgramFiles%` variables, `MsiExec.exe /I{…}` (which *modifies* the
//! product — uninstalling is `/X{…}`), `rundll32` entry points. Parsing never runs
//! anything; whether the program is still on disk decides if the entry is "broken".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A program to start and its arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub exe: PathBuf,
    pub args: String,
    /// What the registry holds (for the confirmation and the CLI).
    pub raw: String,
}

impl Command {
    /// The command line as it will run: `"exe" args`.
    #[must_use]
    pub fn line(&self) -> String {
        if self.args.is_empty() {
            format!("\"{}\"", self.exe.display())
        } else {
            format!("\"{}\" {}", self.exe.display(), self.args)
        }
    }

    /// The program's file name, lower case (`msiexec.exe`).
    #[must_use]
    pub fn file_name(&self) -> String {
        self.exe
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }
}

/// Expand `%VAR%` with `env` (case-insensitive names); unknown variables stay.
#[must_use]
pub fn expand_vars(s: &str, env: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 && !after[..end].contains([' ', '\\', '"']) => {
                let name = &after[..end];
                match env(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// An MSI product code (`{8-4-4-4-12}`) at the start of `s`.
fn guid_at(s: &str) -> Option<&str> {
    let s = s.trim_start();
    let end = s.find('}')?;
    let g = &s[..=end];
    let inner = g.strip_prefix('{')?.strip_suffix('}')?;
    let parts: Vec<&str> = inner.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    (parts.len() == 5
        && parts
            .iter()
            .zip(lens)
            .all(|(p, l)| p.len() == l && p.chars().all(|c| c.is_ascii_hexdigit())))
    .then_some(g)
}

/// The product code of a `MsiExec.exe /I{…}` / `/X{…}` command, upper case.
#[must_use]
pub fn msi_product_code(raw: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    if !lower.contains("msiexec") {
        return None;
    }
    for flag in ["/x", "/i", "-x", "-i", "/uninstall", "/package"] {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(flag) {
            let at = from + pos + flag.len();
            if let Some(g) = guid_at(&raw[at..]) {
                return Some(g.to_ascii_uppercase());
            }
            from = at;
        }
    }
    None
}

/// `msiexec.exe /x {code}` from the Windows folder.
#[must_use]
pub fn msi_uninstall(product_code: &str, system_root: &Path, raw: &str) -> Command {
    Command {
        exe: system_root.join("System32").join("msiexec.exe"),
        args: format!("/x {product_code}"),
        raw: raw.to_owned(),
    }
}

/// Split an `UninstallString`. `env` expands `%VAR%`; `exists` decides which prefix
/// of an unquoted path with spaces is the program; `system_root` resolves bare
/// `MsiExec.exe` / `rundll32.exe`. `None` for an empty string.
#[must_use]
pub fn parse(
    raw: &str,
    env: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
    system_root: &Path,
) -> Option<Command> {
    let raw_trim = raw.trim();
    if raw_trim.is_empty() {
        return None;
    }
    if let Some(code) = msi_product_code(raw_trim) {
        return Some(msi_uninstall(&code, system_root, raw_trim));
    }
    let s = expand_vars(raw_trim, env);
    let (exe, args) = if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"').unwrap_or(rest.len());
        let exe = rest[..end].trim().to_owned();
        let args = rest.get(end + 1..).unwrap_or_default().trim().to_owned();
        (exe, args)
    } else {
        split_unquoted(&s, exists)
    };
    if exe.is_empty() {
        return None;
    }
    let mut exe = PathBuf::from(exe);
    if exe.components().count() == 1 {
        // a bare name: a Windows tool (`MsiExec.exe`, `rundll32.exe`, `cmd.exe`)
        let mut name = exe.as_os_str().to_owned();
        if Path::new(&name).extension().is_none() {
            name.push(".exe");
        }
        let sys = system_root.join("System32").join(&name);
        if exists(&sys) {
            exe = sys;
        } else {
            let win = system_root.join(&name);
            if exists(&win) {
                exe = win;
            }
        }
    }
    Some(Command {
        exe,
        args,
        raw: raw_trim.to_owned(),
    })
}

/// `C:\Program Files\Acme Tool\uninst.exe /S` without quotes: the shortest prefix
/// ending a word that exists as a file (or ends in `.exe`) is the program.
fn split_unquoted(s: &str, exists: &dyn Fn(&Path) -> bool) -> (String, String) {
    let words: Vec<(usize, &str)> = s
        .match_indices(' ')
        .map(|(i, _)| (i, &s[..i]))
        .chain(std::iter::once((s.len(), s)))
        .collect();
    // an existing file first: `C:\Program Files\Acme\uninst.exe`
    for (end, prefix) in &words {
        let p = prefix.trim();
        if !p.is_empty() && Path::new(p).is_absolute() && exists(Path::new(p)) && has_ext(p) {
            return (p.to_owned(), s[*end..].trim().to_owned());
        }
    }
    // then the first prefix that looks like a program, existing or not (broken)
    for (end, prefix) in &words {
        let p = prefix.trim();
        let lower = p.to_ascii_lowercase();
        if [".exe", ".com", ".bat", ".cmd"]
            .iter()
            .any(|e| lower.ends_with(e))
        {
            return (p.to_owned(), s[*end..].trim().to_owned());
        }
    }
    let (exe, args) = s.split_once(' ').unwrap_or((s, ""));
    (exe.to_owned(), args.trim().to_owned())
}

fn has_ext(p: &str) -> bool {
    Path::new(p).extension().is_some()
}

// Windows paths, registry layouts and Store packages: the tests are Windows-only
// like the Programs view itself (hidden on macOS, docs/MACOS.md §4).
#[cfg(all(test, windows))]
mod tests {
    use super::*;

    const SYS: &str = r"C:\Windows";

    fn env(v: &str) -> Option<String> {
        match v.to_ascii_lowercase().as_str() {
            "programfiles" => Some(r"C:\Program Files".into()),
            "systemroot" | "windir" => Some(SYS.into()),
            "localappdata" => Some(r"C:\Users\nano\AppData\Local".into()),
            _ => None,
        }
    }

    fn on_disk(p: &Path) -> bool {
        let s = p.to_string_lossy().to_ascii_lowercase();
        [
            r"c:\program files\acme tool\uninst.exe",
            r"c:\program files\acme tool\uninstall tool.exe",
            r"c:\windows\system32\msiexec.exe",
            r"c:\windows\system32\rundll32.exe",
            r"c:\users\nano\appdata\local\discord\update.exe",
        ]
        .contains(&s.as_str())
    }

    fn split(raw: &str) -> (String, String) {
        let c = parse(raw, &env, &on_disk, Path::new(SYS)).expect("parsed");
        (c.exe.to_string_lossy().into_owned(), c.args)
    }

    #[test]
    fn quoted() {
        assert_eq!(
            split(r#""C:\Program Files\Acme Tool\uninst.exe" /S /lang=en"#),
            (
                r"C:\Program Files\Acme Tool\uninst.exe".into(),
                "/S /lang=en".into()
            )
        );
        assert_eq!(
            split(r#""C:\Program Files\Acme Tool\uninst.exe""#),
            (
                r"C:\Program Files\Acme Tool\uninst.exe".into(),
                String::new()
            )
        );
    }

    #[test]
    fn unquoted_with_spaces() {
        assert_eq!(
            split(r"C:\Program Files\Acme Tool\uninst.exe /S"),
            (r"C:\Program Files\Acme Tool\uninst.exe".into(), "/S".into())
        );
        assert_eq!(
            split(r"C:\Program Files\Acme Tool\Uninstall Tool.exe --remove"),
            (
                r"C:\Program Files\Acme Tool\Uninstall Tool.exe".into(),
                "--remove".into()
            )
        );
    }

    #[test]
    fn unquoted_missing_program_is_still_split() {
        assert_eq!(
            split(r"C:\Program Files\Gone App\unins000.exe /SILENT"),
            (
                r"C:\Program Files\Gone App\unins000.exe".into(),
                "/SILENT".into()
            )
        );
    }

    #[test]
    fn variables_expand() {
        assert_eq!(
            split(r#""%LOCALAPPDATA%\Discord\Update.exe" --uninstall"#),
            (
                r"C:\Users\nano\AppData\Local\Discord\Update.exe".into(),
                "--uninstall".into()
            )
        );
        assert_eq!(
            split(r"%ProgramFiles%\Acme Tool\uninst.exe"),
            (
                r"C:\Program Files\Acme Tool\uninst.exe".into(),
                String::new()
            )
        );
        assert_eq!(expand_vars("100% sure %NOPE%", &env), "100% sure %NOPE%");
    }

    #[test]
    fn msi_modify_becomes_uninstall() {
        let c = parse(
            "MsiExec.exe /I{23170F69-40C1-2702-2301-000001000000}",
            &env,
            &on_disk,
            Path::new(SYS),
        )
        .expect("msi");
        assert_eq!(c.exe, Path::new(SYS).join("System32").join("msiexec.exe"));
        assert_eq!(c.args, "/x {23170F69-40C1-2702-2301-000001000000}");
        assert_eq!(
            msi_product_code("msiexec /x {23170f69-40c1-2702-2301-000001000000} /qn"),
            Some("{23170F69-40C1-2702-2301-000001000000}".into())
        );
        assert_eq!(msi_product_code("MsiExec.exe /I{not-a-guid}"), None);
        assert_eq!(
            msi_product_code(r"C:\x\uninst.exe /I{23170F69-40C1-2702-2301-000001000000}"),
            None
        );
    }

    #[test]
    fn bare_windows_tools_resolve() {
        let (exe, args) = split(r"RunDll32 C:\PROGRA~1\Acme\setup.dll,Uninstall");
        assert_eq!(exe, r"C:\Windows\System32\RunDll32.exe");
        assert_eq!(args, r"C:\PROGRA~1\Acme\setup.dll,Uninstall");
    }

    #[test]
    fn empty_is_none() {
        assert!(parse("   ", &env, &on_disk, Path::new(SYS)).is_none());
        assert!(parse(r#""""#, &env, &on_disk, Path::new(SYS)).is_none());
    }

    #[test]
    fn command_lines_quote_the_program() {
        let c = Command {
            exe: PathBuf::from(r"C:\A B\u.exe"),
            args: "/S".into(),
            raw: String::new(),
        };
        assert_eq!(c.line(), r#""C:\A B\u.exe" /S"#);
        assert_eq!(c.file_name(), "u.exe");
    }
}
