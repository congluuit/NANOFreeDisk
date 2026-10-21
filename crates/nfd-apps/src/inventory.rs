//! Installed programs (`docs/UNINSTALLER.md` §2.1): the entries of the registry's
//! `Uninstall` keys (all users 64-bit and 32-bit, this user) and the Store / MSIX
//! packages, as "Apps & features" shows them — system components, updates and
//! hotfixes left out — with what the uninstaller and the leftover scan need.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::command::{self, Command};
use crate::names;
use crate::registry::{Hive, Registry, number, text};
use crate::{Scope, Source};

/// `…\CurrentVersion\Uninstall` (all users 64-bit, and this user).
pub const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
/// `…\CurrentVersion\Uninstall` of 32-bit programs (all users).
pub const UNINSTALL_WOW: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

/// A Store / MSIX package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub full_name: String,
    pub family: String,
}

/// One installed program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    /// `hklm:<key>`, `hklm32:<key>`, `hkcu:<key>` or `appx:<package full name>`.
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub source: Source,
    pub scope: Scope,
    /// `x64` / `x86` for registry entries.
    pub arch: Option<String>,
    pub install_location: Option<PathBuf>,
    /// The vendor's uninstall command (MSI: `msiexec /x {code}`).
    pub uninstall: Option<Command>,
    /// The vendor's silent uninstall command, when registered.
    pub quiet_uninstall: Option<String>,
    /// The program file of `DisplayIcon` (`,0` dropped).
    pub icon: Option<PathBuf>,
    /// Install date (Unix seconds): `InstallDate`, else the entry's last write.
    pub installed_at: Option<i64>,
    /// Bytes: `EstimatedSize`, or the install folder measured.
    pub size: u64,
    pub size_measured: bool,
    pub product_code: Option<String>,
    /// `HKEY_…\…\Uninstall\<key>` (registry entries).
    pub key: Option<String>,
    pub package: Option<Package>,
    pub url: Option<String>,
    /// Why it deserves care: `runtime` (other programs may need it), `driver`
    /// (hardware may stop working), `windows` (part of Windows' tooling).
    pub caution: Option<String>,
    /// Why it cannot be removed from here: `self`, `webview2`, `no-remove`,
    /// `no-uninstaller`, `non-removable`.
    pub blocked: Option<String>,
    /// The uninstaller is gone from the disk: only the leftovers can be removed.
    pub broken: bool,
}

impl Program {
    /// The registry hive and key path of a registry entry.
    #[must_use]
    pub fn reg_key(&self) -> Option<(Hive, &str)> {
        Hive::split(self.key.as_deref()?)
    }

    /// The folders the program lived in: install folder, uninstaller's folder,
    /// icon's folder (when they exist as absolute paths).
    #[must_use]
    pub fn homes(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        let mut add = |p: Option<&Path>| {
            if let Some(p) = p.filter(|p| p.is_absolute()) {
                let p = p.to_path_buf();
                if !out.iter().any(|o| same(o, &p)) {
                    out.push(p);
                }
            }
        };
        add(self.install_location.as_deref());
        if self
            .uninstall
            .as_ref()
            .is_some_and(|c| c.file_name() != "msiexec.exe")
        {
            add(self.uninstall.as_ref().and_then(|c| c.exe.parent()));
        }
        add(self.icon.as_deref().and_then(Path::parent));
        out
    }
}

/// A resolved folder (`C:/Users/x`, as rule-pack variables expand) in the
/// platform's own form (`C:\Users\x` on Windows), so shown paths do not mix
/// separators.
#[must_use]
pub fn native_path(s: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(s.replace('/', "\\"))
    } else {
        PathBuf::from(s)
    }
}

/// Case-insensitive path equality.
#[must_use]
pub fn same(a: &Path, b: &Path) -> bool {
    norm(a) == norm(b)
}

/// Lower-case `/`-separated form without a trailing separator.
#[must_use]
pub fn norm(p: &Path) -> String {
    let mut s = p.to_string_lossy().replace('\\', "/").to_lowercase();
    while s.ends_with('/') && s.len() > 3 {
        s.pop();
    }
    s
}

