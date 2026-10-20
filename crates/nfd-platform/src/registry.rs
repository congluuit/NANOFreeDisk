//! The registry, for the uninstaller (`docs/UNINSTALLER.md` §2.2): read the keys and
//! values programs register, and remove an uninstalled program's leftover key or
//! value — always after a `.reg` backup that [`import`] puts back.
//!
//! Always the 64-bit view: 32-bit keys are addressed through `WOW6432Node`, so a
//! path names exactly one key whatever the bitness of the caller, and backups made
//! with `reg export /reg:64` import into the same place.

use std::io;
use std::path::Path;

/// The two hives the uninstaller reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

impl Hive {
    /// `HKEY_CURRENT_USER` / `HKEY_LOCAL_MACHINE` (`.reg` files, `reg.exe`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CurrentUser => "HKEY_CURRENT_USER",
            Self::LocalMachine => "HKEY_LOCAL_MACHINE",
        }
    }

    /// `HKCU` / `HKLM`.
    #[must_use]
    pub const fn short(self) -> &'static str {
        match self {
            Self::CurrentUser => "HKCU",
            Self::LocalMachine => "HKLM",
        }
    }

    /// `HKCU`, `HKEY_CURRENT_USER`, `HKLM`, `HKEY_LOCAL_MACHINE` (any case).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "HKCU" | "HKEY_CURRENT_USER" => Some(Self::CurrentUser),
            "HKLM" | "HKEY_LOCAL_MACHINE" => Some(Self::LocalMachine),
            _ => None,
        }
    }

    /// `HKEY_…\path` → hive and path.
    #[must_use]
    pub fn split(full: &str) -> Option<(Self, &str)> {
        let (hive, rest) = full.split_once('\\').unwrap_or((full, ""));
        Some((Self::parse(hive)?, rest))
    }
}

/// A registry value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Sz(String),
    ExpandSz(String),
    MultiSz(Vec<String>),
    Dword(u32),
    Qword(u64),
    Binary(Vec<u8>),
    /// Any other type: (type, raw bytes).
    Other(u32, Vec<u8>),
}

impl Value {
    /// The text of a string value (`REG_SZ`, `REG_EXPAND_SZ`, unexpanded).
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Sz(s) | Self::ExpandSz(s) => Some(s),
            _ => None,
        }
    }

    /// A number value (`REG_DWORD`, `REG_QWORD`, or a string holding digits).
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Dword(n) => Some(u64::from(*n)),
            Self::Qword(n) => Some(*n),
            Self::Sz(s) | Self::ExpandSz(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    /// Registry type number (`REG_SZ` = 1…).
    #[must_use]
    pub const fn kind(&self) -> u32 {
        match self {
            Self::Sz(_) => 1,
            Self::ExpandSz(_) => 2,
            Self::Binary(_) => 3,
            Self::Dword(_) => 4,
            Self::MultiSz(_) => 7,
            Self::Qword(_) => 11,
            Self::Other(t, _) => *t,
        }
    }

    /// The bytes as stored (strings as UTF-16LE with their terminator).
    #[must_use]
    pub fn raw(&self) -> Vec<u8> {
        let utf16 = |s: &str| -> Vec<u8> {
            s.encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(u16::to_le_bytes)
                .collect()
        };
        match self {
            Self::Sz(s) | Self::ExpandSz(s) => utf16(s),
            Self::MultiSz(list) => {
                let mut out: Vec<u8> = list.iter().flat_map(|s| utf16(s)).collect();
                out.extend([0, 0]);
                out
            }
            Self::Dword(n) => n.to_le_bytes().to_vec(),
            Self::Qword(n) => n.to_le_bytes().to_vec(),
            Self::Binary(b) | Self::Other(_, b) => b.clone(),
        }
    }

    /// Decode raw bytes of type `kind`.
    #[must_use]
    pub fn from_raw(kind: u32, bytes: &[u8]) -> Self {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let text = |u: &[u16]| {
            let end = u.iter().position(|&c| c == 0).unwrap_or(u.len());
            String::from_utf16_lossy(&u[..end])
        };
        match kind {
            1 => Self::Sz(text(&units)),
            2 => Self::ExpandSz(text(&units)),
            3 => Self::Binary(bytes.to_vec()),
            4 if bytes.len() >= 4 => {
                Self::Dword(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            7 => Self::MultiSz(
                units
                    .split(|&c| c == 0)
                    .filter(|s| !s.is_empty())
                    .map(String::from_utf16_lossy)
                    .collect(),
            ),
            11 if bytes.len() >= 8 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(&bytes[..8]);
                Self::Qword(u64::from_le_bytes(b))
            }
            other => Self::Other(other, bytes.to_vec()),
        }
    }
}

/// `"…"` with `\` and `"` escaped, as `.reg` files write names and strings.
fn reg_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// One `name=data` line of a `.reg` file.
#[must_use]
pub fn reg_value_line(name: &str, value: &Value) -> String {
    let name = if name.is_empty() {
        "@".to_owned()
    } else {
        reg_quote(name)
    };
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let data = match value {
        Value::Sz(s) if !s.contains(['\r', '\n']) => reg_quote(s),
        Value::Dword(n) => format!("dword:{n:08x}"),
        Value::Binary(b) => format!("hex:{}", hex(b)),
        other => format!("hex({:x}):{}", other.kind(), hex(&other.raw())),
    };
    format!("{name}={data}")
}

/// A `.reg` file that puts back one value of `hive\key` (Windows Registry Editor 5).
#[must_use]
pub fn reg_text_for_value(hive: Hive, key: &str, name: &str, value: &Value) -> String {
    format!(
        "Windows Registry Editor Version 5.00\r\n\r\n[{}\\{key}]\r\n{}\r\n",
        hive.name(),
        reg_value_line(name, value)
    )
}

/// The keys a `.reg` file writes (`[HKEY_…\…]`; deletions `[-…]` included), from
/// its bytes (UTF-16LE with BOM, as `reg export` writes, or UTF-8).
#[must_use]
pub fn reg_file_keys(bytes: &[u8]) -> Vec<String> {
    decode_reg_text(bytes)
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix('[')?.strip_suffix(']'))
        .map(|k| k.trim_start_matches('-').to_owned())
        .collect()
}

