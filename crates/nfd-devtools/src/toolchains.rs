//! Toolchain Inventory (PLAN §13.2 C2, basic): the SDKs, runtimes and editors a
//! developer machine collects — rustup toolchains, Node versions, Pythons, Conda
//! environments, JDKs, Go SDKs, Android SDK packages, .NET SDKs, Unity / Unreal
//! editors, Flutter versions, Visual Studio instances, WSL / Docker disks — with their
//! size, last write and **the proper way to remove them**.
//!
//! Read-only: nothing here deletes anything. Removing a toolchain by deleting its
//! folder breaks registrations, shims and PATH entries, so the UI only shows (and
//! copies) the tool's own uninstall command or names the app that manages it.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;

use crate::fsutil::{self, Totals};

/// Who removes a toolchain when there is no single command for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Guide {
    /// Windows Settings → Apps → Installed apps.
    WindowsApps,
    UnityHub,
    EpicLauncher,
    VisualStudioInstaller,
    /// The IDE that downloaded it (IntelliJ "Project Structure → SDKs" …).
    Ide,
    /// `wsl --shutdown`, then `Optimize-VHD` / `diskpart compact vdisk`.
    CompactVhd,
}

/// One installed toolchain / SDK / editor.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Toolchain {
    /// Tool family id (`rustup`, `nvm`, `android-sdk`, …).
    pub tool: &'static str,
    /// Display name of the family.
    pub family: &'static str,
    /// Version / name of this one.
    pub name: String,
    pub path: PathBuf,
    #[serde(flatten)]
    pub totals: Totals,
    /// The default / active one (rustup default, nvm current…).
    pub default: bool,
    /// The tool's own uninstall command, to show and copy (never run by the app).
    pub command: Option<String>,
    pub guide: Option<Guide>,
}

/// Where to look; built from the platform resolver and environment variables.
pub struct Sources<'a> {
    /// Rule-pack variables (`HOME`, `APPDATA`, `LOCALAPPDATA`, `PROGRAMFILES`, …).
    pub resolve: &'a dyn Fn(&str) -> Option<String>,
    /// Environment variables (`RUSTUP_HOME`, `NVM_HOME`, `ANDROID_HOME`, …).
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// `vswhere -all -format json` output, when available.
    pub vswhere: Option<String>,
}

impl std::fmt::Debug for Sources<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sources").finish_non_exhaustive()
    }
}

impl Sources<'_> {
    fn var(&self, name: &str) -> Option<PathBuf> {
        (self.resolve)(name).map(PathBuf::from)
    }
    fn env(&self, name: &str) -> Option<PathBuf> {
        (self.env)(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }
}

struct Found {
    tool: &'static str,
    family: &'static str,
    name: String,
    path: PathBuf,
    default: bool,
    command: Option<String>,
    guide: Option<Guide>,
}

fn dirs(parent: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = fsutil::subdirs(parent)
        .into_iter()
        .map(|n| {
            let p = parent.join(&n);
            (n, p)
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn quote(arg: &str) -> String {
    if arg.contains([' ', ';']) {
        format!("\"{arg}\"")
    } else {
        arg.to_owned()
    }
}

fn rustup(s: &Sources<'_>, out: &mut Vec<Found>) {
    let Some(home) = s
        .env("RUSTUP_HOME")
        .or_else(|| s.var("HOME").map(|h| h.join(".rustup")))
    else {
        return;
    };
    let default = std::fs::read_to_string(home.join("settings.toml"))
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.trim().strip_prefix("default_toolchain"))
                .map(|r| {
                    r.trim_start_matches([' ', '='])
                        .trim()
                        .trim_matches('"')
                        .to_owned()
                })
        });
    for (name, path) in dirs(&home.join("toolchains")) {
        out.push(Found {
            tool: "rustup",
            family: "Rust (rustup)",
            default: default.as_deref() == Some(name.as_str()),
            command: Some(format!("rustup toolchain uninstall {name}")),
            guide: None,
            name,
            path,
        });
    }
}