/// `inner` equals `outer` or lies inside it.
#[must_use]
pub fn within(inner: &Path, outer: &Path) -> bool {
    let (i, o) = (norm(inner), norm(outer));
    i == o || (i.len() > o.len() && i.starts_with(&o) && i.as_bytes()[o.len()] == b'/')
}

/// What the inventory needs from the machine besides the registry.
pub struct Env<'a> {
    pub env: &'a dyn Fn(&str) -> Option<String>,
    pub exists: &'a dyn Fn(&Path) -> bool,
    pub system_root: PathBuf,
    /// This app's own install folder (never offered for removal).
    pub own_dir: Option<PathBuf>,
}

impl std::fmt::Debug for Env<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Env")
            .field("system_root", &self.system_root)
            .field("own_dir", &self.own_dir)
            .finish_non_exhaustive()
    }
}

/// Unix seconds of `YYYYMMDD`.
#[must_use]
pub fn parse_install_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let y: i64 = s[..4].parse().ok()?;
    let m: i64 = s[4..6].parse().ok()?;
    let d: i64 = s[6..8].parse().ok()?;
    if !(1970..=2200).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // days from civil (Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400)
}

/// `"C:\x\app.exe",0` → `C:\x\app.exe`.
fn icon_path(s: &str) -> Option<PathBuf> {
    let s = s.trim().trim_matches('"');
    let s = match s.rsplit_once(',') {
        Some((p, idx))
            if idx
                .trim()
                .trim_start_matches('-')
                .chars()
                .all(|c| c.is_ascii_digit()) =>
        {
            p
        }
        _ => s,
    };
    let p = PathBuf::from(s.trim().trim_matches('"'));
    p.is_absolute().then_some(p)
}

/// Updates, hotfixes and service packs of other products.
fn is_update(key_name: &str, values: &[(String, crate::registry::Value)]) -> bool {
    if text(values, "ParentKeyName").is_some() || text(values, "ParentDisplayName").is_some() {
        return true;
    }
    if let Some(t) = text(values, "ReleaseType") {
        let t = t.to_ascii_lowercase();
        if [
            "update",
            "hotfix",
            "security update",
            "service pack",
            "servicepack",
        ]
        .contains(&t.as_str())
        {
            return true;
        }
    }
    let k = key_name.to_ascii_uppercase();
    k.starts_with("KB") && k[2..].chars().take(6).all(|c| c.is_ascii_digit()) && k.len() >= 8
}

/// Runtimes and drivers: removing them may break other programs or hardware.
#[must_use]
pub fn caution_of(name: &str, publisher: Option<&str>) -> Option<&'static str> {
    let n = name.to_lowercase();
    let p = publisher.unwrap_or_default().to_lowercase();
    const RUNTIME: &[&str] = &[
        "visual c++",
        "redistributable",
        ".net runtime",
        ".net framework",
        "desktop runtime",
        "asp.net core",
        "windows app runtime",
        "windows app sdk",
        "windows software development kit",
        "directx",
        "vcredist",
        "microsoft .net",
        "java(tm) se runtime",
        "java runtime",
        "openal",
        "physx",
        "xna framework",
        "visual studio tools for office runtime",
        "access database engine",
        "msxml",
        "sql server compact",
        "odbc driver",
    ];
    if RUNTIME.iter().any(|r| n.contains(r)) {
        return Some("runtime");
    }
    const HARDWARE: &[&str] = &[
        "intel",
        "nvidia",
        "amd",
        "advanced micro devices",
        "realtek",
        "qualcomm",
        "broadcom",
        "mediatek",
        "synaptics",
        "elan",
        "conexant",
        "dolby",
        "logitech",
        "razer",
        "asus",
        "lenovo",
        "dell",
        "hp",
        "hewlett",
        "acer",
        "msi",
        "micro-star",
        "gigabyte",
    ];
    const DRIVER_WORDS: &[&str] = &[
        "driver",
        "chipset",
        "graphics",
        "audio",
        "bluetooth",
        "wireless",
        "wi-fi",
        "wifi",
        "ethernet",
        "network",
        "management engine",
        "rapid storage",
        "serial io",
        "thunderbolt",
        "dynamic platform",
        "touchpad",
        "firmware",
        "physx",
        "control panel",
    ];
    let hw_vendor = HARDWARE
        .iter()
        .any(|h| p.split(|c: char| !c.is_alphanumeric()).any(|w| w == *h) || p.starts_with(h));
    if n.contains("driver") || (hw_vendor && DRIVER_WORDS.iter().any(|w| n.contains(w))) {
        return Some("driver");
    }
    const WINDOWS: &[&str] = &[
        "microsoft update health tools",
        "windows pc health check",
        "microsoft gameinput",
        "update for windows",
        "windows subsystem for linux",
        "microsoft edge update",
    ];
    if WINDOWS.iter().any(|w| n.contains(w)) {
        return Some("windows");
    }
    None
}

