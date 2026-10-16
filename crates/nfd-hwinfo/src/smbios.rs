//! SMBIOS table parser (DMTF DSP0134). Portable: it works on the raw bytes Windows
//! returns from `GetSystemFirmwareTable('RSMB')`, so it is tested with fixture blobs
//! on every platform.
//!
//! Only the structures the Overview needs are decoded: BIOS (0), system (1),
//! baseboard (2), chassis (3), processor (4), memory array (16) and memory
//! device (17). Serial numbers and UUIDs are deliberately never read — the
//! inventory is meant to be pasted into support forums.

use serde::{Deserialize, Serialize};

/// One structure: its type, the formatted area (header included) and its strings.
#[derive(Debug, Clone)]
pub struct Structure<'a> {
    pub kind: u8,
    pub handle: u16,
    pub data: &'a [u8],
    strings: Vec<&'a [u8]>,
}

impl Structure<'_> {
    pub fn byte(&self, off: usize) -> Option<u8> {
        self.data.get(off).copied()
    }

    pub fn word(&self, off: usize) -> Option<u16> {
        let b = self.data.get(off..off + 2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn dword(&self, off: usize) -> Option<u32> {
        let b = self.data.get(off..off + 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// The string a 1-based index byte at `off` points to; `None` for 0, a missing
    /// string or a placeholder ("To Be Filled By O.E.M.", "Default string"…).
    pub fn string(&self, off: usize) -> Option<String> {
        let idx = self.byte(off)? as usize;
        if idx == 0 {
            return None;
        }
        let raw = self.strings.get(idx - 1)?;
        let s = String::from_utf8_lossy(raw).trim().to_owned();
        (!is_placeholder(&s)).then_some(s)
    }
}

/// Values OEMs leave in unused fields.
fn is_placeholder(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    s.is_empty()
        || l.contains("to be filled")
        || l == "default string"
        || l == "not specified"
        || l == "none"
        || l == "unknown"
        || l == "system product name"
        || l == "system manufacturer"
        || l == "o.e.m."
        || l.chars().all(|c| c == '0' || c == ' ')
}

/// Split the table into structures. `table` is the SMBIOS data without Windows'
/// 8-byte `RawSMBIOSData` header (see [`parse_raw`]). Stops at type 127 or at the
/// first malformed structure.
pub fn structures(table: &[u8]) -> Vec<Structure<'_>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos + 4 <= table.len() {
        let kind = table[pos];
        let len = table[pos + 1] as usize;
        if len < 4 || pos + len > table.len() {
            break;
        }
        let handle = u16::from_le_bytes([table[pos + 2], table[pos + 3]]);
        let data = &table[pos..pos + len];
        // strings: NUL-terminated, the set ends with an extra NUL
        let mut s = pos + len;
        let mut strings = Vec::new();
        if table.get(s..s + 2) == Some(&[0, 0]) {
            s += 2;
        } else {
            loop {
                let Some(end) = table[s..].iter().position(|&b| b == 0) else {
                    return out;
                };
                strings.push(&table[s..s + end]);
                s += end + 1;
                if table.get(s) == Some(&0) {
                    s += 1;
                    break;
                }
                if s >= table.len() {
                    break;
                }
            }
        }
        out.push(Structure {
            kind,
            handle,
            data,
            strings,
        });
        if kind == 127 {
            break;
        }
        pos = s;
    }
    out
}

/// Version and table of a Windows `RawSMBIOSData` blob.
pub fn parse_raw(blob: &[u8]) -> Option<(u8, u8, &[u8])> {
    if blob.len() < 8 {
        return None;
    }
    let (major, minor) = (blob[1], blob[2]);
    let len = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]) as usize;
    let table = blob.get(8..8 + len.min(blob.len() - 8))?;
    Some((major, minor, table))
}