fn node(s: &Sources<'_>, out: &mut Vec<Found>) {
    // nvm-windows
    if let Some(nvm) = s
        .env("NVM_HOME")
        .or_else(|| s.var("APPDATA").map(|a| a.join("nvm")))
    {
        let current = s
            .env("NVM_SYMLINK")
            .and_then(|l| std::fs::read_link(l).ok())
            .and_then(|t| t.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        for (name, path) in dirs(&nvm) {
            let Some(ver) = name.strip_prefix('v') else {
                continue;
            };
            if !ver.starts_with(|c: char| c.is_ascii_digit()) {
                continue;
            }
            out.push(Found {
                tool: "nvm",
                family: "Node.js (nvm-windows)",
                default: current.as_deref() == Some(name.to_lowercase().as_str()),
                command: Some(format!("nvm uninstall {ver}")),
                guide: None,
                name: name.clone(),
                path,
            });
        }
    }
    // fnm
    let fnm = s
        .env("FNM_DIR")
        .or_else(|| s.var("APPDATA").map(|a| a.join("fnm")))
        .map(|d| d.join("node-versions"));
    if let Some(fnm) = fnm {
        for (name, path) in dirs(&fnm) {
            out.push(Found {
                tool: "fnm",
                family: "Node.js (fnm)",
                default: false,
                command: Some(format!("fnm uninstall {name}")),
                guide: None,
                name,
                path,
            });
        }
    }
    // Volta keeps node images; it has no uninstall for them
    if let Some(volta) = s
        .var("LOCALAPPDATA")
        .map(|l| l.join("Volta").join("tools").join("image").join("node"))
    {
        for (name, path) in dirs(&volta) {
            out.push(Found {
                tool: "volta",
                family: "Node.js (Volta)",
                default: false,
                command: None,
                guide: None,
                name,
                path,
            });
        }
    }
}

fn python(s: &Sources<'_>, out: &mut Vec<Found>) {
    if let Some(home) = s.var("HOME") {
        let pyenv = s
            .env("PYENV_ROOT")
            .unwrap_or_else(|| home.join(".pyenv").join("pyenv-win"));
        for (name, path) in dirs(&pyenv.join("versions")) {
            out.push(Found {
                tool: "pyenv",
                family: "Python (pyenv-win)",
                default: false,
                command: Some(format!("pyenv uninstall {name}")),
                guide: None,
                name,
                path,
            });
        }
        for base in ["miniconda3", "anaconda3", "miniforge3", "mambaforge"] {
            for (name, path) in dirs(&home.join(base).join("envs")) {
                out.push(Found {
                    tool: "conda",
                    family: "Conda environments",
                    default: false,
                    command: Some(format!("conda env remove -n {}", quote(&name))),
                    guide: None,
                    name,
                    path,
                });
            }
        }
    }
    if let Some(local) = s.var("LOCALAPPDATA") {
        for (name, path) in dirs(&local.join("Programs").join("Python")) {
            if name.to_lowercase().starts_with("python") {
                out.push(Found {
                    tool: "python",
                    family: "Python (python.org)",
                    default: false,
                    command: None,
                    guide: Some(Guide::WindowsApps),
                    name,
                    path,
                });
            }
        }
    }
    if let Some(roaming) = s.var("APPDATA") {
        for (name, path) in dirs(&roaming.join("uv").join("python")) {
            if name.starts_with('.') {
                continue;
            }
            out.push(Found {
                tool: "uv-python",
                family: "Python (uv)",
                default: false,
                command: Some(format!("uv python uninstall {name}")),
                guide: None,
                name,
                path,
            });
        }
    }
}

fn jvm_go_flutter(s: &Sources<'_>, out: &mut Vec<Found>) {
    let Some(home) = s.var("HOME") else { return };
    for (name, path) in dirs(&home.join(".jdks")) {
        out.push(Found {
            tool: "jdk",
            family: "JDKs (IDE downloads)",
            default: false,
            command: None,
            guide: Some(Guide::Ide),
            name,
            path,
        });
    }
    for (name, path) in dirs(&home.join("sdk")) {
        if name.starts_with("go") {
            out.push(Found {
                tool: "go",
                family: "Go SDKs (golang.org/dl)",
                default: false,
                command: None,
                guide: Some(Guide::Ide),
                name,
                path,
            });
        }
    }
    let fvm = s
        .env("FVM_CACHE_PATH")
        .map(|p| p.join("versions"))
        .unwrap_or_else(|| home.join("fvm").join("versions"));
    for (name, path) in dirs(&fvm) {
        out.push(Found {
            tool: "fvm",
            family: "Flutter (fvm)",
            default: false,
            command: Some(format!("fvm remove {name}")),
            guide: None,
            name,
            path,
        });
    }
}

fn android(s: &Sources<'_>, out: &mut Vec<Found>) {
    let Some(sdk) = s
        .env("ANDROID_HOME")
        .or_else(|| s.env("ANDROID_SDK_ROOT"))
        .or_else(|| s.var("LOCALAPPDATA").map(|l| l.join("Android").join("Sdk")))
    else {
        return;
    };
    let mut add = |kind: &str, name: String, path: PathBuf| {
        out.push(Found {
            tool: "android-sdk",
            family: "Android SDK",
            default: false,
            command: Some(format!(
                "sdkmanager --uninstall \"{kind};{}\"",
                name.replace('/', ";")
            )),
            guide: None,
            name: format!("{kind};{}", name.replace('/', ";")),
            path,
        });
    };
    for kind in ["platforms", "build-tools", "ndk"] {
        for (name, path) in dirs(&sdk.join(kind)) {
            add(kind, name, path);
        }
    }
    // system-images/<api>/<tag>/<abi>
    for (api, api_path) in dirs(&sdk.join("system-images")) {
        for (tag, tag_path) in dirs(&api_path) {
            for (abi, abi_path) in dirs(&tag_path) {
                add("system-images", format!("{api}/{tag}/{abi}"), abi_path);
            }
        }
    }
}

fn program_files(s: &Sources<'_>, out: &mut Vec<Found>) {
    let Some(pf) = s.var("PROGRAMFILES") else {
        return;
    };
    for (name, path) in dirs(&pf.join("dotnet").join("sdk")) {
        if name.starts_with(|c: char| c.is_ascii_digit()) {
            out.push(Found {
                tool: "dotnet-sdk",
                family: ".NET SDKs",
                default: false,
                command: Some(format!("dotnet-core-uninstall remove --sdk {name}")),
                guide: Some(Guide::WindowsApps),
                name,
                path,
            });
        }
    }
    for (name, path) in dirs(&pf.join("Unity").join("Hub").join("Editor")) {
        out.push(Found {
            tool: "unity",
            family: "Unity Editors",
            default: false,
            command: None,
            guide: Some(Guide::UnityHub),
            name,
            path,
        });
    }
    for (name, path) in dirs(&pf.join("Epic Games")) {
        if name.starts_with("UE_") {
            out.push(Found {
                tool: "unreal",
                family: "Unreal Engine",
                default: false,
                command: None,
                guide: Some(Guide::EpicLauncher),
                name,
                path,
            });
        }
    }
}

/// Instances from `vswhere -all -format json`.
fn visual_studio(s: &Sources<'_>, out: &mut Vec<Found>) {
    let Some(text) = &s.vswhere else { return };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(text)
    else {
        return;
    };
    for item in items {
        let get = |k: &str| item.get(k).and_then(|v| v.as_str()).map(str::to_owned);
        let (Some(path), Some(name)) = (get("installationPath"), get("displayName")) else {
            continue;
        };
        let version = get("installationVersion").unwrap_or_default();
        out.push(Found {
            tool: "visual-studio",
            family: "Visual Studio",
            default: false,
            command: None,
            guide: Some(Guide::VisualStudioInstaller),
            name: format!("{name} {version}").trim().to_owned(),
            path: PathBuf::from(path),
        });
    }
}

fn virtual_disks(s: &Sources<'_>, out: &mut Vec<Found>) {
    let Some(local) = s.var("LOCALAPPDATA") else {
        return;
    };
    let docker = local.join("Docker").join("wsl");
    for (sub, sub_path) in dirs(&docker) {
        for (name, path) in dirs(&sub_path) {
            let vhdx = path.join("ext4.vhdx");
            if vhdx.is_file() {
                out.push(Found {
                    tool: "wsl-disk",
                    family: "WSL / Docker disks",
                    default: false,
                    command: None,
                    guide: Some(Guide::CompactVhd),
                    name: format!("Docker {sub}/{name}"),
                    path: vhdx,
                });
            }
        }
        let vhdx = sub_path.join("ext4.vhdx");
        if vhdx.is_file() {
            out.push(Found {
                tool: "wsl-disk",
                family: "WSL / Docker disks",
                default: false,
                command: None,
                guide: Some(Guide::CompactVhd),
                name: format!("Docker {sub}"),
                path: vhdx,
            });
        }
    }
    for (name, path) in dirs(&local.join("Packages")) {
        let vhdx = path.join("LocalState").join("ext4.vhdx");
        if vhdx.is_file() {
            let short = name.split('_').next().unwrap_or(&name).to_owned();
            out.push(Found {
                tool: "wsl-disk",
                family: "WSL / Docker disks",
                default: false,
                command: None,
                guide: Some(Guide::CompactVhd),
                name: short,
                path: vhdx,
            });
        }
    }
}

/// Everything installed, largest family first; sizes walk at most `max_entries`
/// entries per toolchain.
#[must_use]
pub fn inventory(s: &Sources<'_>, max_entries: u64) -> Vec<Toolchain> {
    let mut found = Vec::new();
    rustup(s, &mut found);
    node(s, &mut found);
    python(s, &mut found);
    jvm_go_flutter(s, &mut found);
    android(s, &mut found);
    program_files(s, &mut found);
    visual_studio(s, &mut found);
    virtual_disks(s, &mut found);
    found
        .into_par_iter()
        .map(|f| Toolchain {
            totals: fsutil::totals(&f.path, max_entries),
            tool: f.tool,
            family: f.family,
            name: f.name,
            path: f.path,
            default: f.default,
            command: f.command,
            guide: f.guide,
        })
        .collect()
}

/// `vswhere.exe -all -format json -utf8`, when Visual Studio Installer is present.
#[must_use]
pub fn run_vswhere(resolve: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let exe = PathBuf::from(resolve("PROGRAMFILES_X86")?)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    if !exe.is_file() {
        return None;
    }
    let args: Vec<String> = ["-all", "-prerelease", "-format", "json", "-utf8"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    crate::process::run(&exe, &args, None, std::time::Duration::from_secs(20), None)
        .ok()
        .filter(crate::process::RunOutput::success)
        .map(|o| o.stdout)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn finds_toolchains_with_their_uninstall_command() {
        let dir = tempfile::tempdir().expect("tmp");
        let home = dir.path().join("home");
        let local = home.join("AppData/Local");
        let mk = |p: PathBuf| {
            fs::create_dir_all(&p).expect("mkdir");
            fs::write(p.join("f"), [0u8; 64]).expect("w");
        };
        mk(home.join(".rustup/toolchains/stable-x86_64-pc-windows-msvc"));
        mk(home.join(".rustup/toolchains/nightly-2024-01-01-x86_64-pc-windows-msvc"));
        fs::write(
            home.join(".rustup/settings.toml"),
            "version = \"12\"\ndefault_toolchain = \"stable-x86_64-pc-windows-msvc\"\n",
        )
        .expect("w");
        mk(local.join("Android/Sdk/platforms/android-33"));
        mk(local.join("Android/Sdk/system-images/android-33/google_apis/x86_64"));
        mk(home.join("miniconda3/envs/ml"));
        let h = home.to_string_lossy().replace('\\', "/");
        let resolve = move |v: &str| match v {
            "HOME" => Some(h.clone()),
            "LOCALAPPDATA" => Some(format!("{h}/AppData/Local")),
            "APPDATA" => Some(format!("{h}/AppData/Roaming")),
            _ => None,
        };
        let env = |_: &str| None;
        let s = Sources {
            resolve: &resolve,
            env: &env,
            vswhere: Some(
                r#"[{"displayName":"Visual Studio Community 2022","installationVersion":"17.11","installationPath":"C:/nope"}]"#
                    .into(),
            ),
        };
        let inv = inventory(&s, 1000);
        let rust: Vec<&Toolchain> = inv.iter().filter(|t| t.tool == "rustup").collect();
        assert_eq!(rust.len(), 2);
        let stable = rust
            .iter()
            .find(|t| t.name.starts_with("stable"))
            .expect("stable");
        assert!(stable.default);
        assert_eq!(
            stable.command.as_deref(),
            Some("rustup toolchain uninstall stable-x86_64-pc-windows-msvc")
        );
        assert_eq!(stable.totals.files, 1);
        let img = inv
            .iter()
            .find(|t| t.name.starts_with("system-images"))
            .expect("image");
        assert_eq!(
            img.command.as_deref(),
            Some("sdkmanager --uninstall \"system-images;android-33;google_apis;x86_64\"")
        );
        assert!(
            inv.iter()
                .any(|t| t.command.as_deref() == Some("conda env remove -n ml"))
        );
        let vs = inv.iter().find(|t| t.tool == "visual-studio").expect("vs");
        assert_eq!(vs.guide, Some(Guide::VisualStudioInstaller));
        assert!(vs.command.is_none());
    }
}
