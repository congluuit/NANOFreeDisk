//! Program icons for the Programs list (`docs/UNINSTALLER.md` §2.7): the vendor's
//! `DisplayIcon`, else the Windows Installer's product icon, else the program file
//! of the install folder that carries the program's name; Store apps the logo
//! their manifest names. Read-only: Windows' icon extractor opens a program file
//! as data (it is never run), and only local drives are read — never a network
//! share, which could hang the list or hand the user's sign-in to a server.

use std::path::{Path, PathBuf};

use crate::inventory::{self, Program};
use crate::names;
use crate::registry::{Hive, Registry, text};

/// Edge of the icons handed to the UI, in pixels (sharp at 34 px on 150 % screens).
pub const ICON_SIZE: u32 = 48;
/// Logo files bigger than this are not read.
pub const MAX_LOGO_BYTES: u64 = 512 * 1024;

/// Where an icon is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconSource {
    /// An icon resource of an `.exe`, `.dll` or `.ico` (index, or resource id
    /// when negative).
    Resource(PathBuf, i32),
    /// A PNG logo; `plate` = drawn for a colored plate (often a white glyph), not
    /// for a bare background.
    Png { path: PathBuf, plate: bool },
}

/// One program's icon, as PNG bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramIcon {
    pub png: Vec<u8>,
    /// Show it on a colored plate (Store logos made for one).
    pub plate: bool,
}

/// Files Windows' icon extractor reads.
const ICON_FILES: [&str; 7] = ["exe", "dll", "ico", "icl", "ocx", "cpl", "mun"];

fn ext_of(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// `p` is on one of `drives` (`C:\`, compared case-insensitively) and is not a
/// network or device path.
#[must_use]
pub fn is_local(p: &Path, drives: &[String]) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    !s.starts_with(r"\\")
        && !s.starts_with("//")
        && drives.iter().any(|d| s.starts_with(&d.to_lowercase()))
}