/// Programs this app never removes: itself, and the WebView2 Runtime it (and many
/// other programs) runs on.
fn blocked_of(name: &str, location: Option<&Path>, own_dir: Option<&Path>) -> Option<&'static str> {
    let n = names::compact(name);
    if n == "nanofreedisk" || location.zip(own_dir).is_some_and(|(l, o)| same(l, o)) {
        return Some("self");
    }
    if n.contains("webview2runtime") {
        return Some("webview2");
    }
    None
}

/// Read one `Uninstall` key into a [`Program`], or `None` when "Apps & features"
/// would not show it.
fn read_entry(
    reg: &dyn Registry,
    hive: Hive,
    base: &str,
    key_name: &str,
    env: &Env<'_>,
) -> Option<Program> {
    let path = format!(r"{base}\{key_name}");
    let values = reg.values(hive, &path)?;
    let name = text(&values, "DisplayName")?.to_owned();
    if number(&values, "SystemComponent") == Some(1) || is_update(key_name, &values) {
        return None;
    }
    let publisher = text(&values, "Publisher").map(str::to_owned);
    let install_location = text(&values, "InstallLocation")
        .map(|s| PathBuf::from(command::expand_vars(s.trim_matches('"'), env.env)))
        .filter(|p| p.is_absolute());
    let raw = text(&values, "UninstallString");
    let msi = number(&values, "WindowsInstaller") == Some(1);
    let product_code = command::msi_product_code(raw.unwrap_or_default()).or_else(|| {
        (msi && key_name.starts_with('{') && key_name.ends_with('}'))
            .then(|| key_name.to_ascii_uppercase())
    });
    let uninstall = match (&product_code, raw) {
        (Some(code), r) if msi || r.is_some_and(|r| r.to_ascii_lowercase().contains("msiexec")) => {
            Some(command::msi_uninstall(
                code,
                &env.system_root,
                r.unwrap_or_default(),
            ))
        }
        (_, Some(r)) => command::parse(r, env.env, env.exists, &env.system_root),
        _ => None,
    };
    let source = if product_code.is_some()
        && uninstall
            .as_ref()
            .is_some_and(|c| c.file_name() == "msiexec.exe")
    {
        Source::Msi
    } else {
        Source::Win32
    };
    let (scope, arch, prefix) = match (hive, base == UNINSTALL_WOW) {
        (Hive::CurrentUser, _) => (Scope::User, None, "hkcu"),
        (Hive::LocalMachine, true) => (Scope::Machine, Some("x86".to_owned()), "hklm32"),
        (Hive::LocalMachine, false) => (Scope::Machine, Some("x64".to_owned()), "hklm"),
    };
    let size = number(&values, "EstimatedSize").map_or(0, |kb| kb.saturating_mul(1024));
    let installed_at = text(&values, "InstallDate")
        .and_then(parse_install_date)
        .or_else(|| reg.last_write(hive, &path));
    let broken = uninstall.as_ref().is_some_and(|c| {
        c.exe.is_absolute() && c.file_name() != "msiexec.exe" && !(env.exists)(&c.exe)
    });
    let no_remove = number(&values, "NoRemove") == Some(1);
    let blocked = blocked_of(&name, install_location.as_deref(), env.own_dir.as_deref())
        .map(str::to_owned)
        .or_else(|| no_remove.then(|| "no-remove".to_owned()))
        .or_else(|| uninstall.is_none().then(|| "no-uninstaller".to_owned()));
    Some(Program {
        id: format!("{prefix}:{key_name}"),
        caution: caution_of(&name, publisher.as_deref()).map(str::to_owned),
        name,
        publisher,
        version: text(&values, "DisplayVersion").map(str::to_owned),
        source,
        scope,
        arch,
        install_location,
        quiet_uninstall: text(&values, "QuietUninstallString").map(str::to_owned),
        icon: text(&values, "DisplayIcon")
            .and_then(|s| icon_path(&command::expand_vars(s, env.env))),
        installed_at,
        size,
        size_measured: false,
        product_code,
        key: Some(format!(r"{}\{path}", hive.name())),
        package: None,
        url: text(&values, "URLInfoAbout")
            .or_else(|| text(&values, "HelpLink"))
            .map(str::to_owned),
        uninstall,
        blocked,
        broken,
    })
}