/// What the Overview takes from SMBIOS.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Firmware {
    pub version: String,
    pub bios_vendor: Option<String>,
    pub bios_version: Option<String>,
    pub bios_date: Option<String>,
    pub system_manufacturer: Option<String>,
    pub system_product: Option<String>,
    pub system_version: Option<String>,
    pub system_family: Option<String>,
    pub system_sku: Option<String>,
    pub board_manufacturer: Option<String>,
    pub board_product: Option<String>,
    pub board_version: Option<String>,
    pub chassis: Option<String>,
    pub cpu_socket: Option<String>,
    /// MHz, as the firmware reports them (often the base clock).
    pub cpu_current_mhz: Option<u32>,
    pub cpu_max_mhz: Option<u32>,
    pub cpu_external_mhz: Option<u32>,
    /// Number of memory device slots (sum of type-16 arrays).
    pub memory_slots: Option<u32>,
    /// Maximum installable memory in bytes.
    pub memory_max: Option<u64>,
    pub modules: Vec<MemoryModule>,
}

/// One populated memory slot (type 17).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryModule {
    pub locator: Option<String>,
    pub bank: Option<String>,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    /// Bytes.
    pub size: u64,
    /// "DDR4", "DDR5", "LPDDR5"…
    pub kind: Option<String>,
    /// "DIMM", "SODIMM"…
    pub form_factor: Option<String>,
    /// Rated speed in MT/s.
    pub speed_mts: Option<u32>,
    /// Speed the memory controller runs at, MT/s.
    pub configured_mts: Option<u32>,
    pub rank: Option<u8>,
}

/// Decode the structures the Overview uses.
pub fn decode(major: u8, minor: u8, table: &[u8]) -> Firmware {
    let mut fw = Firmware {
        version: format!("{major}.{minor}"),
        ..Firmware::default()
    };
    let mut slots = 0u32;
    let mut max = 0u64;
    let mut cpu_seen = false;
    for s in structures(table) {
        match s.kind {
            0 => {
                fw.bios_vendor = s.string(0x04);
                fw.bios_version = s.string(0x05);
                fw.bios_date = s.string(0x08);
            }
            1 => {
                fw.system_manufacturer = s.string(0x04);
                fw.system_product = s.string(0x05);
                fw.system_version = s.string(0x06);
                fw.system_sku = s.string(0x19);
                fw.system_family = s.string(0x1A);
            }
            2 if fw.board_product.is_none() => {
                fw.board_manufacturer = s.string(0x04);
                fw.board_product = s.string(0x05);
                fw.board_version = s.string(0x06);
            }
            3 if fw.chassis.is_none() => {
                fw.chassis = s.byte(0x05).and_then(|b| chassis_name(b & 0x7f));
            }
            4 if !cpu_seen => {
                // first populated socket (bit 6 of status)
                if s.byte(0x18).is_some_and(|st| st & 0x40 != 0) {
                    cpu_seen = true;
                    fw.cpu_socket = s.string(0x04);
                    fw.cpu_external_mhz = s.word(0x12).filter(|&v| v > 0).map(u32::from);
                    fw.cpu_max_mhz = s.word(0x14).filter(|&v| v > 0).map(u32::from);
                    fw.cpu_current_mhz = s.word(0x16).filter(|&v| v > 0).map(u32::from);
                }
            }
            16 => {
                // only system memory (use 0x03)
                if s.byte(0x05) == Some(0x03) {
                    slots += u32::from(s.word(0x0D).unwrap_or(0));
                    let kb = s.dword(0x07).unwrap_or(0);
                    max += if kb == 0x8000_0000 {
                        s.data
                            .get(0x0F..0x17)
                            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
                            .unwrap_or(0)
                    } else {
                        u64::from(kb) * 1024
                    };
                }
            }
            17 => {
                if let Some(m) = module(&s) {
                    fw.modules.push(m);
                }
            }
            _ => {}
        }
    }
    fw.memory_slots = (slots > 0).then_some(slots);
    fw.memory_max = (max > 0).then_some(max);
    fw
}

