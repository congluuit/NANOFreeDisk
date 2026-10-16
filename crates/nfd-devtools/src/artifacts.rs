//! Build artifact policy (PLAN §13.2 C9, DEVTOOL_CACHE_CATALOG §4): which folders
//! inside a project are build output, decided by the project's **marker files**
//! (`Cargo.toml` → `target/`, `package.json` → `node_modules/`, `*.uproject` →
//! `Intermediate/`, `ProjectSettings/ProjectVersion.txt` → Unity `Library/`, …).
//!
//! Generic names (`build`, `dist`, `out`, `bin`, `obj`) are only build output when git
//! says they are ignored — [`Dir::generic`]; the repo scanner checks that, and never
//! proposes a folder that holds tracked files.

use nfd_core::RiskLevel;
use serde::Serialize;

/// Rust `target/` above this size is 🟡: rebuilding it takes long.
pub const LARGE_TARGET: u64 = 5 << 30;

/// How a project type is recognised in a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// A file with this name (case-insensitive).
    File(&'static str),
    /// A file with this extension (`.uproject`).
    Ext(&'static str),
    /// A file at this relative path (`ProjectSettings/ProjectVersion.txt`).
    Path(&'static str),
}

/// A build-output folder of a project type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dir {
    /// Relative path (`target`, `Saved/Logs`), or a prefix ending in `*`
    /// (`cmake-build-*`), or `*.egg-info`.
    pub name: &'static str,
    pub level: RiskLevel,
    /// Only build output when git ignores it.
    pub generic: bool,
    /// Only build output when this file is inside (`pyvenv.cfg`).
    pub needs: Option<&'static str>,
}

const fn safe(name: &'static str) -> Dir {
    Dir {
        name,
        level: RiskLevel::Safe,
        generic: false,
        needs: None,
    }
}

const fn caution(name: &'static str) -> Dir {
    Dir {
        name,
        level: RiskLevel::Caution,
        generic: false,
        needs: None,
    }
}

const fn generic(name: &'static str) -> Dir {
    Dir {
        name,
        level: RiskLevel::Safe,
        generic: true,
        needs: None,
    }
}

const fn venv(name: &'static str) -> Dir {
    Dir {
        name,
        level: RiskLevel::Caution,
        generic: false,
        needs: Some("pyvenv.cfg"),
    }
}

/// One project type.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// `rust`, `node`, `unity`, … (also the i18n key suffix in the UI).
    pub kind: &'static str,
    pub markers: &'static [Marker],
    pub dirs: &'static [Dir],
}

