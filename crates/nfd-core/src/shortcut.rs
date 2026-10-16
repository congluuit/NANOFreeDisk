//! Windows shortcut (`.lnk`) target reader for the broken-shortcut finder (PLAN §13
//! A3).
//!
//! Implements the parts of the Shell Link format ([MS-SHLLINK]) needed to learn where
//! a shortcut points: the `LinkInfo` local / network path, the relative path and the
//! environment-variable target block. Nothing here resolves through the shell, so it
//! never triggers the "searching for the target" UI or network access, and it works on
//! any OS (tests run everywhere).
//!
//! When the target cannot be read reliably (ANSI paths with non-ASCII bytes, shell
//! namespace items such as "This PC", advertised MSI shortcuts) the result is
//! [`Target::Unknown`]: a shortcut is only ever reported broken when its target path is
//! known for certain.
//!
//! [MS-SHLLINK]: https://learn.microsoft.com/openspecs/windows_protocols/ms-shllink/

use std::path::{Path, PathBuf};

use serde::Serialize;

const HEADER_SIZE: usize = 0x4C;
const CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

const HAS_ID_LIST: u32 = 1 << 0;
const HAS_LINK_INFO: u32 = 1 << 1;
const HAS_NAME: u32 = 1 << 2;
const HAS_RELATIVE_PATH: u32 = 1 << 3;
const HAS_WORKING_DIR: u32 = 1 << 4;
const HAS_ARGUMENTS: u32 = 1 << 5;
const HAS_ICON_LOCATION: u32 = 1 << 6;
const IS_UNICODE: u32 = 1 << 7;
const HAS_EXP_STRING: u32 = 1 << 9;
const HAS_DARWIN_ID: u32 = 1 << 12;

const ENV_BLOCK_SIGNATURE: u32 = 0xA000_0001;

/// Largest `.lnk` file worth reading (real ones are a few KiB).
pub const MAX_LINK_SIZE: u64 = 1 << 20;

/// Where a shortcut points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "path", rename_all = "snake_case")]
pub enum Target {
    /// A path on a local drive letter (environment variables already expanded).
    Local(String),
    /// A UNC path (`\\server\share\…`); never probed.
    Network(String),
    /// No file-system target, or one that cannot be decoded reliably.
    Unknown,
}

/// Why a file is not a shell link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not a valid shell link")]
pub struct NotALink;

