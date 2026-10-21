//! Command line, compatible with NSIS installers so scripts can switch easily:
//! `/S` silent, `/D=<dir>` install folder (last, may contain spaces, no quotes),
//! `/ALLUSERS` for everyone (needs admin), `/NOSHORTCUTS`, `/NOSTARTMENU`, `/DESKTOP`,
//! `/SHELLMENU` (Explorer menu), `/LANG=en|vi`; `--uninstall [/S] [/REMOVEDATA]`.
//! In-app update: `--update [--wait-pid=N] [--relaunch] /D=<install dir>` installs
//! over the copy in that folder with the options it was installed with, silently.

use std::path::PathBuf;

use crate::text::Lang;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    pub silent: bool,
    pub uninstall: bool,
    pub all_users: bool,
    pub dir: Option<PathBuf>,
    pub start_menu: bool,
    pub desktop: bool,
    pub shell_menu: bool,
    pub remove_data: bool,
    pub lang: Option<Lang>,
    /// Started elevated by the wizard for an all-users install: go straight to the
    /// progress page with these options.
    pub elevated: bool,
    /// Started by the app's "Update now": silent, keeps the installed options.
    pub update: bool,
    /// The app process to wait for before files are replaced.
    pub wait_pid: Option<u32>,
    /// Start the app again once the update is done.
    pub relaunch: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            silent: false,
            uninstall: false,
            all_users: false,
            dir: None,
            start_menu: true,
            desktop: false,
            shell_menu: false,
            remove_data: false,
            lang: None,
            elevated: false,
            update: false,
            wait_pid: None,
            relaunch: false,
        }
    }
}

/// Parse `args` (without the program name).
#[must_use]
pub fn parse(args: &[String]) -> Args {
    let mut out = Args::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let upper = a.to_ascii_uppercase();
        if let Some(rest) = a.strip_prefix("/D=").or_else(|| a.strip_prefix("/d=")) {
            // NSIS rule: everything after /D= is the folder, spaces included
            let mut dir = rest.to_owned();
            for more in &args[i + 1..] {
                dir.push(' ');
                dir.push_str(more);
            }
            let dir = dir.trim().trim_matches('"').to_owned();
            if !dir.is_empty() {
                out.dir = Some(PathBuf::from(dir));
            }
            break;
        }
        match upper.as_str() {
            "/S" | "--SILENT" => out.silent = true,
            "--UNINSTALL" => out.uninstall = true,
            "/ALLUSERS" | "--ALL-USERS" => out.all_users = true,
            "/CURRENTUSER" => out.all_users = false,
            "/NOSHORTCUTS" => {
                out.start_menu = false;
                out.desktop = false;
            }
            "/NOSTARTMENU" => out.start_menu = false,
            "/DESKTOP" => out.desktop = true,
            "/SHELLMENU" => out.shell_menu = true,
            "/REMOVEDATA" | "--REMOVE-DATA" => out.remove_data = true,
            "--ELEVATED" => out.elevated = true,
            "--UPDATE" => out.update = true,
            "--RELAUNCH" => out.relaunch = true,
            _ => {
                if let Some(code) = upper.strip_prefix("/LANG=") {
                    out.lang = Lang::from_code(code);
                } else if let Some(pid) = upper.strip_prefix("--WAIT-PID=") {
                    out.wait_pid = pid.parse().ok();
                }
            }
        }
        i += 1;
    }
    out
}

/// The arguments that reproduce `a` for the elevated copy of the setup.
#[must_use]
pub fn to_command_line(a: &Args) -> Vec<String> {
    let mut v = vec!["--elevated".to_owned()];
    if a.all_users {
        v.push("/ALLUSERS".into());
    }
    if !a.start_menu {
        v.push("/NOSTARTMENU".into());
    }
    if a.desktop {
        v.push("/DESKTOP".into());
    }
    if a.shell_menu {
        v.push("/SHELLMENU".into());
    }
    if let Some(l) = a.lang {
        v.push(format!("/LANG={}", l.code()));
    }
    if let Some(d) = &a.dir {
        v.push(format!("/D={}", d.display()));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(list: &[&str]) -> Args {
        parse(&list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn nsis_style_flags() {
        let a = p(&[
            "/S",
            "/ALLUSERS",
            "/SHELLMENU",
            r"/D=C:\Program",
            r"Files\NANO FreeDisk",
        ]);
        assert!(a.silent && a.all_users && a.shell_menu);
        assert_eq!(
            a.dir,
            Some(PathBuf::from(r"C:\Program Files\NANO FreeDisk"))
        );
        assert!(a.start_menu && !a.desktop);

        let a = p(&["--uninstall", "/S", "/REMOVEDATA"]);
        assert!(a.uninstall && a.silent && a.remove_data);

        let a = p(&[
            "--update",
            "--wait-pid=4242",
            "--relaunch",
            "/LANG=en",
            r"/D=D:\Apps\NANOFreeDisk",
        ]);
        assert!(a.update && a.relaunch && !a.silent);
        assert_eq!(a.wait_pid, Some(4242));
        assert_eq!(a.lang, Some(Lang::En));
        assert_eq!(a.dir, Some(PathBuf::from(r"D:\Apps\NANOFreeDisk")));

        let a = p(&["/s", "/NOSHORTCUTS", "/LANG=vi"]);
        assert!(a.silent && !a.start_menu && !a.desktop);
        assert_eq!(a.lang, Some(Lang::Vi));
        assert_eq!(a.dir, None);
    }

    #[test]
    fn elevated_round_trip() {
        let a = Args {
            all_users: true,
            desktop: true,
            shell_menu: true,
            lang: Some(Lang::Vi),
            dir: Some(PathBuf::from(r"C:\Program Files\NANOFreeDisk")),
            ..Args::default()
        };
        let back = parse(&to_command_line(&a));
        assert!(back.elevated);
        assert_eq!(
            Args {
                elevated: false,
                ..back
            },
            a
        );
    }
}