/// The Windows Installer's packed form of a product code, the name of its key
/// under `Installer\Products`
/// (`{12345678-90AB-CDEF-1234-567890ABCDEF}` → `87654321BA09FEDC2143658709BADCFE`).
#[must_use]
pub fn packed_guid(code: &str) -> Option<String> {
    let t = code.trim().trim_start_matches('{').trim_end_matches('}');
    let parts: Vec<&str> = t.split('-').collect();
    let lens: Vec<usize> = parts.iter().map(|p| p.len()).collect();
    if lens != [8, 4, 4, 4, 12] || !t.chars().all(|c| c == '-' || c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = String::with_capacity(32);
    for part in &parts[..3] {
        out.extend(part.chars().rev());
    }
    let tail: Vec<char> = format!("{}{}", parts[3], parts[4]).chars().collect();
    for pair in tail.chunks(2) {
        out.push(pair[1]);
        out.push(pair[0]);
    }
    Some(out.to_uppercase())
}

/// Folder names that only qualify a logo (`scale-200`, `targetsize-48`, …).
fn is_qualifier_dir(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["scale-", "targetsize-", "contrast-standard", "theme-"]
        .iter()
        .any(|q| n.starts_with(q))
}

/// The best file on disk for the logo `base` (`Assets\Logo.png`) among
/// `candidates`: the manifest names the logo without the qualifiers its files
/// carry (`Logo.targetsize-48_altform-unplated.png`, `scale-200\Logo.png`).
/// Unplated (made for a bare background) first, then the size closest to
/// [`ICON_SIZE`]; high-contrast variants never. Returns the file and whether it
/// was made for a plate.
#[must_use]
pub fn best_logo(base: &Path, candidates: &[PathBuf]) -> Option<(PathBuf, bool)> {
    let stem = base.file_stem()?.to_string_lossy().to_lowercase();
    let ext = ext_of(base);
    let dir = base.parent()?;
    let mut best: Option<((u8, u8, u32), PathBuf, bool)> = None;
    for c in candidates {
        let Some(name) = c.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
            continue;
        };
        let Some(middle) = name
            .strip_suffix(&format!(".{ext}"))
            .and_then(|n| n.strip_prefix(&stem))
        else {
            continue;
        };
        if !(middle.is_empty() || middle.starts_with('.')) {
            continue; // `LogoBig.png` is another logo
        }
        let mut quals = middle.trim_start_matches('.').to_owned();
        if let Some(parent) = c.parent().filter(|p| *p != dir) {
            match parent.file_name().map(|n| n.to_string_lossy().into_owned()) {
                Some(n) if parent.parent() == Some(dir) && is_qualifier_dir(&n) => {
                    quals.push('_');
                    quals.push_str(&n.to_lowercase());
                }
                _ => continue,
            }
        }
        let (mut size, mut plated, mut light) = (None, true, false);
        let mut usable = true;
        for tok in quals.split(['_', '.']).filter(|t| !t.is_empty()) {
            if let Some(v) = tok.strip_prefix("targetsize-") {
                size = v.parse::<u32>().ok();
            } else if let Some(v) = tok.strip_prefix("scale-") {
                // the app list logo is 44 px at 100 %
                size = size.or_else(|| v.parse::<u32>().ok().map(|s| s * 44 / 100));
            } else if tok == "altform-unplated" {
                plated = false;
            } else if tok == "altform-lightunplated" {
                plated = false;
                light = true;
            } else if tok == "theme-light" {
                light = true;
            } else if tok.starts_with("contrast-") && tok != "contrast-standard" {
                usable = false;
            }
        }
        if !usable {
            continue;
        }
        let distance = match size {
            Some(s) if s >= ICON_SIZE => s - ICON_SIZE,
            Some(s) => (ICON_SIZE - s) * 4,
            None => 1_000,
        };
        let score = (u8::from(plated), u8::from(light), distance);
        if best.as_ref().is_none_or(|(b, ..)| score < *b) {
            best = Some((score, c.clone(), plated));
        }
    }
    best.map(|(_, p, plated)| (p, plated))
}

/// Executable names that never carry the program's own icon.
fn helper_exe(stem: &str) -> bool {
    let s = stem.to_lowercase();
    [
        "unins", "uninst", "setup", "update", "crash", "helper", "install", "elevate",
    ]
    .iter()
    .any(|h| s.starts_with(h) || s.ends_with(h))
}

