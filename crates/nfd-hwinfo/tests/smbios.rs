//! SMBIOS decoding against the synthetic blob in `tests/fixtures/smbios/`
//! (regenerate with `make.mjs` there).

use nfd_hwinfo::smbios;

const BLOB: &[u8] = include_bytes!("fixtures/smbios/laptop-ddr5.bin");

#[test]
fn decodes_laptop_fixture() {
    let (major, minor, table) = smbios::parse_raw(BLOB).expect("header");
    assert_eq!((major, minor), (3, 4));
    let fw = smbios::decode(major, minor, table);
    assert_eq!(fw.version, "3.4");
    assert_eq!(fw.bios_vendor.as_deref(), Some("Acme BIOS"));
    assert_eq!(fw.bios_version.as_deref(), Some("1.07"));
    assert_eq!(fw.bios_date.as_deref(), Some("03/14/2025"));
    assert_eq!(fw.system_manufacturer.as_deref(), Some("Acme"));
    assert_eq!(fw.system_product.as_deref(), Some("Laptop 15"));
    assert_eq!(fw.system_version, None, "OEM placeholder is dropped");
    assert_eq!(fw.system_sku.as_deref(), Some("AC-15-X"));
    assert_eq!(fw.system_family.as_deref(), Some("Workstation"));
    assert_eq!(fw.board_product.as_deref(), Some("LP15-MB"));
    assert_eq!(fw.chassis.as_deref(), Some("laptop"));
    assert_eq!(fw.cpu_socket.as_deref(), Some("U3E1"));
    assert_eq!(
        (fw.cpu_current_mhz, fw.cpu_max_mhz, fw.cpu_external_mhz),
        (Some(2200), Some(5400), Some(100))
    );
    assert_eq!(fw.memory_slots, Some(2));
    assert_eq!(fw.memory_max, Some(64 << 30));

    assert_eq!(fw.modules.len(), 1, "the empty slot is skipped");
    let m = &fw.modules[0];
    assert_eq!(m.size, 16 << 30);
    assert_eq!(m.kind.as_deref(), Some("DDR5"));
    assert_eq!(m.form_factor.as_deref(), Some("SODIMM"));
    assert_eq!(m.locator.as_deref(), Some("DIMM A"));
    assert_eq!(m.bank.as_deref(), Some("BANK 0"));
    assert_eq!(m.manufacturer.as_deref(), Some("Samsung"));
    assert_eq!(m.part_number.as_deref(), Some("M425R2GA3BB0-CWMOD"));
    assert_eq!((m.speed_mts, m.configured_mts), (Some(5600), Some(5200)));
    assert_eq!(m.rank, Some(1));
}

#[test]
fn serial_numbers_never_leave_the_parser() {
    let (major, minor, table) = smbios::parse_raw(BLOB).expect("header");
    let json = serde_json::to_string(&smbios::decode(major, minor, table)).expect("json");
    assert!(!json.contains("SERIAL"), "{json}");
}

#[test]
fn every_truncation_is_safe() {
    let (_, _, table) = smbios::parse_raw(BLOB).expect("header");
    for len in 0..table.len() {
        let _ = smbios::decode(3, 4, &table[..len]);
    }
    for len in 0..BLOB.len() {
        if let Some((a, b, t)) = smbios::parse_raw(&BLOB[..len]) {
            let _ = smbios::decode(a, b, t);
        }
    }
}