/// Every program of the registry's `Uninstall` keys, as "Apps & features" lists
/// them: duplicates between the 64-bit and 32-bit views merged.
#[must_use]
pub fn registry_programs(reg: &dyn Registry, env: &Env<'_>) -> Vec<Program> {
    let mut out: Vec<Program> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (hive, base) in [
        (Hive::LocalMachine, UNINSTALL),
        (Hive::LocalMachine, UNINSTALL_WOW),
        (Hive::CurrentUser, UNINSTALL),
    ] {
        for key in reg.subkeys(hive, base) {
            let Some(p) = read_entry(reg, hive, base, &key, env) else {
                continue;
            };
            // the same product registered twice (both views, or MSI + bootstrapper)
            let dedupe = format!(
                "{}|{}|{}",
                names::compact(&p.name),
                p.version.as_deref().unwrap_or_default(),
                p.product_code
                    .clone()
                    .or_else(|| p.uninstall.as_ref().map(|c| c.line().to_lowercase()))
                    .unwrap_or_default()
            );
            if seen.insert(dedupe) {
                out.push(p);
            }
        }
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// Fill `size` from the install folder for programs that registered none
/// (`EstimatedSize` missing), visiting at most `max_entries` entries per folder.
/// Shared folders (a root, or holding another program) are not measured.
pub fn measure_sizes(programs: &mut [Program], max_entries: u64, roots: &[PathBuf]) {
    let homes: Vec<(usize, PathBuf)> = programs
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.install_location.clone().map(|l| (i, l)))
        .collect();
    let jobs: Vec<(usize, PathBuf)> = homes
        .iter()
        .filter(|(i, l)| {
            programs[*i].size == 0
                && l.is_dir()
                && !roots.iter().any(|r| within(r, l))
                && !homes
                    .iter()
                    .any(|(j, o)| j != i && within(o, l) && !same(o, l))
        })
        .cloned()
        .collect();
    let results: Vec<(usize, u64)> = std::thread::scope(|s| {
        let chunks: Vec<Vec<(usize, PathBuf)>> = {
            let n = 4.min(jobs.len().max(1));
            let mut c = vec![Vec::new(); n];
            for (k, j) in jobs.into_iter().enumerate() {
                c[k % n].push(j);
            }
            c
        };
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                s.spawn(move || {
                    chunk
                        .into_iter()
                        .map(|(i, dir)| (i, crate::fsutil::totals(&dir, max_entries).bytes))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    for (i, bytes) in results {
        if bytes > 0 {
            programs[i].size = bytes;
            programs[i].size_measured = true;
        }
    }
}

/// A Store package as listed by `Get-AppxPackage`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct AppxRow {
    #[serde(rename = "n")]
    pub name: String,
    #[serde(rename = "f")]
    pub full_name: String,
    #[serde(rename = "fam")]
    pub family: String,
    #[serde(rename = "v", default)]
    pub version: String,
    #[serde(rename = "l", default)]
    pub location: Option<String>,
    #[serde(rename = "nr", default)]
    pub non_removable: bool,
    #[serde(rename = "s", default)]
    pub signature: String,
}

/// `<DisplayName>` and `<PublisherDisplayName>` of an `AppxManifest.xml`.
#[must_use]
pub fn manifest_names(xml: &str) -> (Option<String>, Option<String>) {
    let props = xml
        .find("<Properties>")
        .and_then(|s| xml[s..].find("</Properties>").map(|e| &xml[s..s + e]))
        .unwrap_or(xml);
    let tag = |t: &str| {
        let open = format!("<{t}>");
        let s = props.find(&open)? + open.len();
        let e = props[s..].find('<')?;
        let v = xml_unescape(props[s..s + e].trim());
        (!v.is_empty()).then_some(v)
    };
    (tag("DisplayName"), tag("PublisherDisplayName"))
}

fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// The `ms-resource:` references to try for a package's display name.
#[must_use]
pub fn resource_candidates(package_name: &str, full_name: &str, res: &str) -> Vec<String> {
    let r = res
        .trim()
        .strip_prefix("ms-resource:")
        .unwrap_or(res.trim());
    let uris = if r.starts_with("//") {
        vec![format!("ms-resource:{r}")]
    } else if let Some(abs) = r.strip_prefix('/') {
        vec![format!("ms-resource://{package_name}/{abs}")]
    } else {
        vec![
            format!("ms-resource://{package_name}/resources/{r}"),
            format!("ms-resource://{package_name}/{r}"),
        ]
    };
    uris.into_iter()
        .map(|u| format!("@{{{full_name}? {u}}}"))
        .collect()
}

/// A readable name from a package name (`Microsoft.WindowsCalculator` →
/// `Windows Calculator`).
#[must_use]
pub fn pretty_package_name(name: &str) -> String {
    let last = name.rsplit('.').next().unwrap_or(name);
    let mut out = String::new();
    let mut prev_lower = false;
    for c in last.chars() {
        if c.is_uppercase() && prev_lower {
            out.push(' ');
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        out.push(c);
    }
    out
}

/// Store packages → programs. `resolve` turns an `ms-resource:` reference into
/// text; `read` reads a manifest.
#[must_use]
pub fn store_programs(
    rows: Vec<AppxRow>,
    read: &dyn Fn(&Path) -> Option<String>,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Vec<Program> {
    let mut out = Vec::new();
    for row in rows {
        if row.signature.eq_ignore_ascii_case("system") {
            continue;
        }
        let location = row
            .location
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        let (display, publisher) = location
            .as_deref()
            .and_then(|l| read(&l.join("AppxManifest.xml")))
            .map(|x| manifest_names(&x))
            .unwrap_or_default();
        let resolve_text = |v: Option<String>| -> Option<String> {
            let v = v?;
            if v.starts_with("ms-resource:") {
                resource_candidates(&row.name, &row.full_name, &v)
                    .iter()
                    .find_map(|c| resolve(c))
                    .filter(|s| !s.trim().is_empty())
            } else {
                Some(v)
            }
        };
        let name = resolve_text(display).unwrap_or_else(|| pretty_package_name(&row.name));
        let publisher = resolve_text(publisher);
        out.push(Program {
            id: format!("appx:{}", row.full_name),
            caution: caution_of(&name, publisher.as_deref()).map(str::to_owned),
            blocked: row.non_removable.then(|| "non-removable".to_owned()),
            name,
            publisher,
            version: (!row.version.is_empty()).then_some(row.version),
            source: Source::Store,
            scope: Scope::User,
            arch: None,
            install_location: location,
            uninstall: None,
            quiet_uninstall: None,
            icon: None,
            installed_at: None,
            size: 0,
            size_measured: false,
            product_code: None,
            key: None,
            package: Some(Package {
                full_name: row.full_name,
                family: row.family,
            }),
            url: None,
            broken: false,
        });
    }
    out
}

/// Parse `Get-AppxPackage … | ConvertTo-Json` output (an object or an array).
#[must_use]
pub fn parse_appx_json(text: &str) -> Vec<AppxRow> {
    let t = text.trim();
    if t.starts_with('[') {
        serde_json::from_str(t).unwrap_or_default()
    } else if t.starts_with('{') {
        serde_json::from_str::<AppxRow>(t)
            .map(|r| vec![r])
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// The PowerShell script that lists Store packages for [`parse_appx_json`].
pub const APPX_SCRIPT: &str = "[Console]::OutputEncoding=[Text.Encoding]::UTF8;$ErrorActionPreference='SilentlyContinue';Get-AppxPackage -PackageTypeFilter Main | Where-Object { -not $_.IsFramework } | ForEach-Object { [pscustomobject]@{ n=$_.Name; f=$_.PackageFullName; fam=$_.PackageFamilyName; v=[string]$_.Version; l=$_.InstallLocation; nr=[bool]$_.NonRemovable; s=[string]$_.SignatureKind } } | ConvertTo-Json -Compress -Depth 2";

// Windows paths, registry layouts and Store packages: the tests are Windows-only
// like the Programs view itself (hidden on macOS, docs/MACOS.md §4).
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::registry::MemRegistry;

    fn env_var(v: &str) -> Option<String> {
        match v.to_ascii_lowercase().as_str() {
            "programfiles" => Some(r"C:\Program Files".into()),
            "systemroot" => Some(r"C:\Windows".into()),
            _ => None,
        }
    }

    fn exists(p: &Path) -> bool {
        let s = p.to_string_lossy().to_ascii_lowercase();
        s.starts_with(r"c:\program files\acme") || s.ends_with("msiexec.exe")
    }

    fn env() -> Env<'static> {
        Env {
            env: &env_var,
            exists: &exists,
            system_root: PathBuf::from(r"C:\Windows"),
            own_dir: Some(PathBuf::from(r"C:\Users\nano\AppData\Local\NANOFreeDisk")),
        }
    }

    fn sample() -> MemRegistry {
        let mut r = MemRegistry::new();
        let h = Hive::LocalMachine;
        let acme = format!(r"{UNINSTALL}\Acme Tool");
        r.sz(h, &acme, "DisplayName", "Acme Tool 2.1 (x64)")
            .sz(h, &acme, "Publisher", "Acme Software, Inc.")
            .sz(h, &acme, "DisplayVersion", "2.1.0")
            .sz(h, &acme, "InstallLocation", r"C:\Program Files\Acme Tool\")
            .sz(
                h,
                &acme,
                "UninstallString",
                r"C:\Program Files\Acme Tool\uninst.exe /lang=en",
            )
            .sz(
                h,
                &acme,
                "DisplayIcon",
                r#""C:\Program Files\Acme Tool\acme.exe",0"#,
            )
            .sz(h, &acme, "InstallDate", "20240215")
            .set(
                h,
                &acme,
                "EstimatedSize",
                crate::registry::Value::Dword(2048),
            );
        let msi = format!(r"{UNINSTALL_WOW}\{{23170F69-40C1-2702-2301-000001000000}}");
        r.sz(h, &msi, "DisplayName", "7-Zip 23.01")
            .sz(
                h,
                &msi,
                "UninstallString",
                "MsiExec.exe /I{23170F69-40C1-2702-2301-000001000000}",
            )
            .set(
                h,
                &msi,
                "WindowsInstaller",
                crate::registry::Value::Dword(1),
            );
        // the same product in the 64-bit view: merged
        let msi64 = format!(r"{UNINSTALL}\{{23170F69-40C1-2702-2301-000001000000}}");
        r.sz(h, &msi64, "DisplayName", "7-Zip 23.01").sz(
            h,
            &msi64,
            "UninstallString",
            "MsiExec.exe /X{23170F69-40C1-2702-2301-000001000000}",
        );
        // hidden ones
        let sys = format!(r"{UNINSTALL}\Hidden");
        r.sz(h, &sys, "DisplayName", "Hidden").set(
            h,
            &sys,
            "SystemComponent",
            crate::registry::Value::Dword(1),
        );
        let upd = format!(r"{UNINSTALL}\KB5034441");
        r.sz(
            h,
            &upd,
            "DisplayName",
            "Security Update for Windows (KB5034441)",
        );
        let child = format!(r"{UNINSTALL}\Acme Tool Patch");
        r.sz(h, &child, "DisplayName", "Acme Tool Patch 1").sz(
            h,
            &child,
            "ParentKeyName",
            "Acme Tool",
        );
        r.add_key(h, &format!(r"{UNINSTALL}\NoName"));
        // this user's: a broken one (uninstaller gone) and the app itself
        let gone = format!(r"{UNINSTALL}\GoneApp");
        r.sz(Hive::CurrentUser, &gone, "DisplayName", "Gone App")
            .sz(
                Hive::CurrentUser,
                &gone,
                "UninstallString",
                r#""C:\Users\nano\AppData\Local\Gone\unins000.exe""#,
            );
        let me = format!(r"{UNINSTALL}\NANOFreeDisk");
        r.sz(Hive::CurrentUser, &me, "DisplayName", "NANOFreeDisk")
            .sz(
                Hive::CurrentUser,
                &me,
                "UninstallString",
                r#""C:\Users\nano\AppData\Local\NANOFreeDisk\uninstall.exe""#,
            );
        let rt = format!(r"{UNINSTALL}\VC");
        r.sz(
            h,
            &rt,
            "DisplayName",
            "Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.38.33130",
        )
        .sz(h, &rt, "Publisher", "Microsoft Corporation")
        .sz(
            h,
            &rt,
            "UninstallString",
            r#""C:\Program Files\Acme Tool\vc.exe" /uninstall"#,
        );
        r
    }

    #[test]
    fn lists_what_apps_and_features_shows() {
        let list = registry_programs(&sample(), &env());
        let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "7-Zip 23.01",
                "Acme Tool 2.1 (x64)",
                "Gone App",
                "Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.38.33130",
                "NANOFreeDisk"
            ]
        );
    }

    #[test]
    fn reads_an_entry() {
        let list = registry_programs(&sample(), &env());
        let acme = list
            .iter()
            .find(|p| p.name.starts_with("Acme"))
            .expect("acme");
        assert_eq!(acme.id, "hklm:Acme Tool");
        assert_eq!(acme.scope, Scope::Machine);
        assert_eq!(acme.arch.as_deref(), Some("x64"));
        assert_eq!(acme.size, 2048 * 1024);
        assert_eq!(
            acme.install_location.as_deref(),
            Some(Path::new(r"C:\Program Files\Acme Tool\"))
        );
        assert_eq!(
            acme.icon.as_deref(),
            Some(Path::new(r"C:\Program Files\Acme Tool\acme.exe"))
        );
        assert_eq!(acme.installed_at, Some(1_707_955_200));
        let u = acme.uninstall.as_ref().expect("uninstall");
        assert_eq!(u.exe, Path::new(r"C:\Program Files\Acme Tool\uninst.exe"));
        assert_eq!(u.args, "/lang=en");
        assert!(!acme.broken && acme.blocked.is_none() && acme.caution.is_none());
        assert_eq!(
            acme.homes().len(),
            1,
            "install, uninstaller and icon share one folder"
        );
    }

    #[test]
    fn msi_entries_uninstall_with_msiexec() {
        let list = registry_programs(&sample(), &env());
        let zip = list
            .iter()
            .find(|p| p.name.starts_with("7-Zip"))
            .expect("7-zip");
        assert_eq!(zip.source, Source::Msi);
        assert_eq!(
            zip.product_code.as_deref(),
            Some("{23170F69-40C1-2702-2301-000001000000}")
        );
        assert_eq!(
            zip.uninstall.as_ref().map(|c| c.args.as_str()),
            Some("/x {23170F69-40C1-2702-2301-000001000000}")
        );
        assert_eq!(
            list.iter().filter(|p| p.name.starts_with("7-Zip")).count(),
            1
        );
    }

    #[test]
    fn flags() {
        let list = registry_programs(&sample(), &env());
        let get = |n: &str| list.iter().find(|p| p.name.starts_with(n)).expect(n);
        assert!(get("Gone App").broken);
        assert_eq!(get("Gone App").scope, Scope::User);
        assert_eq!(get("NANOFreeDisk").blocked.as_deref(), Some("self"));
        assert_eq!(
            get("Microsoft Visual C++").caution.as_deref(),
            Some("runtime")
        );
    }

    #[test]
    fn cautions() {
        assert_eq!(
            caution_of(
                "Intel(R) Management Engine Components",
                Some("Intel Corporation")
            ),
            Some("driver")
        );
        assert_eq!(
            caution_of("NVIDIA Graphics Driver 551.23", Some("NVIDIA Corporation")),
            Some("driver")
        );
        assert_eq!(
            caution_of("Realtek Audio Console", Some("Realtek")),
            Some("driver")
        );
        assert_eq!(
            caution_of(
                "Microsoft Windows Desktop Runtime - 8.0.1 (x64)",
                Some("Microsoft Corporation")
            ),
            Some("runtime")
        );
        assert_eq!(caution_of("Steam", Some("Valve Corporation")), None);
        assert_eq!(caution_of("Intel Unison", Some("Intel")), None);
        assert_eq!(
            blocked_of("Microsoft Edge WebView2 Runtime", None, None),
            Some("webview2")
        );
    }

    #[test]
    fn install_dates() {
        assert_eq!(parse_install_date("19700101"), Some(0));
        assert_eq!(parse_install_date("20240215"), Some(1_707_955_200));
        assert_eq!(parse_install_date("2024-02-15"), None);
        assert_eq!(parse_install_date("20241315"), None);
    }

    #[test]
    fn store_packages() {
        let json = r#"[{"n":"Microsoft.WindowsCalculator","f":"Microsoft.WindowsCalculator_11.2401.0.0_x64__8wekyb3d8bbwe","fam":"Microsoft.WindowsCalculator_8wekyb3d8bbwe","v":"11.2401.0.0","l":"C:\\Program Files\\WindowsApps\\calc","nr":false,"s":"Store"},
            {"n":"Microsoft.Windows.ShellExperienceHost","f":"x","fam":"y","v":"1","l":null,"nr":true,"s":"System"},
            {"n":"Acme.Notes","f":"Acme.Notes_1.0.0.0_x64__abc","fam":"Acme.Notes_abc","v":"1.0.0.0","l":"C:\\Program Files\\WindowsApps\\notes","nr":true,"s":"Developer"}]"#;
        let rows = parse_appx_json(json);
        assert_eq!(rows.len(), 3);
        let manifest = |p: &Path| -> Option<String> {
            p.to_string_lossy().contains("calc").then(|| {
                "<Package><Properties><DisplayName>ms-resource:AppStoreName</DisplayName><PublisherDisplayName>Microsoft Corporation</PublisherDisplayName></Properties></Package>".to_owned()
            })
        };
        let resolve = |r: &str| -> Option<String> {
            r.contains("ms-resource://Microsoft.WindowsCalculator/resources/AppStoreName")
                .then(|| "Calculator".to_owned())
        };
        let list = store_programs(rows, &manifest, &resolve);
        assert_eq!(list.len(), 2, "system packages are not listed");
        assert_eq!(list[0].name, "Calculator");
        assert_eq!(list[0].publisher.as_deref(), Some("Microsoft Corporation"));
        assert_eq!(list[0].source, Source::Store);
        assert_eq!(list[1].name, "Notes");
        assert_eq!(list[1].blocked.as_deref(), Some("non-removable"));
        assert_eq!(parse_appx_json(r#"{"n":"A","f":"B","fam":"C"}"#).len(), 1);
        assert!(parse_appx_json("").is_empty());
        assert_eq!(
            pretty_package_name("Microsoft.WindowsCalculator"),
            "Windows Calculator"
        );
    }

    #[test]
    fn manifest_display_names_unescape() {
        let (n, p) = manifest_names(
            "<Properties><DisplayName>Tom &amp; Jerry</DisplayName><PublisherDisplayName> Acme </PublisherDisplayName></Properties>",
        );
        assert_eq!(n.as_deref(), Some("Tom & Jerry"));
        assert_eq!(p.as_deref(), Some("Acme"));
    }
}