/// The icon files to try for `p`, best first. `list_dir` lists a folder's
/// entries (files and folders); `local` keeps paths on local drives only.
#[must_use]
pub fn sources(
    p: &Program,
    reg: &dyn Registry,
    list_dir: &dyn Fn(&Path) -> Vec<PathBuf>,
    local: &dyn Fn(&Path) -> bool,
) -> Vec<IconSource> {
    let mut out: Vec<IconSource> = Vec::new();
    // Store: the logo its manifest names
    if let Some(logo) = p.logo.as_deref().filter(|l| local(l)) {
        if let Some(dir) = logo.parent() {
            let mut files = list_dir(dir);
            let sub: Vec<PathBuf> = files
                .iter()
                .filter(|f| {
                    f.file_name()
                        .is_some_and(|n| is_qualifier_dir(&n.to_string_lossy()))
                })
                .cloned()
                .collect();
            for d in sub {
                files.extend(list_dir(&d));
            }
            if let Some((path, plate)) = best_logo(logo, &files) {
                out.push(IconSource::Png { path, plate });
            }
        }
    }
    // the vendor's DisplayIcon
    if let Some(icon) = p.icon.as_deref().filter(|i| local(i)) {
        match ext_of(icon).as_str() {
            "png" => out.push(IconSource::Png {
                path: icon.to_path_buf(),
                plate: false,
            }),
            e if ICON_FILES.contains(&e) => {
                out.push(IconSource::Resource(icon.to_path_buf(), p.icon_index));
            }
            _ => {}
        }
    }
    // Windows Installer: the product icon it keeps under C:\Windows\Installer
    if let Some(packed) = p.product_code.as_deref().and_then(packed_guid) {
        for (hive, key) in [
            (
                Hive::LocalMachine,
                format!(r"SOFTWARE\Classes\Installer\Products\{packed}"),
            ),
            (
                Hive::CurrentUser,
                format!(r"Software\Microsoft\Installer\Products\{packed}"),
            ),
        ] {
            let icon = reg
                .values(hive, &key)
                .and_then(|v| text(&v, "ProductIcon").and_then(inventory::display_icon));
            if let Some((path, index)) = icon.filter(|(p, _)| local(p)) {
                if ICON_FILES.contains(&ext_of(&path).as_str()) {
                    out.push(IconSource::Resource(path, index));
                }
            }
        }
    }
    // the install folder's program file that carries the program's name
    if let Some(dir) = p.install_location.as_deref().filter(|d| local(d)) {
        let exes: Vec<PathBuf> = list_dir(dir)
            .into_iter()
            .filter(|f| ext_of(f) == "exe")
            .filter(|f| {
                f.file_stem()
                    .is_some_and(|s| !helper_exe(&s.to_string_lossy()))
            })
            .collect();
        let dir_name = dir.file_name().map(|n| n.to_string_lossy().into_owned());
        let keys = names::program_keys(&p.name, p.publisher.as_deref(), dir_name.as_deref(), &[]);
        let named = exes.iter().find(|e| {
            e.file_stem()
                .is_some_and(|s| names::matches(&s.to_string_lossy(), &keys))
        });
        match (named, exes.as_slice()) {
            (Some(e), _) | (None, [e]) => out.push(IconSource::Resource(e.clone(), 0)),
            _ => {}
        }
    }
    out.dedup();
    out
}

/// The local drives icons may be read from (fixed, removable and RAM disks that
/// are ready; never network or optical drives).
#[must_use]
pub fn local_drives() -> Vec<String> {
    use nfd_platform::volumes::VolumeKind;
    nfd_platform::volumes::list()
        .into_iter()
        .filter(|v| {
            v.ready
                && matches!(
                    v.kind,
                    VolumeKind::Fixed | VolumeKind::Removable | VolumeKind::RamDisk
                )
        })
        .map(|v| v.root.to_string_lossy().into_owned())
        .collect()
}

fn list_dir(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).take(4_000).collect())
        .unwrap_or_default()
}

