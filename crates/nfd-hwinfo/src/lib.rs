//! Hardware inventory for the Hardware → Overview view and `nfd hw`
//! (`docs/HARDWARE_MONITOR.md` §4): CPU (CPUID + topology), system / board / BIOS /
//! memory sticks (SMBIOS), GPUs, disks, monitors, network adapters and batteries.
//!
//! Everything is read-only and works without a driver or admin rights. Serial
//! numbers, UUIDs, MAC and IP addresses are never collected: the inventory is meant
//! to be copied into a support request. Serialized as `nanofreedisk.hwinfo.v1`.

pub mod cpu;
pub mod smbios;
#[cfg(windows)]
pub mod win;

use serde::{Deserialize, Serialize};

pub use smbios::MemoryModule;

/// JSON schema id of [`HwInventory`].
pub const SCHEMA: &str = "nanofreedisk.hwinfo.v1";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HwInventory {
    pub schema: String,
    /// Unix seconds.
    pub collected_at: u64,
    pub system: System,
    pub cpu: Cpu,
    pub board: Board,
    pub memory: Memory,
    pub gpus: Vec<Gpu>,
    pub disks: Vec<Disk>,
    pub displays: Vec<Display>,
    pub network: Vec<NetAdapter>,
    pub batteries: Vec<Battery>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct System {
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub family: Option<String>,
    pub sku: Option<String>,
    /// "laptop", "desktop", "server", "tablet", "handheld".
    pub chassis: Option<String>,
    pub os_name: String,
    pub os_version: Option<String>,
    pub os_build: Option<String>,
    pub os_edition: Option<String>,
    pub computer: Option<String>,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cpu {
    pub vendor: String,
    pub name: String,
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub packages: u32,
    pub cores: u32,
    pub threads: u32,
    pub p_cores: Option<u32>,
    pub e_cores: Option<u32>,
    /// Bytes, summed over all instances.
    pub l1: u64,
    pub l2: u64,
    pub l3: u64,
    pub base_mhz: Option<u32>,
    pub max_mhz: Option<u32>,
    pub bus_mhz: Option<u32>,
    pub socket: Option<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub bios_vendor: Option<String>,
    pub bios_version: Option<String>,
    pub bios_date: Option<String>,
    pub smbios_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    /// Bytes usable by Windows.
    pub total: u64,
    pub available: u64,
    /// Bytes installed (sum of the sticks), when SMBIOS lists them.
    pub installed: Option<u64>,
    pub slots: Option<u32>,
    pub max_capacity: Option<u64>,
    pub modules: Vec<MemoryModule>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Gpu {
    pub name: String,
    pub vendor: Option<String>,
    /// Dedicated video memory, bytes.
    pub vram: Option<u64>,
    pub driver_version: Option<String>,
    pub driver_date: Option<String>,
    /// "PCI bus 1, device 0, function 0".
    pub location: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Disk {
    /// `N` of `\\.\PhysicalDriveN`.
    pub index: u32,
    pub model: String,
    pub firmware: Option<String>,
    /// "NVMe", "SATA", "USB"…
    pub bus: String,
    pub size: Option<u64>,
    pub ssd: Option<bool>,
    pub removable: bool,
    pub temperature_c: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Display {
    pub name: Option<String>,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: Option<f64>,
    pub connector: Option<String>,
    pub primary: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NetAdapter {
    pub name: String,
    pub alias: Option<String>,
    /// "ethernet" or "wifi".
    pub kind: String,
    pub connected: bool,
    pub link_bps: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    pub name: Option<String>,
    pub manufacturer: Option<String>,
    pub chemistry: Option<String>,
    pub design_mwh: u32,
    pub full_mwh: u32,
    pub cycles: Option<u32>,
    pub charge_percent: Option<f32>,
}

impl Battery {
    /// Lost capacity, 0–100 %.
    pub fn wear_percent(&self) -> Option<f32> {
        (self.design_mwh > 0 && self.full_mwh > 0).then(|| {
            (100.0 - f64::from(self.full_mwh) * 100.0 / f64::from(self.design_mwh))
                .clamp(0.0, 100.0) as f32
        })
    }
}

/// Combine CPUID, topology and SMBIOS into the CPU section.
pub fn cpu_section(id: &cpu::CpuId, fw: &smbios::Firmware) -> Cpu {
    Cpu {
        vendor: id.vendor.clone(),
        name: id.brand.clone(),
        family: id.family,
        model: id.model,
        stepping: id.stepping,
        base_mhz: id.base_mhz.or(fw.cpu_current_mhz),
        max_mhz: id
            .max_mhz
            .or(fw.cpu_max_mhz)
            .filter(|&m| m > id.base_mhz.unwrap_or(0)),
        bus_mhz: id.bus_mhz.or(fw.cpu_external_mhz),
        socket: fw.cpu_socket.clone(),
        features: id.features.iter().map(|s| (*s).to_owned()).collect(),
        ..Cpu::default()
    }
}

/// Collect the whole inventory (a few hundred ms; call off the UI thread).
#[cfg(windows)]
pub fn collect() -> HwInventory {
    let fw = win::smbios_blob()
        .and_then(|b| smbios::parse_raw(&b).map(|(maj, min, t)| smbios::decode(maj, min, t)))
        .unwrap_or_default();
    let topo = win::topology::native();
    let mut cpu = cpu_section(&cpu::native(), &fw);
    cpu.packages = topo.packages.max(1);
    cpu.cores = topo.cores;
    cpu.threads = topo.threads;
    cpu.p_cores = topo.p_cores;
    cpu.e_cores = topo.e_cores;
    (cpu.l1, cpu.l2, cpu.l3) = (topo.l1, topo.l2, topo.l3);
    let os = win::os::native();
    let (total, available) = win::os::memory();
    let installed: u64 = fw.modules.iter().map(|m| m.size).sum();
    HwInventory {
        schema: SCHEMA.to_owned(),
        collected_at: now(),
        system: System {
            manufacturer: fw.system_manufacturer.clone(),
            product: fw.system_product.clone(),
            version: fw.system_version.clone(),
            family: fw.system_family.clone(),
            sku: fw.system_sku.clone(),
            chassis: fw.chassis.clone(),
            os_name: os.name,
            os_version: os.display_version,
            os_build: os.build,
            os_edition: os.edition,
            computer: os.computer,
            uptime_secs: os.uptime_secs,
        },
        cpu,
        board: Board {
            manufacturer: fw.board_manufacturer.clone(),
            product: fw.board_product.clone(),
            version: fw.board_version.clone(),
            bios_vendor: fw.bios_vendor.clone(),
            bios_version: fw.bios_version.clone(),
            bios_date: fw.bios_date.clone(),
            smbios_version: (!fw.version.is_empty()).then(|| fw.version.clone()),
        },
        memory: Memory {
            total,
            available,
            installed: (installed > 0).then_some(installed),
            slots: fw.memory_slots,
            max_capacity: fw.memory_max,
            modules: fw.modules,
        },
        gpus: win::gpu::native(),
        disks: win::storage::native(),
        displays: win::display::native(),
        network: win::net::native(),
        batteries: win::battery::native(),
    }
}

/// Non-Windows builds: CPUID only (Phase 10 adds the other platforms).
#[cfg(not(windows))]
pub fn collect() -> HwInventory {
    let cpu = cpu_section(&cpu::native(), &smbios::Firmware::default());
    HwInventory {
        schema: SCHEMA.to_owned(),
        collected_at: now(),
        system: System {
            os_name: std::env::consts::OS.to_owned(),
            ..System::default()
        },
        cpu,
        ..HwInventory::default()
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn gib(b: u64) -> String {
    let g = b as f64 / f64::from(1u32 << 30);
    if g >= 10.0 {
        format!("{g:.0} GB")
    } else {
        format!("{g:.1} GB")
    }
}

fn mib(b: u64) -> String {
    if b >= 1 << 20 {
        format!("{} MB", b >> 20)
    } else {
        format!("{} KB", b >> 10)
    }
}

/// Plain-text report (English) for `nfd hw` and bug reports.
pub fn to_text(inv: &HwInventory) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let o = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
    let sys = &inv.system;
    let _ = writeln!(s, "System");
    let _ = writeln!(
        s,
        "  Model      {} {}{}",
        o(&sys.manufacturer),
        o(&sys.product),
        sys.chassis
            .as_ref()
            .map(|c| format!(" ({c})"))
            .unwrap_or_default()
    );
    let _ = writeln!(
        s,
        "  OS         {} {} (build {})",
        sys.os_name,
        o(&sys.os_version),
        o(&sys.os_build)
    );
    let up = sys.uptime_secs;
    let _ = writeln!(
        s,
        "  Uptime     {}d {}h {}m",
        up / 86400,
        up / 3600 % 24,
        up / 60 % 60
    );
    let c = &inv.cpu;
    let _ = writeln!(s, "CPU");
    let _ = writeln!(s, "  Name       {}", c.name);
    let split = match (c.p_cores, c.e_cores) {
        (Some(p), Some(e)) => format!(" ({p}P + {e}E)"),
        _ => String::new(),
    };
    let _ = writeln!(
        s,
        "  Cores      {} cores{split}, {} threads",
        c.cores, c.threads
    );
    let clocks = match (c.base_mhz, c.max_mhz) {
        (Some(b), Some(m)) => format!("{b} MHz base, {m} MHz max"),
        (Some(b), None) => format!("{b} MHz"),
        _ => "-".into(),
    };
    let _ = writeln!(s, "  Clocks     {clocks}");
    let _ = writeln!(
        s,
        "  Cache      L1 {} · L2 {} · L3 {}",
        mib(c.l1),
        mib(c.l2),
        mib(c.l3)
    );
    let _ = writeln!(s, "  Socket     {}", o(&c.socket));
    let b = &inv.board;
    let _ = writeln!(s, "Mainboard");
    let _ = writeln!(s, "  Board      {} {}", o(&b.manufacturer), o(&b.product));
    let _ = writeln!(
        s,
        "  BIOS       {} {} ({})",
        o(&b.bios_vendor),
        o(&b.bios_version),
        o(&b.bios_date)
    );
    let m = &inv.memory;
    let _ = writeln!(s, "Memory");
    let _ = writeln!(
        s,
        "  Total      {} ({} slots)",
        gib(m.installed.unwrap_or(m.total)),
        m.slots.map_or("-".into(), |n| n.to_string())
    );
    for d in &m.modules {
        let _ = writeln!(
            s,
            "  {:<10} {} {} {} {} MT/s {}",
            o(&d.locator),
            gib(d.size),
            o(&d.kind),
            o(&d.manufacturer),
            d.configured_mts.or(d.speed_mts).unwrap_or(0),
            o(&d.part_number)
        );
    }
    let _ = writeln!(s, "Graphics");
    for g in &inv.gpus {
        let _ = writeln!(
            s,
            "  {} — {} VRAM, driver {}",
            g.name,
            g.vram.map_or("-".into(), gib),
            o(&g.driver_version)
        );
    }
    let _ = writeln!(s, "Storage");
    for d in &inv.disks {
        let kind = match d.ssd {
            Some(true) => "SSD",
            Some(false) => "HDD",
            None => "",
        };
        let _ = writeln!(
            s,
            "  Disk {}     {} — {} {} {}{}",
            d.index,
            d.model,
            d.size.map_or("-".into(), gib),
            d.bus,
            kind,
            d.temperature_c
                .map(|t| format!(", {t:.0} °C"))
                .unwrap_or_default()
        );
    }
    if !inv.displays.is_empty() {
        let _ = writeln!(s, "Displays");
        for d in &inv.displays {
            let _ = writeln!(
                s,
                "  {} — {}×{}{}{}",
                d.name.clone().unwrap_or_else(|| "Display".into()),
                d.width,
                d.height,
                d.refresh_hz
                    .map(|r| format!(" @ {r:.0} Hz"))
                    .unwrap_or_default(),
                if d.primary { " (primary)" } else { "" }
            );
        }
    }
    if !inv.network.is_empty() {
        let _ = writeln!(s, "Network");
        for n in &inv.network {
            let _ = writeln!(
                s,
                "  {} ({}){}",
                n.name,
                n.kind,
                n.link_bps
                    .map(|b| format!(" — {} Mbps", b / 1_000_000))
                    .unwrap_or_default()
            );
        }
    }
    for bat in &inv.batteries {
        let _ = writeln!(s, "Battery");
        let _ = writeln!(
            s,
            "  {} — design {} mWh, full {} mWh, wear {}{}",
            o(&bat.name),
            bat.design_mwh,
            bat.full_mwh,
            bat.wear_percent()
                .map_or("-".into(), |w| format!("{w:.0} %")),
            bat.cycles
                .map(|c| format!(", {c} cycles"))
                .unwrap_or_default()
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_wear() {
        let b = Battery {
            design_mwh: 80_000,
            full_mwh: 72_000,
            ..Battery::default()
        };
        assert_eq!(b.wear_percent(), Some(10.0));
        assert_eq!(Battery::default().wear_percent(), None);
    }

    #[test]
    fn collect_runs_and_serializes() {
        let inv = collect();
        assert_eq!(inv.schema, SCHEMA);
        let json = serde_json::to_string(&inv).expect("json");
        let back: HwInventory = serde_json::from_str(&json).expect("round trip");
        assert_eq!(back, inv);
        assert!(!to_text(&inv).is_empty());
        #[cfg(windows)]
        {
            assert!(inv.cpu.threads >= 1, "{:?}", inv.cpu);
            assert!(inv.memory.total > 0);
        }
    }
}
