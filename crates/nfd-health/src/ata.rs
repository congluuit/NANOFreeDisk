//! ATA SMART attribute table (SMART READ DATA, 512 bytes) and thresholds (SMART
//! READ THRESHOLDS). Both are 30 entries of 12 bytes from offset 2; the layout is
//! the de-facto standard every tool reads (ATA-8 left it vendor specific).

use serde::{Deserialize, Serialize};

/// Size of a SMART data / threshold sector.
pub const SECTOR: usize = 512;
const ENTRIES: usize = 30;

/// One SMART attribute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribute {
    pub id: u8,
    pub flags: u16,
    /// Normalized current value (higher is better, usually 1–253).
    pub current: u8,
    pub worst: u8,
    /// Failure threshold; `None` when the thresholds could not be read.
    pub threshold: Option<u8>,
    /// 48-bit raw value.
    pub raw: u64,
    /// Standard English name (a technical identifier, shown as is in every language,
    /// as CrystalDiskInfo and smartctl do); `None` for vendor-specific ids.
    #[serde(default)]
    pub name: Option<String>,
}

impl Attribute {
    /// The pre-fail flag (bit 0): falling to the threshold predicts failure.
    #[must_use]
    pub fn prefail(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Below its failure threshold (CrystalDiskInfo: threshold ≠ 0 and current <
    /// threshold; smartctl uses ≤).
    #[must_use]
    pub fn failing(&self) -> bool {
        matches!(self.threshold, Some(t) if t != 0 && self.current != 0 && self.current < t)
    }
}

/// Names of the attributes the app interprets or that are common enough to show.
#[must_use]
pub fn name(id: u8) -> Option<&'static str> {
    Some(match id {
        0x01 => "Read Error Rate",
        0x02 => "Throughput Performance",
        0x03 => "Spin-Up Time",
        0x04 => "Start/Stop Count",
        0x05 => "Reallocated Sectors Count",
        0x07 => "Seek Error Rate",
        0x08 => "Seek Time Performance",
        0x09 => "Power-On Hours",
        0x0A => "Spin Retry Count",
        0x0B => "Recalibration Retries",
        0x0C => "Power Cycle Count",
        0xA9 => "Remaining Life",
        0xAA => "Available Reserved Space",
        0xAB => "Program Fail Count",
        0xAC => "Erase Fail Count",
        0xAD => "Wear Leveling Count",
        0xAE => "Unexpected Power Loss Count",
        0xB1 => "Wear Range Delta / Wear Leveling Count",
        0xB3 => "Used Reserved Block Count",
        0xB5 => "Program Fail Count",
        0xB6 => "Erase Fail Count",
        0xB7 => "SATA Downshift Error Count",
        0xB8 => "End-to-End Error",
        0xBB => "Reported Uncorrectable Errors",
        0xBC => "Command Timeout",
        0xBD => "High Fly Writes",
        0xBE => "Airflow Temperature",
        0xBF => "G-Sense Error Rate",
        0xC0 => "Power-off Retract Count",
        0xC1 => "Load/Unload Cycle Count",
        0xC2 => "Temperature",
        0xC3 => "Hardware ECC Recovered",
        0xC4 => "Reallocation Event Count",
        0xC5 => "Current Pending Sector Count",
        0xC6 => "Uncorrectable Sector Count",
        0xC7 => "UltraDMA CRC Error Count",
        0xC8 => "Write Error Rate",
        0xCA => "Percent Lifetime Remaining",
        0xDC => "Disk Shift",
        0xE7 => "SSD Life Left",
        0xE8 => "Available Reserved Space",
        0xE9 => "Media Wearout Indicator",
        0xF0 => "Head Flying Hours",
        0xF1 => "Total Host Writes",
        0xF2 => "Total Host Reads",
        _ => return None,
    })
}

/// Parse a SMART READ DATA sector, joined with a SMART READ THRESHOLDS sector when
/// there is one. Empty slots (id 0) are skipped.
#[must_use]
pub fn parse(data: &[u8], thresholds: Option<&[u8]>) -> Vec<Attribute> {
    if data.len() < SECTOR {
        return Vec::new();
    }
    let limits: Vec<(u8, u8)> = thresholds
        .filter(|t| t.len() >= SECTOR)
        .map(|t| {
            (0..ENTRIES)
                .map(|i| (t[2 + 12 * i], t[3 + 12 * i]))
                .filter(|&(id, _)| id != 0)
                .collect()
        })
        .unwrap_or_default();
    let known = thresholds.is_some_and(|t| t.len() >= SECTOR);
    (0..ENTRIES)
        .filter_map(|i| {
            let e = &data[2 + 12 * i..2 + 12 * (i + 1)];
            if e[0] == 0 {
                return None;
            }
            let mut raw = [0u8; 8];
            raw[..6].copy_from_slice(&e[5..11]);
            Some(Attribute {
                id: e[0],
                flags: u16::from_le_bytes([e[1], e[2]]),
                current: e[3],
                worst: e[4],
                threshold: if known {
                    Some(limits.iter().find(|l| l.0 == e[0]).map_or(0, |l| l.1))
                } else {
                    None
                },
                raw: u64::from_le_bytes(raw),
                name: name(e[0]).map(str::to_owned),
            })
        })
        .collect()
}

/// What the attribute table says in plain numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Derived {
    pub temperature: Option<i32>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    pub bytes_written: Option<u64>,
    pub bytes_read: Option<u64>,
    /// Remaining life in percent (SSD).
    pub life_left: Option<u8>,
}