/// `(parent, name)` of a key path at least two levels deep (`Software\Acme` →
/// `("Software", "Acme")`); `None` for a top-level key, which is never deleted.
#[must_use]
pub fn parent_and_name(path: &str) -> Option<(&str, &str)> {
    let path = path.trim_matches('\\');
    let (parent, name) = path.rsplit_once('\\')?;
    (!parent.is_empty() && !name.is_empty()).then_some((parent, name))
}

/// The names of the values a `.reg` file writes (`"name"=…`; `@` for the default
/// value), in order.
#[must_use]
pub fn reg_file_value_names(bytes: &[u8]) -> Vec<String> {
    decode_reg_text(bytes)
        .lines()
        .map(str::trim_start)
        .filter_map(|l| {
            if l.starts_with("@=") {
                return Some("@".to_owned());
            }
            let rest = l.strip_prefix('"')?;
            let mut name = String::new();
            let mut chars = rest.chars();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => name.push(chars.next()?),
                    '"' => return chars.as_str().starts_with('=').then_some(name),
                    other => name.push(other),
                }
            }
            None
        })
        .collect()
}

/// The text of a `.reg` file (UTF-16LE with BOM, or UTF-8).
fn decode_reg_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xff, 0xfe]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .into_owned()
    }
}

/// UTF-16LE with a BOM, the encoding `reg import` expects of version-5 files.
#[must_use]
pub fn utf16_file(text: &str) -> Vec<u8> {
    let mut out = vec![0xff, 0xfe];
    out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    out
}

pub use imp::{
    delete_tree, delete_value, exists, export_key, import, is_elevated, last_write, subkeys, value,
    values,
};

