//! The files the setup installs, appended to the setup executable.
//!
//! Layout of a packed setup: `stub exe | zstd(archive) | zero padding | footer`.
//! The padding keeps the file length a multiple of 8 so Authenticode signing (which
//! appends its certificate table at an 8-byte boundary) adds nothing between the
//! footer and the certificates. The reader finds the footer before the certificate
//! table named by the PE security directory, so a signed setup still unpacks.
//!
//! Archive (before compression): for each file `u32 name length | name (UTF-8,
//! `/`-separated, relative) | u64 size | bytes`, ended by a zero name length.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

/// Footer marker.
pub const MAGIC: &[u8; 8] = b"NFDSETUP";
const VERSION: u32 = 1;
const FOOTER_LEN: usize = 32;
/// Name the stub itself gets inside the payload (the uninstaller).
pub const UNINSTALLER: &str = "uninstall.exe";

/// One file of the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// A relative, `/`-separated name that cannot escape the install folder.
pub fn check_name(name: &str) -> io::Result<()> {
    let p = Path::new(name);
    let ok = !name.is_empty()
        && !name.contains('\\')
        && !name.contains(':')
        && p.components().all(|c| matches!(c, Component::Normal(_)));
    if ok {
        Ok(())
    } else {
        Err(bad(format!("unsafe file name in payload: {name}")))
    }
}

/// Serialize and compress `entries`.
///
/// # Errors
/// An unsafe name or a compression failure.
pub fn compress(entries: &[Entry]) -> io::Result<Vec<u8>> {
    let mut raw = Vec::new();
    for e in entries {
        check_name(&e.name)?;
        let len = u32::try_from(e.name.len()).map_err(|_| bad("name too long"))?;
        raw.extend_from_slice(&len.to_le_bytes());
        raw.extend_from_slice(e.name.as_bytes());
        raw.extend_from_slice(&(e.data.len() as u64).to_le_bytes());
        raw.extend_from_slice(&e.data);
    }
    raw.extend_from_slice(&0u32.to_le_bytes());
    zstd::bulk::compress(&raw, 19)
}

/// Inverse of [`compress`].
///
/// # Errors
/// Corrupt data or an unsafe name.
pub fn decompress(packed: &[u8]) -> io::Result<Vec<Entry>> {
    let mut raw = Vec::new();
    zstd::stream::Decoder::new(packed)?.read_to_end(&mut raw)?;
    let mut out = Vec::new();
    let mut pos = 0usize;
    let mut take = |n: usize| -> io::Result<&[u8]> {
        let end = pos.checked_add(n).filter(|e| *e <= raw.len());
        let end = end.ok_or_else(|| bad("truncated payload"))?;
        let s = &raw[pos..end];
        pos = end;
        Ok(s)
    };
    loop {
        let len = u32::from_le_bytes(take(4)?.try_into().map_err(|_| bad("len"))?) as usize;
        if len == 0 {
            break;
        }
        let name = String::from_utf8(take(len)?.to_vec()).map_err(|_| bad("name"))?;
        check_name(&name)?;
        let size = u64::from_le_bytes(take(8)?.try_into().map_err(|_| bad("size"))?);
        let size = usize::try_from(size).map_err(|_| bad("size"))?;
        let data = take(size)?.to_vec();
        out.push(Entry { name, data });
    }
    Ok(out)
}

/// `stub` + packed payload + padding + footer.
///
/// # Errors
/// See [`compress`].
pub fn pack(stub: &[u8], entries: &[Entry]) -> io::Result<Vec<u8>> {
    let packed = compress(entries)?;
    let mut out = Vec::with_capacity(stub.len() + packed.len() + 64);
    out.extend_from_slice(stub);
    let offset = out.len() as u64;
    out.extend_from_slice(&packed);
    while (out.len() + FOOTER_LEN) % 8 != 0 {
        out.push(0);
    }
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&(packed.len() as u64).to_le_bytes());
    Ok(out)
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Where the certificate table of a signed PE starts (= end of our data), or the
/// file length when unsigned / not a PE.
fn data_end(file: &[u8]) -> usize {
    let cert = (|| {
        if file.get(..2)? != b"MZ" {
            return None;
        }
        let pe = u32_at(file, 0x3c)? as usize;
        if file.get(pe..pe + 4)? != b"PE\0\0" {
            return None;
        }
        let opt = pe + 24;
        let magic = u16::from_le_bytes(file.get(opt..opt + 2)?.try_into().ok()?);
        // data directory 4 = security: file offset + size
        let dir = match magic {
            0x20b => opt + 112 + 4 * 8,
            0x10b => opt + 96 + 4 * 8,
            _ => return None,
        };
        let off = u32_at(file, dir)? as usize;
        let size = u32_at(file, dir + 4)? as usize;
        (size > 0 && off > 0 && off <= file.len()).then_some(off)
    })();
    cert.unwrap_or(file.len())
}