fn find(attrs: &[Attribute], id: u8) -> Option<&Attribute> {
    attrs.iter().find(|a| a.id == id)
}

/// Life-left attributes in order of preference: their normalized value is the
/// remaining life in percent (Intel / Samsung / Kingston / Micron / SanDisk…).
const LIFE: [u8; 5] = [0xE7, 0xE9, 0xB1, 0xA9, 0xAD];

/// Read the common values. `ssd` turns on the life estimate.
#[must_use]
pub fn derive(attrs: &[Attribute], ssd: bool) -> Derived {
    let temperature = [0xC2, 0xBE].iter().find_map(|&id| {
        let t = i32::from((find(attrs, id)?.raw & 0xFF) as u8);
        (1..100).contains(&t).then_some(t)
    });
    // Power-On Hours: low 32 bits (some drives keep minutes in the upper bytes)
    let power_on_hours = find(attrs, 0x09).map(|a| a.raw & 0xFFFF_FFFF);
    let power_cycles = find(attrs, 0x0C).map(|a| a.raw & 0xFFFF_FFFF);
    // Total LBAs Written / Read in 512-byte sectors (the common unit)
    let bytes_written = find(attrs, 0xF1)
        .filter(|a| a.raw > 0)
        .map(|a| a.raw.saturating_mul(512));
    let bytes_read = find(attrs, 0xF2)
        .filter(|a| a.raw > 0)
        .map(|a| a.raw.saturating_mul(512));
    let life_left = if ssd {
        LIFE.iter().find_map(|&id| {
            find(attrs, id)
                .filter(|a| a.current <= 100)
                .map(|a| a.current)
        })
    } else {
        None
    };
    Derived {
        temperature,
        power_on_hours,
        power_cycles,
        bytes_written,
        bytes_read,
        life_left,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// (id, flags, current, worst, raw, threshold): one table row.
    pub(crate) type Row = (u8, u16, u8, u8, u64, u8);

    /// Build SMART data + threshold sectors in the on-the-wire layout.
    pub(crate) fn sectors(rows: &[Row]) -> (Vec<u8>, Vec<u8>) {
        let mut data = vec![0u8; SECTOR];
        let mut thr = vec![0u8; SECTOR];
        data[0] = 0x10; // revision
        thr[0] = 0x10;
        for (i, &(id, flags, cur, worst, raw, t)) in rows.iter().enumerate() {
            let o = 2 + 12 * i;
            data[o] = id;
            data[o + 1..o + 3].copy_from_slice(&flags.to_le_bytes());
            data[o + 3] = cur;
            data[o + 4] = worst;
            data[o + 5..o + 11].copy_from_slice(&raw.to_le_bytes()[..6]);
            thr[o] = id;
            thr[o + 1] = t;
        }
        (data, thr)
    }

    /// A 2 TB desktop hard disk with 8 pending sectors (CrystalDiskInfo: Caution).
    pub(crate) fn hdd_pending() -> Vec<Row> {
        vec![
            (0x01, 0x2F, 200, 200, 0, 51),
            (0x03, 0x27, 172, 170, 4_383, 21),
            (0x04, 0x32, 100, 100, 1_012, 0),
            (0x05, 0x33, 200, 200, 0, 140),
            (0x07, 0x2E, 200, 200, 0, 0),
            (0x09, 0x32, 54, 54, 33_712, 0),
            (0x0A, 0x32, 100, 100, 0, 0),
            (0x0C, 0x32, 100, 100, 1_007, 0),
            (0xC0, 0x32, 200, 200, 61, 0),
            (0xC1, 0x32, 200, 200, 1_284, 0),
            (0xC2, 0x22, 114, 101, 0x0000_2D00_1A00_0021, 0), // 33 °C, min/max packed above
            (0xC4, 0x32, 200, 200, 0, 0),
            (0xC5, 0x32, 200, 200, 8, 0),
            (0xC6, 0x30, 100, 253, 0, 0),
            (0xC7, 0x32, 200, 200, 0, 0),
            (0xC8, 0x08, 100, 253, 0, 0),
        ]
    }

    /// A SATA SSD at 99 % life (Samsung-style table; CrystalDiskInfo: Good 99 %).
    pub(crate) fn ssd_good() -> Vec<Row> {
        vec![
            (0x05, 0x33, 100, 100, 0, 10),
            (0x09, 0x32, 98, 98, 6_154, 0),
            (0x0C, 0x32, 99, 99, 1_402, 0),
            (0xB1, 0x13, 99, 99, 9, 0),
            (0xB3, 0x13, 100, 100, 0, 10),
            (0xB5, 0x32, 100, 100, 0, 10),
            (0xB6, 0x32, 100, 100, 0, 10),
            (0xB7, 0x13, 100, 100, 0, 10),
            (0xBB, 0x32, 100, 100, 0, 0),
            (0xBE, 0x32, 66, 49, 34, 0),
            (0xC3, 0x1A, 200, 200, 0, 0),
            (0xC7, 0x3E, 100, 100, 0, 0),
            (0xEB, 0x12, 99, 99, 117, 0),
            (0xF1, 0x32, 99, 99, 17_210_524_811, 0),
        ]
    }

    /// A hard disk whose reallocated-sector count fell below its threshold
    /// (CrystalDiskInfo: Bad).
    pub(crate) fn hdd_failing() -> Vec<Row> {
        vec![
            (0x01, 0x0F, 81, 63, 147_331_032, 6),
            (0x05, 0x33, 3, 3, 3_912, 10),
            (0x09, 0x32, 41, 41, 52_011, 0),
            (0x0C, 0x32, 97, 97, 3_311, 20),
            (0xC2, 0x22, 38, 51, 38, 0),
            (0xC5, 0x12, 100, 100, 24, 0),
            (0xC6, 0x10, 100, 100, 24, 0),
        ]
    }

    #[test]
    fn parses_the_table_with_thresholds() {
        let (data, thr) = sectors(&hdd_pending());
        let attrs = parse(&data, Some(&thr));
        assert_eq!(attrs.len(), 16);
        let poh = attrs.iter().find(|a| a.id == 0x09).expect("poh");
        assert_eq!((poh.current, poh.worst, poh.raw), (54, 54, 33_712));
        assert_eq!(attrs[0].threshold, Some(51));
        assert!(attrs[0].prefail());
        assert_eq!(attrs[0].name.as_deref(), Some("Read Error Rate"));
        let d = derive(&attrs, false);
        assert_eq!(d.temperature, Some(33));
        assert_eq!(d.power_on_hours, Some(33_712));
        assert_eq!(d.power_cycles, Some(1_007));
        assert_eq!(d.life_left, None);
    }

    #[test]
    fn without_thresholds_none_is_failing() {
        let (data, _) = sectors(&hdd_failing());
        let attrs = parse(&data, None);
        assert!(attrs.iter().all(|a| a.threshold.is_none() && !a.failing()));
        let (data, thr) = sectors(&hdd_failing());
        let failing: Vec<u8> = parse(&data, Some(&thr))
            .iter()
            .filter(|a| a.failing())
            .map(|a| a.id)
            .collect();
        assert_eq!(failing, vec![0x05]);
    }

    #[test]
    fn ssd_values() {
        let (data, thr) = sectors(&ssd_good());
        let d = derive(&parse(&data, Some(&thr)), true);
        assert_eq!(d.life_left, Some(99));
        assert_eq!(d.temperature, Some(34));
        assert_eq!(d.bytes_written, Some(17_210_524_811 * 512));
        assert_eq!(parse(&data[..100], None), Vec::new());
    }
}
