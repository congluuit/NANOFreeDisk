// Browser-only stand-in for the Hardware commands (`pnpm dev` without Tauri), shaped
// like the reference machine (i9-13900HX laptop) so the views can be designed.

import type { HwInventory } from "./types";

const GB = 2 ** 30;

const inventory: HwInventory = {
  schema: "nanofreedisk.hwinfo.v1",
  collected_at: Math.floor(Date.now() / 1000),
  system: {
    manufacturer: "System76",
    product: "Adder WS",
    version: "addw3",
    family: null,
    sku: null,
    chassis: "laptop",
    os_name: "Windows 11 Pro",
    os_version: "24H2",
    os_build: "26100.4061",
    os_edition: "Professional",
    computer: "NANO-PC",
    uptime_secs: 3 * 86400 + 5 * 3600 + 120,
  },
  cpu: {
    vendor: "Intel",
    name: "13th Gen Intel(R) Core(TM) i9-13900HX",
    family: 6,
    model: 0xb7,
    stepping: 1,
    packages: 1,
    cores: 24,
    threads: 32,
    p_cores: 8,
    e_cores: 16,
    l1: 2_176 * 1024,
    l2: 32 * 2 ** 20,
    l3: 36 * 2 ** 20,
    base_mhz: 2200,
    max_mhz: 5400,
    bus_mhz: 100,
    socket: "CPU0",
    features: ["SSE4.2", "AES", "AVX", "AVX2", "FMA3", "SHA", "VT-x"],
  },
  board: {
    manufacturer: "System76",
    product: "Adder WS",
    version: "addw3",
    bios_vendor: "coreboot",
    bios_version: "2023-09-19_16ef69c",
    bios_date: "09/15/2023",
    smbios_version: "3.0",
  },
  memory: {
    total: 31.7 * GB,
    available: 14.2 * GB,
    installed: 32 * GB,
    slots: 2,
    max_capacity: 64 * GB,
    modules: [1, 2].map((i) => ({
      locator: `Channel-${i - 1}-DIMM-0`,
      bank: null,
      manufacturer: "Samsung",
      part_number: "M425R2GA3BB0-CWMOD",
      size: 16 * GB,
      kind: "DDR5",
      form_factor: "SODIMM",
      speed_mts: 5600,
      configured_mts: 5600,
      rank: 1,
    })),
  },
  gpus: [
    { name: "NVIDIA GeForce RTX 4050 Laptop GPU", vendor: "NVIDIA", vram: 6 * GB, driver_version: "32.0.15.6603", driver_date: "10-10-2024", location: "PCI bus 1, device 0, function 0" },
    { name: "Intel(R) UHD Graphics", vendor: "Intel", vram: 2 * GB, driver_version: "32.0.101.7088", driver_date: "8-27-2025", location: "PCI bus 0, device 2, function 0" },
  ],
  disks: [
    { index: 0, model: "Samsung SSD 980 PRO 1TB", firmware: "5B2QGXA7", bus: "NVMe", size: 932 * GB, ssd: true, removable: false, temperature_c: 47 },
    { index: 1, model: "SAMSUNG MZVLB512HAJQ-000L7", firmware: "5M2QEXF7", bus: "NVMe", size: 477 * GB, ssd: true, removable: false, temperature_c: 52 },
  ],
  displays: [{ name: null, width: 1920, height: 1080, refresh_hz: 144, connector: "Internal", primary: true }],
  network: [
    { name: "Intel(R) Ethernet Connection (22) I219-V", alias: "Ethernet", kind: "ethernet", connected: false, link_bps: null },
    { name: "Intel(R) Wi-Fi 6E AX211 160MHz", alias: "Wi-Fi", kind: "wifi", connected: true, link_bps: 865_000_000 },
  ],
  batteries: [{ name: "BAT", manufacturer: "Notebook", chemistry: "LION", design_mwh: 70840, full_mwh: 59968, cycles: 163, charge_percent: 87 }],
};

export const hwHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  hw_inventory: () => ({ ...inventory, collected_at: Math.floor(Date.now() / 1000) }),
  hw_export: () => undefined,
};