/// The icon of each program (programs without one are left out).
#[must_use]
pub fn program_icons(programs: &[Program]) -> Vec<(String, ProgramIcon)> {
    let drives = local_drives();
    let local = |p: &Path| is_local(p, &drives);
    let reg = crate::registry::LiveRegistry;
    programs
        .iter()
        .filter_map(|p| {
            sources(p, &reg, &list_dir, &local)
                .iter()
                .find_map(imp::read)
                .map(|icon| (p.id.clone(), icon))
        })
        .collect()
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW, HBITMAP,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, GetIconInfo, HICON, ICONINFO, PrivateExtractIconsW,
    };

    use super::{ICON_SIZE, IconSource, MAX_LOGO_BYTES, ProgramIcon};

    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    pub fn read(src: &IconSource) -> Option<ProgramIcon> {
        match src {
            IconSource::Png { path, plate } => {
                let len = std::fs::metadata(path).ok()?.len();
                if len > MAX_LOGO_BYTES {
                    return None;
                }
                let png = std::fs::read(path).ok()?;
                png.starts_with(&PNG_SIGNATURE)
                    .then_some(ProgramIcon { png, plate: *plate })
            }
            IconSource::Resource(path, index) => {
                let (w, h, rgba) = extract(path, *index)?;
                encode_png(&rgba, w, h).map(|png| ProgramIcon { png, plate: false })
            }
        }
    }

    /// One icon of `path` at [`ICON_SIZE`], as RGBA rows.
    fn extract(path: &Path, index: i32) -> Option<(u32, u32, Vec<u8>)> {
        if !path.is_file() {
            return None;
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if wide.len() > 260 {
            return None; // the extractor takes MAX_PATH paths only
        }
        let size = ICON_SIZE as i32;
        let mut icon: HICON = ptr::null_mut();
        let mut id = 0u32;
        // SAFETY: a NUL-terminated path and room for one icon; the file is loaded
        // as data, never run.
        let n = unsafe {
            PrivateExtractIconsW(wide.as_ptr(), index, size, size, &mut icon, &mut id, 1, 0)
        };
        if n == 0 || n == u32::MAX || icon.is_null() {
            return None;
        }
        // SAFETY: `icon` is a valid icon handle owned here, destroyed below.
        let out = unsafe { icon_rgba(icon) };
        // SAFETY: as above.
        unsafe { DestroyIcon(icon) };
        out
    }

    /// The pixels of an icon (its mask applied when it has no alpha channel).
    ///
    /// # Safety
    /// `icon` must be a valid icon handle.
    unsafe fn icon_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
        let mut info = ICONINFO::default();
        // SAFETY: valid icon handle and output struct.
        if unsafe { GetIconInfo(icon, &mut info) } == 0 {
            return None;
        }
        let out = if info.hbmColor.is_null() {
            None // monochrome icon
        } else {
            // SAFETY: both bitmaps come from GetIconInfo.
            unsafe { color_pixels(info.hbmColor, info.hbmMask) }
        };
        // SAFETY: GetIconInfo hands the caller copies of both bitmaps.
        unsafe {
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
        }
        out
    }

    /// # Safety
    /// `color` and `mask` must be valid bitmaps not selected into a DC.
    unsafe fn color_pixels(color: HBITMAP, mask: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
        let mut bm = BITMAP::default();
        // SAFETY: valid bitmap, output sized for a BITMAP.
        let got = unsafe {
            GetObjectW(
                color,
                std::mem::size_of::<BITMAP>() as i32,
                (&raw mut bm).cast(),
            )
        };
        if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 || bm.bmWidth > 256 || bm.bmHeight > 256
        {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        // SAFETY: a memory DC compatible with the screen, deleted below.
        let dc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
        if dc.is_null() {
            return None;
        }
        let read = |bitmap: HBITMAP| -> Option<Vec<u8>> {
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // top-down rows
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..BITMAPINFOHEADER::default()
                },
                ..BITMAPINFO::default()
            };
            let mut px = vec![0u8; (w * h * 4) as usize];
            // SAFETY: the buffer holds `h` rows of `w` 32-bit pixels.
            let lines = unsafe {
                GetDIBits(
                    dc,
                    bitmap,
                    0,
                    h as u32,
                    px.as_mut_ptr().cast(),
                    &mut bmi,
                    DIB_RGB_COLORS,
                )
            };
            (lines == h).then_some(px)
        };
        let color_px = read(color);
        let mask_px = if mask.is_null() { None } else { read(mask) };
        // SAFETY: created above.
        unsafe { DeleteDC(dc) };
        let mut px = color_px?;
        if !px.chunks_exact(4).any(|p| p[3] != 0) {
            // no alpha channel: the mask says what is transparent (white)
            let mask_px = mask_px?;
            for (p, m) in px.chunks_exact_mut(4).zip(mask_px.chunks_exact(4)) {
                p[3] = if m[0] == 0 { 255 } else { 0 };
            }
        }
        if !px.chunks_exact(4).any(|p| p[3] != 0) {
            return None; // nothing visible
        }
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2); // BGRA → RGBA
        }
        Some((w as u32, h as u32, px))
    }

    fn encode_png(rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().ok()?;
            writer.write_image_data(rgba).ok()?;
        }
        Some(out)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{IconSource, ProgramIcon};

    pub fn read(_: &IconSource) -> Option<ProgramIcon> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::MemRegistry;
    use crate::{Scope, Source};

    fn pb(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn packed_guids_match_windows_installer() {
        // Office 2016's well-known key
        assert_eq!(
            packed_guid("{90160000-008C-0000-0000-0000000FF1CE}").as_deref(),
            Some("00006109C80000000000000000F01FEC")
        );
        assert_eq!(
            packed_guid("{12345678-90ab-cdef-1234-567890ABCDEF}").as_deref(),
            Some("87654321BA09FEDC2143658709BADCFE")
        );
        assert_eq!(packed_guid("{1234}"), None);
        assert_eq!(packed_guid("{1234567G-90AB-CDEF-1234-567890ABCDEF}"), None);
    }

    #[test]
    fn local_paths_only() {
        let drives = vec![r"C:\".to_owned(), r"D:\".to_owned()];
        assert!(is_local(Path::new(r"c:\Program Files\A\a.exe"), &drives));
        assert!(is_local(Path::new(r"D:\Apps\a.exe"), &drives));
        assert!(!is_local(Path::new(r"Z:\Share\a.exe"), &drives));
        assert!(!is_local(Path::new(r"\\server\share\a.exe"), &drives));
        assert!(!is_local(Path::new(r"\\?\C:\a.exe"), &drives));
    }

    #[test]
    fn the_unplated_logo_closest_to_48_wins() {
        let base = pb("/app/Assets/Square44x44Logo.png");
        let files: Vec<PathBuf> = [
            "/app/Assets/Square44x44Logo.scale-100.png",
            "/app/Assets/Square44x44Logo.scale-200.png",
            "/app/Assets/Square44x44Logo.targetsize-16.png",
            "/app/Assets/Square44x44Logo.targetsize-48.png",
            "/app/Assets/Square44x44Logo.targetsize-256_altform-unplated.png",
            "/app/Assets/Square44x44Logo.targetsize-48_altform-unplated.png",
            "/app/Assets/Square44x44Logo.targetsize-48_altform-lightunplated.png",
            "/app/Assets/Square44x44Logo.targetsize-48_altform-unplated_contrast-black.png",
            "/app/Assets/Square44x44LogoBig.png",
            "/app/Assets/StoreLogo.png",
        ]
        .iter()
        .map(|s| pb(s))
        .collect();
        assert_eq!(
            best_logo(&base, &files),
            Some((
                pb("/app/Assets/Square44x44Logo.targetsize-48_altform-unplated.png"),
                false
            ))
        );
        // only plated ones: the closest size, flagged for a plate
        let plated = &files[..4];
        assert_eq!(
            best_logo(&base, plated),
            Some((pb("/app/Assets/Square44x44Logo.targetsize-48.png"), true))
        );
        // the bare name when nothing else exists
        assert_eq!(
            best_logo(&base, &[pb("/app/Assets/Square44x44Logo.png")]),
            Some((pb("/app/Assets/Square44x44Logo.png"), true))
        );
        assert_eq!(best_logo(&base, &[pb("/app/Assets/StoreLogo.png")]), None);
    }

    #[test]
    fn logos_in_qualifier_folders() {
        let base = pb("/app/Images/Logo.png");
        let files = vec![
            pb("/app/Images/scale-100/Logo.png"),
            pb("/app/Images/scale-200/Logo.png"),
            pb("/app/Images/contrast-black/Logo.png"),
            pb("/app/Images/Other/Logo.png"),
        ];
        assert_eq!(
            best_logo(&base, &files),
            Some((pb("/app/Images/scale-100/Logo.png"), true))
        );
    }

    fn program(name: &str) -> Program {
        Program {
            id: format!("hkcu:{name}"),
            name: name.to_owned(),
            publisher: Some("Acme".into()),
            version: None,
            source: Source::Win32,
            scope: Scope::User,
            arch: None,
            install_location: None,
            uninstall: None,
            quiet_uninstall: None,
            icon: None,
            icon_index: 0,
            logo: None,
            installed_at: None,
            size: 0,
            size_measured: false,
            product_code: None,
            key: None,
            package: None,
            url: None,
            caution: None,
            blocked: None,
            broken: false,
        }
    }

    #[test]
    fn sources_in_order_and_local_only() {
        let mut p = program("Acme Studio");
        p.icon = Some(pb("/apps/acme/studio.exe"));
        p.icon_index = -101;
        p.product_code = Some("{12345678-90AB-CDEF-1234-567890ABCDEF}".into());
        p.install_location = Some(pb("/apps/AcmeStudio"));
        let mut reg = MemRegistry::new();
        reg.sz(
            Hive::LocalMachine,
            r"SOFTWARE\Classes\Installer\Products\87654321BA09FEDC2143658709BADCFE",
            "ProductIcon",
            r#""/win/Installer/{1234}/icon.ico""#,
        );
        let list = |d: &Path| -> Vec<PathBuf> {
            if d == Path::new("/apps/AcmeStudio") {
                vec![
                    pb("/apps/AcmeStudio/unins000.exe"),
                    pb("/apps/AcmeStudio/AcmeStudio.exe"),
                    pb("/apps/AcmeStudio/helper.exe"),
                    pb("/apps/AcmeStudio/readme.txt"),
                ]
            } else {
                Vec::new()
            }
        };
        let all = |_: &Path| true;
        let got = sources(&p, &reg, &list, &all);
        if cfg!(windows) {
            // `/win/…` is not absolute on Windows: the product icon is left out
            assert_eq!(got.len(), 2, "{got:?}");
        } else {
            assert_eq!(
                got,
                vec![
                    IconSource::Resource(pb("/apps/acme/studio.exe"), -101),
                    IconSource::Resource(pb("/win/Installer/{1234}/icon.ico"), 0),
                    IconSource::Resource(pb("/apps/AcmeStudio/AcmeStudio.exe"), 0),
                ]
            );
        }
        assert_eq!(
            got[0],
            IconSource::Resource(pb("/apps/acme/studio.exe"), -101)
        );
        assert_eq!(
            got.last(),
            Some(&IconSource::Resource(
                pb("/apps/AcmeStudio/AcmeStudio.exe"),
                0
            ))
        );
        // nothing from a drive that is not local
        let none = |_: &Path| false;
        assert!(sources(&p, &reg, &list, &none).is_empty());
    }

    /// The real extractor on this machine: Windows' own Explorer has an icon, a
    /// missing file has none.
    #[cfg(windows)]
    #[test]
    fn icons_of_real_program_files_become_pngs() {
        let explorer = crate::runner::system_root().join("explorer.exe");
        let icon = imp::read(&IconSource::Resource(explorer.clone(), 0)).expect("an icon");
        assert!(icon.png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(!icon.plate);
        let reader = png::Decoder::new(std::io::Cursor::new(&icon.png))
            .read_info()
            .expect("a valid PNG");
        assert_eq!(
            (reader.info().width, reader.info().height),
            (ICON_SIZE, ICON_SIZE)
        );
        assert!(imp::read(&IconSource::Resource(pb(r"C:\nope\none.exe"), 0)).is_none());

        let mut p = program("Explorer");
        p.icon = Some(explorer);
        let got = program_icons(std::slice::from_ref(&p));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, p.id);
    }

    #[test]
    fn a_lone_program_file_is_used_helpers_never() {
        let mut p = program("Something Else");
        p.install_location = Some(pb("/apps/X"));
        let all = |_: &Path| true;
        let reg = MemRegistry::new();
        let one = |_: &Path| vec![pb("/apps/X/tool.exe"), pb("/apps/X/uninstall.exe")];
        assert_eq!(
            sources(&p, &reg, &one, &all),
            vec![IconSource::Resource(pb("/apps/X/tool.exe"), 0)]
        );
        let two = |_: &Path| vec![pb("/apps/X/a.exe"), pb("/apps/X/b.exe")];
        assert!(sources(&p, &reg, &two, &all).is_empty());
    }
}