/// Write a `.reg` backup of one value to `file`.
///
/// # Errors
/// The value cannot be read or the file written.
pub fn export_value(hive: Hive, key: &str, name: &str, file: &Path) -> io::Result<()> {
    let v = value(hive, key, name)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such value"))?;
    std::fs::write(file, utf16_file(&reg_text_for_value(hive, key, name, &v)))
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;
    use std::ptr;

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS,
        FILETIME, HANDLE,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, KEY_WRITE,
        RegCloseKey, RegDeleteKeyExW, RegDeleteTreeW, RegDeleteValueW, RegEnumKeyExW,
        RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegQueryValueExW,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    use super::{Hive, Value};

    const DELETE: u32 = 0x0001_0000;
    /// No console window for `reg.exe`.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn root(h: Hive) -> HKEY {
        match h {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        }
    }

    fn err(code: u32) -> io::Error {
        io::Error::from_raw_os_error(code as i32)
    }

    /// An open key, closed on drop.
    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the handle came from RegOpenKeyExW and is closed once.
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }

    fn open(hive: Hive, path: &str, access: u32) -> io::Result<Key> {
        let p = wide(path);
        let mut h: HKEY = ptr::null_mut();
        // SAFETY: valid NUL-terminated name and out pointer.
        let rc =
            unsafe { RegOpenKeyExW(root(hive), p.as_ptr(), 0, access | KEY_WOW64_64KEY, &mut h) };
        if rc == ERROR_SUCCESS {
            Ok(Key(h))
        } else {
            Err(err(rc))
        }
    }

    struct Info {
        subkeys: u32,
        max_subkey: u32,
        values: u32,
        max_name: u32,
        max_data: u32,
        written: FILETIME,
    }

    fn info(k: &Key) -> io::Result<Info> {
        let mut i = Info {
            subkeys: 0,
            max_subkey: 0,
            values: 0,
            max_name: 0,
            max_data: 0,
            written: FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
        };
        // SAFETY: every out pointer is valid; unused ones are null.
        let rc = unsafe {
            RegQueryInfoKeyW(
                k.0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
                &mut i.subkeys,
                &mut i.max_subkey,
                ptr::null_mut(),
                &mut i.values,
                &mut i.max_name,
                &mut i.max_data,
                ptr::null_mut(),
                &mut i.written,
            )
        };
        if rc == ERROR_SUCCESS {
            Ok(i)
        } else {
            Err(err(rc))
        }
    }

    /// Names of the subkeys of `hive\path`.
    ///
    /// # Errors
    /// The key cannot be opened (missing: `NotFound`).
    pub fn subkeys(hive: Hive, path: &str) -> io::Result<Vec<String>> {
        let k = open(hive, path, KEY_READ)?;
        let i = info(&k)?;
        let mut out = Vec::with_capacity(i.subkeys as usize);
        let mut buf = vec![0u16; i.max_subkey as usize + 2];
        let mut n = 0;
        loop {
            let mut len = buf.len() as u32;
            // SAFETY: buffer and length describe the same allocation.
            let rc = unsafe {
                RegEnumKeyExW(
                    k.0,
                    n,
                    buf.as_mut_ptr(),
                    &mut len,
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            match rc {
                ERROR_SUCCESS => out.push(String::from_utf16_lossy(&buf[..len as usize])),
                ERROR_MORE_DATA => {
                    buf.resize(buf.len() * 2 + 64, 0);
                    continue;
                }
                ERROR_NO_MORE_ITEMS => break,
                other => return Err(err(other)),
            }
            n += 1;
        }
        Ok(out)
    }

    /// Every value of `hive\path` (`""` = the default value).
    ///
    /// # Errors
    /// The key cannot be opened.
    pub fn values(hive: Hive, path: &str) -> io::Result<Vec<(String, Value)>> {
        let k = open(hive, path, KEY_READ)?;
        let i = info(&k)?;
        let mut out = Vec::with_capacity(i.values as usize);
        let mut name = vec![0u16; i.max_name as usize + 2];
        let mut data = vec![0u8; i.max_data as usize + 8];
        let mut n = 0;
        loop {
            let mut name_len = name.len() as u32;
            let mut data_len = data.len() as u32;
            let mut kind = 0u32;
            // SAFETY: buffers and lengths describe the same allocations.
            let rc = unsafe {
                RegEnumValueW(
                    k.0,
                    n,
                    name.as_mut_ptr(),
                    &mut name_len,
                    ptr::null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut data_len,
                )
            };
            match rc {
                ERROR_SUCCESS => out.push((
                    String::from_utf16_lossy(&name[..name_len as usize]),
                    Value::from_raw(kind, &data[..data_len as usize]),
                )),
                ERROR_MORE_DATA => {
                    name.resize(name.len() * 2 + 64, 0);
                    data.resize(data.len() * 2 + 256, 0);
                    continue;
                }
                ERROR_NO_MORE_ITEMS => break,
                other => return Err(err(other)),
            }
            n += 1;
        }
        Ok(out)
    }

    /// One value of `hive\path`, `None` when the key exists without it.
    ///
    /// # Errors
    /// The key cannot be opened.
    pub fn value(hive: Hive, path: &str, name: &str) -> io::Result<Option<Value>> {
        let k = open(hive, path, KEY_READ)?;
        let n = wide(name);
        let mut kind = 0u32;
        let mut len = 0u32;
        // SAFETY: size query (null data pointer).
        let rc = unsafe {
            RegQueryValueExW(
                k.0,
                n.as_ptr(),
                ptr::null(),
                &mut kind,
                ptr::null_mut(),
                &mut len,
            )
        };
        if rc == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if rc != ERROR_SUCCESS {
            return Err(err(rc));
        }
        let mut data = vec![0u8; len as usize + 2];
        let mut len = data.len() as u32;
        // SAFETY: buffer and length describe the same allocation.
        let rc = unsafe {
            RegQueryValueExW(
                k.0,
                n.as_ptr(),
                ptr::null(),
                &mut kind,
                data.as_mut_ptr(),
                &mut len,
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(err(rc));
        }
        Ok(Some(Value::from_raw(kind, &data[..len as usize])))
    }

    /// Last write of `hive\path`, Unix seconds.
    ///
    /// # Errors
    /// The key cannot be opened.
    pub fn last_write(hive: Hive, path: &str) -> io::Result<i64> {
        let k = open(hive, path, KEY_READ)?;
        let ft = info(&k)?.written;
        let t = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
        Ok((t / 10_000_000) as i64 - 11_644_473_600)
    }

    /// `hive\path` exists.
    #[must_use]
    pub fn exists(hive: Hive, path: &str) -> bool {
        open(hive, path, KEY_READ).is_ok()
    }

    /// Delete `hive\path` with everything below it.
    ///
    /// # Errors
    /// A root key, or the key cannot be deleted (`PermissionDenied` without rights).
    pub fn delete_tree(hive: Hive, path: &str) -> io::Result<()> {
        let Some((parent, child)) = super::parent_and_name(path) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a top-level key is never deleted",
            ));
        };
        let p = open(hive, parent, KEY_READ | KEY_WRITE | DELETE)?;
        let c = wide(child);
        // SAFETY: open parent handle and NUL-terminated subkey name.
        let rc = unsafe { RegDeleteTreeW(p.0, c.as_ptr()) };
        if rc != ERROR_SUCCESS && rc != ERROR_FILE_NOT_FOUND {
            return Err(err(rc));
        }
        // RegDeleteTreeW empties the key; remove the key itself if still there
        // SAFETY: as above.
        let rc = unsafe { RegDeleteKeyExW(p.0, c.as_ptr(), KEY_WOW64_64KEY, 0) };
        if rc != ERROR_SUCCESS && rc != ERROR_FILE_NOT_FOUND {
            return Err(err(rc));
        }
        Ok(())
    }

    /// Delete one value of `hive\path`.
    ///
    /// # Errors
    /// The value cannot be deleted.
    pub fn delete_value(hive: Hive, path: &str, name: &str) -> io::Result<()> {
        let k = open(hive, path, KEY_READ | KEY_WRITE)?;
        let n = wide(name);
        // SAFETY: open handle and NUL-terminated value name.
        let rc = unsafe { RegDeleteValueW(k.0, n.as_ptr()) };
        if rc == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(err(rc))
        }
    }

    fn reg_exe() -> std::path::PathBuf {
        let sys = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        Path::new(&sys).join("System32").join("reg.exe")
    }

    fn run_reg(args: &[&std::ffi::OsStr]) -> io::Result<()> {
        let out = Command::new(reg_exe())
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        if out.status.success() {
            Ok(())
        } else {
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            Err(io::Error::other(if msg.is_empty() {
                format!("reg.exe failed ({})", out.status)
            } else {
                msg
            }))
        }
    }

    /// Back up `hive\path` with everything below it to `file` (`reg export /reg:64`).
    ///
    /// # Errors
    /// `reg.exe` failed (missing key, no rights, disk full).
    pub fn export_key(hive: Hive, path: &str, file: &Path) -> io::Result<()> {
        let key = format!("{}\\{}", hive.name(), path.trim_matches('\\'));
        run_reg(&[
            "export".as_ref(),
            key.as_ref(),
            file.as_os_str(),
            "/y".as_ref(),
            "/reg:64".as_ref(),
        ])
    }

    /// Put a `.reg` backup back (`reg import /reg:64`). A file that writes into
    /// `HKEY_LOCAL_MACHINE` needs an elevated process: `PermissionDenied` otherwise.
    ///
    /// # Errors
    /// No rights, or `reg.exe` failed.
    pub fn import(file: &Path) -> io::Result<()> {
        let keys = super::reg_file_keys(&std::fs::read(file)?);
        if keys.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a registry backup",
            ));
        }
        let machine = keys
            .iter()
            .any(|k| matches!(Hive::split(k), Some((Hive::LocalMachine, _))));
        if machine && !is_elevated() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        run_reg(&["import".as_ref(), file.as_os_str(), "/reg:64".as_ref()])
    }

    /// This process runs elevated (administrator rights in effect).
    #[must_use]
    pub fn is_elevated() -> bool {
        let mut token: HANDLE = ptr::null_mut();
        // SAFETY: the pseudo handle needs no closing; `token` is closed below.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return false;
        }
        let mut e = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        // SAFETY: the buffer is a TOKEN_ELEVATION of the size given.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                (&raw mut e).cast(),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut len,
            )
        };
        // SAFETY: opened above.
        unsafe {
            CloseHandle(token);
        }
        ok != 0 && e.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::Path;

    use super::{Hive, Value};

    fn none() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "no registry on this platform")
    }

    pub fn subkeys(_: Hive, _: &str) -> io::Result<Vec<String>> {
        Err(none())
    }
    pub fn values(_: Hive, _: &str) -> io::Result<Vec<(String, Value)>> {
        Err(none())
    }
    pub fn value(_: Hive, _: &str, _: &str) -> io::Result<Option<Value>> {
        Err(none())
    }
    pub fn last_write(_: Hive, _: &str) -> io::Result<i64> {
        Err(none())
    }
    #[must_use]
    pub fn exists(_: Hive, _: &str) -> bool {
        false
    }
    pub fn delete_tree(_: Hive, _: &str) -> io::Result<()> {
        Err(none())
    }
    pub fn delete_value(_: Hive, _: &str, _: &str) -> io::Result<()> {
        Err(none())
    }
    pub fn export_key(_: Hive, _: &str, _: &Path) -> io::Result<()> {
        Err(none())
    }
    pub fn import(_: &Path) -> io::Result<()> {
        Err(none())
    }
    #[must_use]
    pub fn is_elevated() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_lines_escape_and_encode() {
        assert_eq!(
            reg_value_line("Tool", &Value::Sz(r#"C:\A "b"\c.exe"#.into())),
            r#""Tool"="C:\\A \"b\"\\c.exe""#
        );
        assert_eq!(reg_value_line("", &Value::Dword(0x1f)), "@=dword:0000001f");
        assert_eq!(
            reg_value_line("X", &Value::ExpandSz("%A%".into())),
            "\"X\"=hex(2):25,00,41,00,25,00,00,00"
        );
        assert_eq!(
            reg_value_line("M", &Value::MultiSz(vec!["a".into(), "b".into()])),
            "\"M\"=hex(7):61,00,00,00,62,00,00,00,00,00"
        );
        assert_eq!(
            reg_value_line("B", &Value::Binary(vec![1, 255])),
            "\"B\"=hex:01,ff"
        );
        assert_eq!(
            reg_value_line("Q", &Value::Qword(1)),
            "\"Q\"=hex(b):01,00,00,00,00,00,00,00"
        );
        // a string with a line break cannot be quoted: written as hex(1)
        assert!(reg_value_line("N", &Value::Sz("a\nb".into())).starts_with("\"N\"=hex(1):"));
    }

    #[test]
    fn raw_round_trips() {
        for v in [
            Value::Sz("héllo".into()),
            Value::ExpandSz("%TEMP%\\x".into()),
            Value::MultiSz(vec!["one".into(), "two".into()]),
            Value::Dword(7),
            Value::Qword(1 << 40),
            Value::Binary(vec![0, 1, 2]),
        ] {
            assert_eq!(Value::from_raw(v.kind(), &v.raw()), v);
        }
    }

    #[test]
    fn backup_files_name_their_keys() {
        let text = reg_text_for_value(
            Hive::CurrentUser,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            "Tool",
            &Value::Sz("x".into()),
        );
        let keys = reg_file_keys(&utf16_file(&text));
        assert_eq!(
            reg_file_value_names(&utf16_file(&text)),
            vec!["Tool".to_owned()]
        );
        assert_eq!(
            keys,
            vec![r"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run".to_owned()]
        );
        let utf8 = b"\xef\xbb\xbfWindows Registry Editor Version 5.00\r\n\r\n[HKEY_LOCAL_MACHINE\\SOFTWARE\\Acme]\r\n[-HKEY_CURRENT_USER\\Software\\Old]\r\n";
        assert_eq!(
            reg_file_keys(utf8),
            vec![
                r"HKEY_LOCAL_MACHINE\SOFTWARE\Acme".to_owned(),
                r"HKEY_CURRENT_USER\Software\Old".to_owned()
            ]
        );
    }

    #[test]
    fn top_level_keys_have_no_parent() {
        assert_eq!(parent_and_name("Software"), None);
        assert_eq!(parent_and_name(r"\Software\"), None);
        assert_eq!(
            parent_and_name(r"Software\Acme\"),
            Some(("Software", "Acme"))
        );
        assert_eq!(
            parent_and_name(r"SOFTWARE\WOW6432Node\Acme Tool"),
            Some((r"SOFTWARE\WOW6432Node", "Acme Tool"))
        );
    }

    #[test]
    fn hives_parse() {
        assert_eq!(
            Hive::split(r"HKLM\SOFTWARE\X"),
            Some((Hive::LocalMachine, r"SOFTWARE\X"))
        );
        assert_eq!(
            Hive::split(r"hkey_current_user\Software"),
            Some((Hive::CurrentUser, "Software"))
        );
        assert_eq!(Hive::split(r"HKCR\x"), None);
    }

    #[cfg(windows)]
    #[test]
    fn reads_a_known_key() {
        let names = subkeys(
            Hive::LocalMachine,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion",
        )
        .expect("CurrentVersion");
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("Uninstall")));
        let v = value(
            Hive::LocalMachine,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "CurrentBuild",
        )
        .expect("read");
        assert!(v.and_then(|v| v.as_u64()).is_some());
        assert!(last_write(Hive::LocalMachine, r"SOFTWARE\Microsoft").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn backs_up_deletes_and_restores_a_user_key() {
        let base = format!(r"Software\NANOFreeDiskTest\reg-{}", std::process::id());
        let dir = tempfile::tempdir().expect("tmp");
        // create the key through reg.exe import of our own writer's output
        let file = dir.path().join("seed.reg");
        std::fs::write(
            &file,
            utf16_file(&reg_text_for_value(
                Hive::CurrentUser,
                &format!(r"{base}\Sub"),
                "Path",
                &Value::Sz(r"C:\Gone\app.exe".into()),
            )),
        )
        .expect("write");
        import(&file).expect("seed");
        assert!(exists(Hive::CurrentUser, &format!(r"{base}\Sub")));
        let backup = dir.path().join("backup.reg");
        export_key(Hive::CurrentUser, &base, &backup).expect("export");
        delete_tree(Hive::CurrentUser, &base).expect("delete");
        assert!(!exists(Hive::CurrentUser, &base));
        import(&backup).expect("restore");
        assert_eq!(
            value(Hive::CurrentUser, &format!(r"{base}\Sub"), "Path").expect("read"),
            Some(Value::Sz(r"C:\Gone\app.exe".into()))
        );
        // one value: backed up, removed, put back
        let vb = dir.path().join("value.reg");
        export_value(Hive::CurrentUser, &format!(r"{base}\Sub"), "Path", &vb).expect("value");
        delete_value(Hive::CurrentUser, &format!(r"{base}\Sub"), "Path").expect("del value");
        assert_eq!(
            value(Hive::CurrentUser, &format!(r"{base}\Sub"), "Path").expect("read"),
            None
        );
        import(&vb).expect("restore value");
        assert!(
            value(Hive::CurrentUser, &format!(r"{base}\Sub"), "Path")
                .expect("read")
                .is_some()
        );
        delete_tree(Hive::CurrentUser, r"Software\NANOFreeDiskTest").expect("cleanup");
    }
}