/// The policy table (DEVTOOL_CACHE_CATALOG §4 and §4.1).
pub const POLICIES: &[Policy] = &[
    Policy {
        kind: "rust",
        markers: &[Marker::File("Cargo.toml")],
        dirs: &[safe("target")],
    },
    Policy {
        kind: "node",
        markers: &[Marker::File("package.json")],
        dirs: &[
            safe("node_modules"),
            safe(".next"),
            safe(".nuxt"),
            safe(".turbo"),
            safe(".svelte-kit"),
            safe(".parcel-cache"),
            safe(".vite"),
            safe(".angular"),
            safe(".expo"),
            generic("dist"),
            generic("build"),
            generic("out"),
            generic(".cache"),
        ],
    },
    Policy {
        kind: "python",
        markers: &[
            Marker::File("pyproject.toml"),
            Marker::File("requirements.txt"),
            Marker::File("setup.py"),
            Marker::File("setup.cfg"),
        ],
        dirs: &[
            safe(".tox"),
            safe(".nox"),
            safe("*.egg-info"),
            caution(".venv"),
            venv("venv"),
            venv("env"),
            generic("build"),
            generic("dist"),
        ],
    },
    Policy {
        kind: "dotnet",
        markers: &[
            Marker::Ext(".sln"),
            Marker::Ext(".csproj"),
            Marker::Ext(".fsproj"),
            Marker::Ext(".vbproj"),
        ],
        dirs: &[safe(".vs"), generic("bin"), generic("obj")],
    },
    Policy {
        kind: "cmake",
        markers: &[Marker::File("CMakeLists.txt")],
        dirs: &[safe("cmake-build-*"), generic("out"), generic("build")],
    },
    Policy {
        kind: "gradle",
        markers: &[
            Marker::File("build.gradle"),
            Marker::File("build.gradle.kts"),
            Marker::File("settings.gradle"),
            Marker::File("settings.gradle.kts"),
        ],
        dirs: &[safe(".gradle"), generic("build")],
    },
    Policy {
        kind: "maven",
        markers: &[Marker::File("pom.xml")],
        dirs: &[safe("target")],
    },
    Policy {
        kind: "flutter",
        markers: &[Marker::File("pubspec.yaml")],
        dirs: &[safe(".dart_tool"), generic("build")],
    },
    Policy {
        kind: "unity",
        markers: &[Marker::Path("ProjectSettings/ProjectVersion.txt")],
        dirs: &[caution("Library"), safe("Temp"), safe("Logs"), safe("obj")],
    },
    Policy {
        kind: "unreal",
        markers: &[Marker::Ext(".uproject")],
        dirs: &[
            safe("Intermediate"),
            safe("Saved/Logs"),
            safe("Saved/Crashes"),
            caution("DerivedDataCache"),
            caution("Binaries"),
        ],
    },
    Policy {
        kind: "godot",
        markers: &[Marker::File("project.godot")],
        dirs: &[safe(".godot")],
    },
    Policy {
        kind: "terraform",
        markers: &[Marker::File(".terraform.lock.hcl")],
        dirs: &[caution(".terraform")],
    },
    Policy {
        kind: "ruby",
        markers: &[Marker::File("Gemfile")],
        dirs: &[caution("vendor/bundle"), safe("tmp/cache")],
    },
];

/// Folder names that are build output wherever they appear (no marker needed).
pub const ANYWHERE: &[&str] = &["__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache"];

/// Folders never descended into while looking for projects (lower case).
pub const NEVER_DESCEND: &[&str] = &[
    ".git",
    "node_modules",
    "$recycle.bin",
    "system volume information",
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "appdata",
];

/// A build-output folder found in one directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    /// Path relative to the directory (`target`, `Saved/Logs`, `foo.egg-info`).
    pub rel: String,
    pub kind: &'static str,
    pub level: RiskLevel,
    pub generic: bool,
}

fn has_marker(m: Marker, files: &[String], exists: &dyn Fn(&str) -> bool) -> bool {
    match m {
        Marker::File(name) => files.iter().any(|f| f.eq_ignore_ascii_case(name)),
        Marker::Ext(ext) => files
            .iter()
            .any(|f| f.len() > ext.len() && f.to_lowercase().ends_with(ext)),
        Marker::Path(p) => exists(p),
    }
}

/// Project types whose markers are in a directory with these `files` (names) —
/// `exists` checks a relative path inside it.
#[must_use]
pub fn kinds(files: &[String], exists: &dyn Fn(&str) -> bool) -> Vec<&'static Policy> {
    POLICIES
        .iter()
        .filter(|p| p.markers.iter().any(|m| has_marker(*m, files, exists)))
        .collect()
}

fn name_matches(pattern: &str, name: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        name.len() > prefix.len() && name.to_lowercase().starts_with(&prefix.to_lowercase())
    } else if let Some(suffix) = pattern.strip_prefix('*') {
        name.len() > suffix.len() && name.to_lowercase().ends_with(&suffix.to_lowercase())
    } else {
        name.eq_ignore_ascii_case(pattern)
    }
}

