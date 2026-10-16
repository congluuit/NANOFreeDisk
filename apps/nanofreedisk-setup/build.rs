//! Windows resources of the setup: icon, version info and an application manifest
//! (per-monitor DPI, Common Controls 6, `asInvoker` — the wizard asks for UAC only
//! when "For everyone" is chosen). The `.rc` file is generated so it carries the
//! crate version.

use std::path::PathBuf;

fn rc_path(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}

fn main() {
    println!("cargo:rerun-if-changed=setup.manifest");
    println!("cargo:rerun-if-changed=../../installer/icons/nanofreedisk.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut parts: Vec<u16> = version
        .split(['.', '-'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect();
    parts.resize(3, 0);
    let num = format!("{},{},{},0", parts[0], parts[1], parts[2]);
    let icon = rc_path(&dir.join("../../installer/icons/nanofreedisk.ico"));
    let manifest = rc_path(&dir.join("setup.manifest"));
    let rc = format!(
        r#"1 ICON "{icon}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {num}
PRODUCTVERSION {num}
FILEFLAGSMASK 0x3f
FILEFLAGS 0x0
FILEOS 0x40004
FILETYPE 0x1
FILESUBTYPE 0x0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "NANO (nanodev.io.vn)"
      VALUE "FileDescription", "NANOFreeDisk Setup"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "nanofreedisk-setup"
      VALUE "LegalCopyright", "(c) 2026 Luu Duc Thanh Cong (NANO). MIT License."
      VALUE "OriginalFilename", "nanofreedisk-setup.exe"
      VALUE "ProductName", "NANOFreeDisk"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let rc_file = out.join("setup.rc");
    if std::fs::write(&rc_file, rc).is_ok() {
        let _ = embed_resource::compile(&rc_file, embed_resource::NONE).manifest_optional();
    }
}
