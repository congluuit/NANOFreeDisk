//! The only ATA commands NANOFreeDisk ever sends, as fixed byte layouts: SMART READ
//! DATA and SMART READ THRESHOLDS (both read one 512-byte sector from the drive and
//! change nothing). There is deliberately no way to build any other command — no
//! SMART ENABLE / DISABLE, no self-test, no attribute autosave.

/// Which of the two read commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartRead {
    /// SMART READ DATA (feature D0h): the attribute table.
    Data,
    /// SMART READ THRESHOLDS (feature D1h): the failure thresholds.
    Thresholds,
}

impl SmartRead {
    #[must_use]
    pub fn feature(self) -> u8 {
        match self {
            Self::Data => 0xD0,
            Self::Thresholds => 0xD1,
        }
    }
}

/// ATA command SMART (B0h); the magic LBA mid/high values select the SMART feature set.
pub const SMART: u8 = 0xB0;
pub const LBA_MID: u8 = 0x4F;
pub const LBA_HIGH: u8 = 0xC2;

/// `IDEREGS` for `SMART_RCV_DRIVE_DATA`: features, sector count, sector number, cyl
/// low, cyl high, drive/head, command, reserved.
#[must_use]
pub fn ide_regs(read: SmartRead, drive: u8) -> [u8; 8] {
    [
        read.feature(),
        1,
        1,
        LBA_MID,
        LBA_HIGH,
        0xA0 | ((drive & 1) << 4),
        SMART,
        0,
    ]
}

/// SCSI / ATA Translation `ATA PASS-THROUGH (16)` (opcode 85h): PIO data-in, one
/// sector, transfer length in the sector count field.
#[must_use]
pub fn sat16(read: SmartRead) -> [u8; 16] {
    [
        0x85,
        4 << 1, // protocol 4 = PIO Data-In, no 48-bit extension
        0x0E,   // T_DIR = from device, BYTE_BLOCK = blocks, T_LENGTH = sector count
        0,
        read.feature(),
        0,
        1, // sector count
        0,
        1, // LBA low
        0,
        LBA_MID,
        0,
        LBA_HIGH,
        0xA0,
        SMART,
        0,
    ]
}

/// `ATA PASS-THROUGH (12)` (opcode A1h) for bridges without the 16-byte form.
#[must_use]
pub fn sat12(read: SmartRead) -> [u8; 12] {
    [
        0xA1,
        4 << 1,
        0x0E,
        read.feature(),
        1,
        1,
        LBA_MID,
        LBA_HIGH,
        0xA0,
        SMART,
        0,
        0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts() {
        assert_eq!(
            ide_regs(SmartRead::Data, 0),
            [0xD0, 1, 1, 0x4F, 0xC2, 0xA0, 0xB0, 0]
        );
        assert_eq!(ide_regs(SmartRead::Thresholds, 1)[0], 0xD1);
        assert_eq!(ide_regs(SmartRead::Thresholds, 1)[5], 0xB0);
        let c = sat16(SmartRead::Data);
        assert_eq!(
            (c[0], c[1], c[2], c[4], c[6], c[14]),
            (0x85, 8, 0x0E, 0xD0, 1, 0xB0)
        );
        assert_eq!((c[10], c[12]), (0x4F, 0xC2));
        let c = sat12(SmartRead::Thresholds);
        assert_eq!(
            (c[0], c[3], c[6], c[7], c[9]),
            (0xA1, 0xD1, 0x4F, 0xC2, 0xB0)
        );
    }

    /// Every command byte this module can produce is SMART (B0h) with a read feature.
    #[test]
    fn only_read_commands_exist() {
        for r in [SmartRead::Data, SmartRead::Thresholds] {
            assert!(matches!(r.feature(), 0xD0 | 0xD1));
            assert_eq!(ide_regs(r, 0)[6], SMART);
            assert_eq!(sat16(r)[14], SMART);
            assert_eq!(sat12(r)[9], SMART);
        }
    }
}