/// Build-output folders of a directory: `files` / `dirs` are the names of its files
/// and sub-folders (links left out), `exists` checks a relative path inside it.
#[must_use]
pub fn sites(files: &[String], dirs: &[String], exists: &dyn Fn(&str) -> bool) -> Vec<Site> {
    let mut out: Vec<Site> = Vec::new();
    for policy in kinds(files, exists) {
        for d in policy.dirs {
            let found: Vec<String> = if d.name.contains('/') {
                if exists(d.name) {
                    vec![d.name.to_owned()]
                } else {
                    Vec::new()
                }
            } else {
                dirs.iter()
                    .filter(|n| name_matches(d.name, n))
                    .cloned()
                    .collect()
            };
            for rel in found {
                if d.needs.is_some_and(|f| !exists(&format!("{rel}/{f}"))) {
                    continue;
                }
                // two project types claiming one folder: the stricter one wins
                if let Some(prev) = out.iter_mut().find(|s| s.rel.eq_ignore_ascii_case(&rel)) {
                    if d.level > prev.level || (prev.generic && !d.generic) {
                        prev.level = prev.level.max(d.level);
                        prev.generic = prev.generic && d.generic;
                    }
                    continue;
                }
                out.push(Site {
                    rel,
                    kind: policy.kind,
                    level: d.level,
                    generic: d.generic,
                });
            }
        }
    }
    for n in dirs {
        if ANYWHERE.iter().any(|a| a.eq_ignore_ascii_case(n))
            && !out.iter().any(|s| s.rel.eq_ignore_ascii_case(n))
        {
            out.push(Site {
                rel: n.clone(),
                kind: "python",
                level: RiskLevel::Safe,
                generic: false,
            });
        }
    }
    out
}

/// The level of a site once its size is known (large Rust targets are 🟡).
#[must_use]
pub fn level_for(site: &Site, bytes: u64) -> RiskLevel {
    if site.kind == "rust" && site.rel.eq_ignore_ascii_case("target") && bytes > LARGE_TARGET {
        RiskLevel::Caution
    } else {
        site.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn markers_select_the_build_folders() {
        let none = |_: &str| false;
        let got = sites(
            &s(&["Cargo.toml", "README.md"]),
            &s(&["src", "target", "node_modules"]),
            &none,
        );
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].rel, "target");
        assert_eq!(got[0].kind, "rust");
        // node_modules without package.json is not proposed here
        let got = sites(
            &s(&["package.json"]),
            &s(&["node_modules", "dist", "src"]),
            &none,
        );
        let names: Vec<&str> = got.iter().map(|x| x.rel.as_str()).collect();
        assert_eq!(names, ["node_modules", "dist"]);
        assert!(got[1].generic);
    }

    #[test]
    fn unity_and_unreal() {
        let unity = |p: &str| p == "ProjectSettings/ProjectVersion.txt";
        let got = sites(
            &[],
            &s(&["Assets", "Library", "Temp", "ProjectSettings"]),
            &unity,
        );
        let lib = got.iter().find(|x| x.rel == "Library").expect("library");
        assert_eq!(lib.level, RiskLevel::Caution);
        assert_eq!(lib.kind, "unity");
        let unreal = |p: &str| p == "Saved/Logs";
        let got = sites(
            &s(&["Game.uproject"]),
            &s(&["Content", "Intermediate", "Binaries", "Saved"]),
            &unreal,
        );
        let names: Vec<&str> = got.iter().map(|x| x.rel.as_str()).collect();
        assert_eq!(names, ["Intermediate", "Saved/Logs", "Binaries"]);
        assert_eq!(got[2].level, RiskLevel::Caution);
    }

    #[test]
    fn venv_needs_its_config_and_pycache_is_anywhere() {
        let cfg = |p: &str| p == "venv/pyvenv.cfg";
        let got = sites(
            &s(&["requirements.txt"]),
            &s(&["venv", "env", "__pycache__"]),
            &cfg,
        );
        let names: Vec<&str> = got.iter().map(|x| x.rel.as_str()).collect();
        assert_eq!(names, ["venv", "__pycache__"]);
        let got = sites(&[], &s(&["__pycache__", "cmake-build-debug"]), &|_| false);
        assert_eq!(got.len(), 1);
        let got = sites(&s(&["CMakeLists.txt"]), &s(&["cmake-build-debug"]), &|_| {
            false
        });
        assert_eq!(got[0].rel, "cmake-build-debug");
    }

    #[test]
    fn large_rust_targets_need_care() {
        let site = Site {
            rel: "target".into(),
            kind: "rust",
            level: RiskLevel::Safe,
            generic: false,
        };
        assert_eq!(level_for(&site, 1 << 20), RiskLevel::Safe);
        assert_eq!(level_for(&site, LARGE_TARGET + 1), RiskLevel::Caution);
    }
}
