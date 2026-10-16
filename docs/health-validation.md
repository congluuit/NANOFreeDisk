# Drive health — validation

DoD (`docs/PLAN.md` §8 row 7, `docs/PHASE7_HEALTH.md` §1): the verdict and values match CrystalDiskInfo on at least five sample drives (SATA SSD, NVMe, HDD, USB).

## What could be checked on the reference machine (2026-10-16)

- **Machine:** System76 Adder WS (i9-13900HX), Windows 10 22H2, standard user (not elevated).
- **Drives:** Samsung SSD 980 PRO 1 TB (NVMe, disk 0) and Samsung PM981 MZVLB512HAJQ 512 GB (NVMe, disk 1). No SATA, HDD or USB drive is attached and none was bought or attached for this; CrystalDiskInfo is not installed.
- **Independent reader:** a PowerShell script with its own `DeviceIoControl` declaration (C# `Add-Type`, nothing shared with the Rust code) that sends the same `IOCTL_STORAGE_QUERY_PROPERTY` / `StorageDeviceProtocolSpecificProperty` request for log page 02h and decodes the offsets by hand, plus `Get-PhysicalDisk` for Windows' own health flag. Both run seconds apart from `nfd health --json` (the drives were in use, so the I/O counters move a little).

| Field | Disk 0 script | Disk 0 `nfd` | Disk 1 script | Disk 1 `nfd` |
|---|---|---|---|---|
| Critical warning | 0 | 0 | 0 | 0 |
| Composite temperature | 56 °C | 56 °C | 62 °C | 62 °C |
| Available spare | 100 % | 100 % | 100 % | 100 % |
| Percentage used | 4 % | 4 % (96 % life left) | 2 % | 2 % (98 % life left) |
| Data units read | 60 884 551 | 60 884 609 | 18 820 701 | 18 820 716 |
| Data units written | 63 710 171 | 63 710 180 | 20 731 186 | 20 731 294 |
| Power cycles | 1 393 | 1 393 | 1 733 | 1 733 |
| Power-on hours | 4 287 | 4 287 | 6 596 | 6 596 |
| Unsafe shutdowns | 77 | 77 | 157 | 157 |
| Media errors | 0 | 0 | 0 | 0 |
| Verdict | `Get-PhysicalDisk`: Healthy | Good | Healthy | Good |

Serial numbers come from NVMe Identify Controller (adapter-level protocol query) and have Samsung's serial format (`S5P2…`, `S3TN…`), which is what CrystalDiskInfo shows; Windows' device descriptor gives the EUI-64 instead (`0025_38…`) — hence the extra query. Warning / critical temperatures (82/85 °C and 81/82 °C) are the drives' WCTEMP / CCTEMP.

Both drives answer without administrator rights. NSID `FFFFFFFF` returns an all-zero page on the 980 PRO, so the controller-wide NSID 0 is asked first and a zero page is never taken as data.

## ATA / SATA, HDD and USB (not attached here)

The verdict rules are pure functions (`crates/nfd-health/src/assess.rs`) with tests on SMART sectors in the on-the-wire layout (`crates/nfd-health/src/ata.rs`, `tests` module) for the three typical CrystalDiskInfo outcomes: a hard disk with pending sectors (Caution), a SATA SSD at 99 % life (Good) and a hard disk whose reallocated-sector value fell below its threshold (Bad). The tables are built from typical attribute values, not dumped from a real drive, so they check the decoding and the rules, not a vendor's quirks.

CrystalDiskInfo's rules as implemented: **Bad** when an attribute's normalized value is below a non-zero threshold (smartctl uses ≤; the two only differ when the value equals the threshold) or the drive predicts a failure; **Caution** when the raw value of 05 (reallocated), C5 (pending) or C6 (uncorrectable) is non-zero, or the SSD life attribute is at 10 % or less. Without administrator rights the thresholds are unknown, so Bad can only come from the drive's own failure flag — the app says so and offers the elevated read.

## Completing the DoD on another machine

1. Install CrystalDiskInfo (Standard edition) and run it elevated; note for each drive: *Health Status* (and %), *Temperature*, *Power On Hours*, *Power On Count*, *Host Writes*, and for ATA drives the attribute table (*Current*, *Worst*, *Threshold*, *Raw Values*).
2. In an administrator terminal run `nfd health --raw` (or the app's Health view → *Read everything (administrator)*), then `nfd health --json --redact > health.json` for the record.
3. Compare: verdict must be equal; temperature ±1 °C (both read the same register at slightly different times); hours / counts equal; Host Writes equal after unit conversion (CrystalDiskInfo shows GB = 10⁹ bytes; `nfd` shows binary units); every attribute's current / worst / threshold / raw equal.
4. Add a row per drive to the table above (model, bus, both verdicts, any difference and why).

LibreHardwareMonitor (already used for `docs/hwmon-validation.md`) also reads NVMe and ATA SMART when elevated and is a second reference for the same fields.