/// The packed payload of a setup file, if it carries one.
#[must_use]
pub fn find(file: &[u8]) -> Option<&[u8]> {
    let mut end = data_end(file);
    // tolerate alignment padding a signing tool may still have added
    for _ in 0..8 {
        if end >= FOOTER_LEN && &file[end - FOOTER_LEN..end - FOOTER_LEN + 8] == MAGIC {
            let f = end - FOOTER_LEN;
            if u32_at(file, f + 8)? != VERSION {
                return None;
            }
            let offset = usize::try_from(u64_at(file, f + 16)?).ok()?;
            let len = usize::try_from(u64_at(file, f + 24)?).ok()?;
            return file.get(offset..offset.checked_add(len)?);
        }
        if end == 0 || file[end - 1] != 0 {
            break;
        }
        end -= 1;
    }
    None
}

/// The stub part of a packed setup (everything before the payload), used as the
/// uninstaller when packing.
#[must_use]
pub fn stub_of(file: &[u8]) -> &[u8] {
    match find(file) {
        Some(p) => {
            let start = p.as_ptr() as usize - file.as_ptr() as usize;
            &file[..start]
        }
        None => file,
    }
}

/// Every file below `dir` as payload entries (sorted, `/`-separated names).
///
/// # Errors
/// I/O failure while reading.
pub fn entries_from_dir(dir: &Path) -> io::Result<Vec<Entry>> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<Entry>) -> io::Result<()> {
        let mut items: Vec<_> = fs::read_dir(dir)?.collect::<io::Result<_>>()?;
        items.sort_by_key(fs::DirEntry::file_name);
        for item in items {
            let path = item.path();
            if item.file_type()?.is_dir() {
                walk(base, &path, out)?;
            } else {
                let rel = path.strip_prefix(base).map_err(|_| bad("outside dir"))?;
                let name: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push(Entry {
                    name: name.join("/"),
                    data: fs::read(&path)?,
                });
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

/// Write `entries` below `dir`; returns the paths written. Each file is written to a
/// temporary name first and then moved into place.
///
/// # Errors
/// The first file that could not be written.
pub fn extract(entries: &[Entry], dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for e in entries {
        check_name(&e.name)?;
        let target = e.name.split('/').fold(dir.to_path_buf(), |p, s| p.join(s));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = target.with_extension("nfd-new");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&e.data)?;
            f.sync_all()?;
        }
        if let Err(err) = fs::rename(&tmp, &target) {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
        written.push(target);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Entry> {
        vec![
            Entry {
                name: "nanofreedisk.exe".into(),
                data: vec![1, 2, 3, 4, 5],
            },
            Entry {
                name: "rules/ai-tools.toml".into(),
                data: b"[pack]\nid = \"x\"\n".to_vec(),
            },
            Entry {
                name: "empty.txt".into(),
                data: Vec::new(),
            },
        ]
    }

    #[test]
    fn round_trip() {
        let stub = b"MZ-not-really-a-pe".to_vec();
        let file = pack(&stub, &sample()).expect("pack");
        assert_eq!(file.len() % 8, 0);
        let payload = find(&file).expect("found");
        assert_eq!(decompress(payload).expect("decompress"), sample());
        assert_eq!(stub_of(&file), &stub[..]);
    }

    #[test]
    fn no_payload_in_a_plain_exe() {
        assert!(find(b"MZ plain").is_none());
        assert!(find(&[]).is_none());
        assert_eq!(stub_of(b"MZ plain"), b"MZ plain");
    }

    /// A minimal PE32+ header whose security directory points at `cert_off`.
    fn pe_with_cert(body: &[u8], cert: &[u8]) -> Vec<u8> {
        let mut f = body.to_vec();
        let cert_off = f.len();
        f.extend_from_slice(cert);
        // MZ header at 0, e_lfanew = 0x40 (body starts with room for it)
        f[0] = b'M';
        f[1] = b'Z';
        f[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        f[0x40..0x44].copy_from_slice(b"PE\0\0");
        let opt = 0x40 + 24;
        f[opt..opt + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        let dir = opt + 112 + 32;
        f[dir..dir + 4].copy_from_slice(&(cert_off as u32).to_le_bytes());
        f[dir + 4..dir + 8].copy_from_slice(&(cert.len() as u32).to_le_bytes());
        f
    }

    #[test]
    fn signed_file_still_unpacks() {
        let stub = vec![0u8; 512];
        let packed = pack(&stub, &sample()).expect("pack");
        let signed = pe_with_cert(&packed, &[0xAB; 40]);
        let payload = find(&signed).expect("found despite certificates");
        assert_eq!(decompress(payload).expect("ok"), sample());
    }

    #[test]
    fn names_cannot_escape() {
        for bad_name in ["../x", "/abs", "C:/x", "a\\b", "", "a/../../b"] {
            assert!(check_name(bad_name).is_err(), "{bad_name}");
        }
        assert!(check_name("rules/x.toml").is_ok());
        let evil = vec![Entry {
            name: "../evil.exe".into(),
            data: vec![1],
        }];
        assert!(compress(&evil).is_err());
    }

    #[test]
    fn extract_and_read_dir() {
        let src = tempfile::tempdir().expect("tmp");
        let written = extract(&sample(), src.path()).expect("extract");
        assert_eq!(written.len(), 3);
        let mut back = entries_from_dir(src.path()).expect("read");
        back.sort_by(|a, b| a.name.cmp(&b.name));
        let mut want = sample();
        want.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(back, want);
    }
}