fn module(s: &Structure<'_>) -> Option<MemoryModule> {
    let raw = s.word(0x0C)?;
    let size = match raw {
        0 | 0xFFFF => return None, // empty or unknown
        0x7FFF => u64::from(s.dword(0x1C)? & 0x7FFF_FFFF) << 20,
        v if v & 0x8000 != 0 => u64::from(v & 0x7FFF) << 10,
        v => u64::from(v) << 20,
    };
    let speed = |off: usize, ext: usize| -> Option<u32> {
        match s.word(off)? {
            0 => None,
            0xFFFF => s.dword(ext).filter(|&v| v > 0),
            v => Some(u32::from(v)),
        }
    };
    Some(MemoryModule {
        locator: s.string(0x10),
        bank: s.string(0x11),
        manufacturer: s
            .string(0x17)
            .map(|m| jedec_vendor(&m))
            .filter(|m| !m.to_ascii_lowercase().starts_with("unknown")),
        part_number: s.string(0x1A),
        size,
        kind: s.byte(0x12).and_then(memory_type),
        form_factor: s.byte(0x0E).and_then(form_factor),
        speed_mts: speed(0x15, 0x54),
        configured_mts: speed(0x20, 0x58),
        rank: s.byte(0x1B).map(|b| b & 0x0F).filter(|&r| r > 0),
    })
}

/// Some firmware puts the JEDEC id ("80CE", "Unknown (2C00)") instead of a name.
fn jedec_vendor(s: &str) -> String {
    let hex: String = s.chars().filter(char::is_ascii_hexdigit).collect();
    let id = hex.get(..4).map(str::to_ascii_uppercase);
    let name = match id.as_deref() {
        Some("80CE" | "CE00") => "Samsung",
        Some("80AD" | "AD00") => "SK hynix",
        Some("802C" | "2C00") => "Micron",
        Some("859B" | "9B05") => "Crucial",
        Some("04CB" | "CB04") => "A-DATA",
        Some("04CD" | "CD04") => "G.Skill",
        Some("0198" | "9801") => "Kingston",
        Some("029E" | "9E02") => "Corsair",
        _ => return s.to_owned(),
    };
    // only replace when the field is just the id
    if s.len() <= 16
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() || c == ' ' || c == '(' || c == ')')
        || s.to_ascii_lowercase().starts_with("unknown")
    {
        name.to_owned()
    } else {
        s.to_owned()
    }
}

fn memory_type(b: u8) -> Option<String> {
    Some(
        match b {
            0x12 => "DDR",
            0x13 => "DDR2",
            0x18 => "DDR3",
            0x1A => "DDR4",
            0x1B => "LPDDR",
            0x1C => "LPDDR2",
            0x1D => "LPDDR3",
            0x1E => "LPDDR4",
            0x20 => "HBM",
            0x21 => "HBM2",
            0x22 => "DDR5",
            0x23 => "LPDDR5",
            0x24 => "HBM3",
            _ => return None,
        }
        .to_owned(),
    )
}

fn form_factor(b: u8) -> Option<String> {
    Some(
        match b {
            0x09 => "DIMM",
            0x0D => "SODIMM",
            0x0F => "FB-DIMM",
            0x10 => "Die",
            0x11 => "CAMM",
            _ => return None,
        }
        .to_owned(),
    )
}

fn chassis_name(b: u8) -> Option<String> {
    Some(
        match b {
            0x03 | 0x04 | 0x06 | 0x07 | 0x0D | 0x23 | 0x24 => "desktop",
            0x05 | 0x10 | 0x11 | 0x17 | 0x1C | 0x1D => "server",
            0x08..=0x0A | 0x0E | 0x1F | 0x20 => "laptop",
            0x1E => "tablet",
            0x0B => "handheld",
            _ => return None,
        }
        .to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_dropped() {
        assert!(is_placeholder("To Be Filled By O.E.M."));
        assert!(is_placeholder("Default string"));
        assert!(is_placeholder("0000000"));
        assert!(!is_placeholder("Adder WS"));
    }

    #[test]
    fn jedec_ids_become_names() {
        assert_eq!(jedec_vendor("80CE"), "Samsung");
        assert_eq!(jedec_vendor("Unknown (802C)"), "Micron");
        assert_eq!(jedec_vendor("Kingston"), "Kingston");
    }

    #[test]
    fn truncated_tables_do_not_panic() {
        assert!(structures(&[]).is_empty());
        assert!(structures(&[1, 30, 0]).is_empty());
        assert!(structures(&[1, 2, 0, 0]).is_empty());
        let s = structures(&[0, 4, 0, 0, b'a', 0]);
        assert_eq!(s.len(), 1);
        assert!(parse_raw(&[0; 4]).is_none());
    }
}