/// Read the target of a shortcut whose bytes are `data`. `link_dir` is the folder
/// holding the `.lnk` (for relative targets); `env` looks up environment variables
/// for `%VAR%` expansion.
///
/// # Errors
/// [`NotALink`] when the header is missing or the structure is truncated.
pub fn parse(
    data: &[u8],
    link_dir: &Path,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<Target, NotALink> {
    let r = Reader(data);
    if r.u32(0)? as usize != HEADER_SIZE || data.get(4..20) != Some(&CLSID[..]) {
        return Err(NotALink);
    }
    let flags = r.u32(0x14)?;
    let mut pos = HEADER_SIZE;
    if flags & HAS_ID_LIST != 0 {
        pos += 2 + usize::from(r.u16(pos)?);
    }
    let mut info_target = None;
    if flags & HAS_LINK_INFO != 0 {
        let size = r.u32(pos)? as usize;
        info_target = link_info(&Reader(r.slice(pos, size)?))?;
        pos += size;
    }
    let mut relative = None;
    let unicode = flags & IS_UNICODE != 0;
    for bit in [
        HAS_NAME,
        HAS_RELATIVE_PATH,
        HAS_WORKING_DIR,
        HAS_ARGUMENTS,
        HAS_ICON_LOCATION,
    ] {
        if flags & bit == 0 {
            continue;
        }
        let chars = usize::from(r.u16(pos)?);
        let bytes = if unicode { chars * 2 } else { chars };
        let raw = r.slice(pos + 2, bytes)?;
        if bit == HAS_RELATIVE_PATH {
            relative = if unicode { utf16(raw) } else { ascii(raw) };
        }
        pos += 2 + bytes;
    }
    let env_target = if flags & HAS_EXP_STRING != 0 {
        env_block(&r, pos)
    } else {
        None
    };
    if flags & HAS_DARWIN_ID != 0 {
        // advertised (Windows Installer) shortcut: resolved by msi.dll, not a path
        return Ok(Target::Unknown);
    }

    if let Some(t) = env_target.and_then(|t| expand(&t, env)) {
        return Ok(classify(t));
    }
    if let Some(t) = info_target {
        return Ok(t);
    }
    if let Some(rel) = relative.filter(|r| !r.is_empty()) {
        let joined = link_dir.join(rel);
        return Ok(classify(normalize(&joined)));
    }
    Ok(Target::Unknown)
}

fn classify(path: String) -> Target {
    if path.starts_with(r"\\") || path.starts_with("//") {
        Target::Network(path)
    } else if has_drive(&path) {
        Target::Local(path)
    } else {
        Target::Unknown
    }
}

fn has_drive(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

/// `LinkInfo` → local base path + suffix, or network share + suffix.
fn link_info(r: &Reader<'_>) -> Result<Option<Target>, NotALink> {
    let header = r.u32(4)? as usize;
    let flags = r.u32(8)?;
    let suffix_ansi = r.u32(0x18)? as usize;
    let (base_uni, suffix_uni) = if header >= 0x24 {
        (r.u32(0x1C)? as usize, r.u32(0x20)? as usize)
    } else {
        (0, 0)
    };
    let suffix = if suffix_uni != 0 {
        r.wstr(suffix_uni)
    } else {
        r.astr(suffix_ansi)
    };
    if flags & 1 != 0 {
        let base = if base_uni != 0 {
            r.wstr(base_uni)
        } else {
            r.astr(r.u32(0x10)? as usize)
        };
        return Ok(match (base, suffix) {
            (Some(b), Some(s)) => Some(classify(join(&b, &s))),
            _ => Some(Target::Unknown),
        });
    }
    if flags & 2 != 0 {
        let net = r.u32(0x14)? as usize;
        let name_off = r.u32(net + 8)? as usize;
        let name_uni = if name_off > 0x14 {
            r.u32(net + 0x14)? as usize
        } else {
            0
        };
        let name = if name_uni != 0 {
            r.wstr(net + name_uni)
        } else {
            r.astr(net + name_off)
        };
        return Ok(match (name, suffix) {
            (Some(n), Some(s)) => Some(Target::Network(join(&n, &s))),
            _ => Some(Target::Unknown),
        });
    }
    Ok(None)
}

/// `EnvironmentVariableDataBlock` target (Unicode preferred), if present.
fn env_block(r: &Reader<'_>, mut pos: usize) -> Option<String> {
    loop {
        let size = r.u32(pos).ok()? as usize;
        if size < 8 {
            return None;
        }
        if r.u32(pos + 4).ok()? == ENV_BLOCK_SIGNATURE && size >= 8 + 260 + 520 {
            let uni = r.slice(pos + 8 + 260, 520).ok()?;
            let ansi = r.slice(pos + 8, 260).ok()?;
            let until_nul = |b: &[u8]| b.iter().position(|&c| c == 0).unwrap_or(b.len());
            let w = utf16(uni).map(|s| s.trim_end_matches('\0').to_owned());
            return w
                .filter(|s| !s.is_empty())
                .or_else(|| ascii(&ansi[..until_nul(ansi)]));
        }
        pos += size;
    }
}

/// Expand `%VAR%` references; `None` if one is unknown (target then not trusted).
fn expand(s: &str, env: &impl Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        out.push_str(&env(&after[..end])?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

fn join(base: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        base.to_owned()
    } else if base.ends_with('\\') {
        format!("{base}{suffix}")
    } else {
        format!("{base}\\{suffix}")
    }
}

/// Fold `.` and `..` components lexically (relative targets like `..\app.exe`).
fn normalize(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    let s = path.to_string_lossy().replace('/', "\\");
    for part in s.split('\\') {
        match part {
            "." => {}
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            p => parts.push(p.to_owned()),
        }
    }
    parts.join("\\")
}

fn utf16(raw: &[u8]) -> Option<String> {
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).ok()
}

/// ANSI strings are only trusted when they are pure ASCII (the code page is unknown).
fn ascii(raw: &[u8]) -> Option<String> {
    raw.is_ascii()
        .then(|| String::from_utf8_lossy(raw).into_owned())
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn slice(&self, pos: usize, len: usize) -> Result<&[u8], NotALink> {
        self.0
            .get(pos..pos.checked_add(len).ok_or(NotALink)?)
            .ok_or(NotALink)
    }

    fn u16(&self, pos: usize) -> Result<u16, NotALink> {
        let b = self.slice(pos, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&self, pos: usize) -> Result<u32, NotALink> {
        let b = self.slice(pos, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// NUL-terminated ANSI string at `pos` (ASCII only, see [`ascii`]).
    fn astr(&self, pos: usize) -> Option<String> {
        let rest = self.0.get(pos..)?;
        let end = rest.iter().position(|&c| c == 0)?;
        ascii(&rest[..end])
    }

    /// NUL-terminated UTF-16LE string at `pos`.
    fn wstr(&self, pos: usize) -> Option<String> {
        let rest = self.0.get(pos..)?;
        let end = rest.chunks_exact(2).position(|c| c == [0, 0])?;
        utf16(&rest[..end * 2])
    }
}

/// Verdict for one shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The target exists.
    Ok,
    /// The target's drive is present but the target is gone.
    Missing,
    /// The target's drive letter is not mounted (unplugged USB disk, old partition).
    DriveMissing,
    /// Network or unknown target: not checked.
    Unchecked,
}

/// Check `target` with `exists` (normally [`Path::exists`]-like, injected for tests).
#[must_use]
pub fn status(target: &Target, exists: &impl Fn(&Path) -> bool) -> Status {
    let Target::Local(path) = target else {
        return Status::Unchecked;
    };
    if exists(Path::new(path)) {
        return Status::Ok;
    }
    let drive = PathBuf::from(format!("{}\\", &path[..2]));
    if exists(&drive) {
        Status::Missing
    } else {
        Status::DriveMissing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(flags: u32) -> Vec<u8> {
        let mut h = vec![0u8; HEADER_SIZE];
        h[0..4].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        h[4..20].copy_from_slice(&CLSID);
        h[0x14..0x18].copy_from_slice(&flags.to_le_bytes());
        h
    }

    fn wz(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    /// LinkInfo with a local base path (ANSI + optional Unicode) and empty suffix.
    fn link_info_local(ansi: &str, unicode: Option<&str>) -> Vec<u8> {
        let head = if unicode.is_some() { 0x24 } else { 0x1C };
        let mut body = Vec::new();
        let base_off = head;
        body.extend(ansi.as_bytes());
        body.push(0);
        let suffix_off = head + body.len();
        body.push(0);
        let mut uni_off = 0;
        let mut suf_uni_off = 0;
        if let Some(u) = unicode {
            uni_off = head + body.len();
            body.extend(wz(u));
            suf_uni_off = head + body.len();
            body.extend(wz(""));
        }
        let mut v = Vec::new();
        let total = head + body.len();
        for x in [total, head, 1, 0, base_off, 0, suffix_off] {
            v.extend((x as u32).to_le_bytes());
        }
        if unicode.is_some() {
            v.extend((uni_off as u32).to_le_bytes());
            v.extend((suf_uni_off as u32).to_le_bytes());
        }
        v.extend(body);
        v
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn local_target_from_link_info() {
        let mut d = header(HAS_LINK_INFO);
        d.extend(link_info_local(r"C:\Tools\app.exe", None));
        d.extend(0u32.to_le_bytes());
        let t = parse(&d, Path::new(r"C:\x"), &no_env).expect("parse");
        assert_eq!(t, Target::Local(r"C:\Tools\app.exe".into()));
    }

    #[test]
    fn unicode_path_wins_and_ansi_non_ascii_is_untrusted() {
        let mut d = header(HAS_LINK_INFO);
        d.extend(link_info_local("C:\\Caf\u{e9}", Some(r"D:\Phim\Tết.mp4")));
        let t = parse(&d, Path::new("."), &no_env).expect("parse");
        assert_eq!(t, Target::Local(r"D:\Phim\Tết.mp4".into()));

        let mut d = header(HAS_LINK_INFO);
        let mut info = link_info_local("C:\\Cafx", None);
        let i = info.iter().position(|&b| b == b'x').expect("x");
        info[i] = 0xE9; // ANSI é: code page unknown
        d.extend(info);
        assert_eq!(parse(&d, Path::new("."), &no_env), Ok(Target::Unknown));
    }

    #[test]
    fn relative_path_and_env_block() {
        // IDList (skipped) + unicode relative path
        let mut d = header(HAS_ID_LIST | HAS_RELATIVE_PATH | IS_UNICODE);
        d.extend(4u16.to_le_bytes());
        d.extend([0xAA; 4]);
        let rel = r"..\bin\tool.exe";
        d.extend((rel.encode_utf16().count() as u16).to_le_bytes());
        d.extend(rel.encode_utf16().flat_map(u16::to_le_bytes));
        let t = parse(&d, Path::new(r"C:\Users\me\Desktop"), &no_env).expect("parse");
        assert_eq!(t, Target::Local(r"C:\Users\me\bin\tool.exe".into()));

        // environment block overrides, %VAR% expanded
        let mut d = header(HAS_EXP_STRING | IS_UNICODE);
        let mut block = vec![0u8; 0x314];
        block[0..4].copy_from_slice(&0x314u32.to_le_bytes());
        block[4..8].copy_from_slice(&ENV_BLOCK_SIGNATURE.to_le_bytes());
        let w = wz(r"%ProgramFiles%\App\a.exe");
        block[8 + 260..8 + 260 + w.len()].copy_from_slice(&w);
        d.extend(block);
        d.extend(0u32.to_le_bytes());
        let env = |k: &str| (k == "ProgramFiles").then(|| r"C:\Program Files".to_owned());
        assert_eq!(
            parse(&d, Path::new("."), &env),
            Ok(Target::Local(r"C:\Program Files\App\a.exe".into()))
        );
        // unknown variable: not trusted
        assert_eq!(parse(&d, Path::new("."), &no_env), Ok(Target::Unknown));
    }

    #[test]
    fn network_darwin_and_garbage() {
        let mut d = header(HAS_LINK_INFO);
        d.extend(link_info_local(r"\\nas\share\f.txt", None));
        assert_eq!(
            parse(&d, Path::new("."), &no_env),
            Ok(Target::Network(r"\\nas\share\f.txt".into()))
        );

        let mut d = header(HAS_LINK_INFO | HAS_DARWIN_ID);
        d.extend(link_info_local(r"C:\gone.exe", None));
        assert_eq!(parse(&d, Path::new("."), &no_env), Ok(Target::Unknown));

        assert_eq!(parse(b"MZ\x90\x00", Path::new("."), &no_env), Err(NotALink));
        let mut d = header(HAS_LINK_INFO);
        d.extend(500u32.to_le_bytes()); // LinkInfo claims more than the file holds
        assert_eq!(parse(&d, Path::new("."), &no_env), Err(NotALink));
        // a shell item only (no path): unknown, never "broken"
        let mut d = header(HAS_ID_LIST);
        d.extend(0u16.to_le_bytes());
        assert_eq!(parse(&d, Path::new("."), &no_env), Ok(Target::Unknown));
    }

    #[test]
    fn network_share_from_common_network_relative_link() {
        let name = &b"\\\\nas\\share\0"[..];
        let mut cnrl = Vec::new();
        for x in [0x14 + name.len(), 0, 0x14, 0, 0] {
            cnrl.extend((x as u32).to_le_bytes());
        }
        cnrl.extend(name);
        let suffix_off = 0x1C + cnrl.len();
        let mut info = Vec::new();
        let total = suffix_off + 11;
        for x in [total, 0x1C, 2, 0, 0, 0x1C, suffix_off] {
            info.extend((x as u32).to_le_bytes());
        }
        info.extend(cnrl);
        info.extend(b"docs\\a.txt\0");
        let mut d = header(HAS_LINK_INFO);
        d.extend(info);
        assert_eq!(
            parse(&d, Path::new("."), &no_env),
            Ok(Target::Network(r"\\nas\share\docs\a.txt".into()))
        );
    }

    #[test]
    fn status_of_targets() {
        let exists = |p: &Path| {
            let s = p.to_string_lossy();
            s == r"C:\" || s == r"C:\here.exe"
        };
        assert_eq!(
            status(&Target::Local(r"C:\here.exe".into()), &exists),
            Status::Ok
        );
        assert_eq!(
            status(&Target::Local(r"C:\gone.exe".into()), &exists),
            Status::Missing
        );
        assert_eq!(
            status(&Target::Local(r"Q:\usb.txt".into()), &exists),
            Status::DriveMissing
        );
        assert_eq!(
            status(&Target::Network(r"\\nas\x".into()), &exists),
            Status::Unchecked
        );
        assert_eq!(status(&Target::Unknown, &exists), Status::Unchecked);
    }

    #[test]
    fn expand_and_normalize() {
        let env = |k: &str| Some(format!("<{k}>"));
        assert_eq!(expand("%A%\\x\\%B%", &env).as_deref(), Some(r"<A>\x\<B>"));
        assert_eq!(expand("50%", &env), None);
        assert_eq!(normalize(Path::new(r"C:\a\.\b\..\..\..\c")), r"C:\c");
    }
}
