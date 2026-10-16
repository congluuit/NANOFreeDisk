// Browser-only stand-in for the Hardware commands (`pnpm dev` without Tauri), shaped
// like the reference machine (i9-13900HX laptop) so the views can be designed.

import type { HwInventory, Sensor, SensorSnapshot } from "./types";

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

type Def = Omit<Sensor, "value" | "min" | "max" | "id"> & { gen: (t: number) => number };
const def = (device: string, kind: Sensor["kind"], label: string, gen: (t: number) => number, extra: Partial<Def> = {}): Def => ({
  device, kind, label, index: null, detail: null, primary: false, estimated: false, source: "mock", gen, ...extra,
});
const wave = (base: number, amp: number, period: number, phase = 0) => (t: number) => base + amp * Math.sin(t / period + phase);
const DEFS: Def[] = [
  def("cpu", "load", "cpu-total", wave(14, 9, 7), { primary: true }),
  def("cpu", "load", "cpu-thread-max", wave(48, 30, 5)),
  ...Array.from({ length: 32 }, (_, i) =>
    def("cpu", "load", "cpu-thread", wave(12 + (i % 5) * 4, 10, 3 + (i % 7), i), { index: i + 1, detail: i < 16 ? "P" : "E" }),
  ),
  def("cpu", "clock", "cpu-clock-avg", wave(3900, 500, 6), { primary: true, estimated: true }),
  def("memory", "load", "mem-load", wave(64, 3, 30), { primary: true }),
  def("memory", "data", "mem-used", wave(20.5 * GB, GB, 30), { primary: true }),
  def("memory", "data", "mem-available", wave(11 * GB, GB, 30, Math.PI)),
  def("gpu1", "temperature", "gpu-temp", wave(56, 6, 20), { primary: true, source: "NVML" }),
  def("gpu1", "clock", "gpu-clock-core", wave(1400, 900, 9), { primary: true }),
  def("gpu1", "power", "gpu-power", wave(22, 18, 9), { primary: true }),
  def("gpu1", "load", "gpu-load", wave(25, 20, 9), { primary: true }),
  def("gpu1", "load", "gpu-load-3d", wave(25, 20, 9)),
  def("gpu1", "data", "gpu-mem-used", wave(1.2 * GB, 0.3 * GB, 15), { primary: true }),
  def("gpu0", "load", "gpu-load", wave(6, 4, 4), { primary: true }),
  def("disk0", "temperature", "disk-temp", wave(47, 2, 40), { primary: true }),
  def("disk0", "throughput", "disk-read", (t) => Math.max(0, 40e6 * Math.sin(t / 3))),
  def("disk1", "temperature", "disk-temp", wave(63, 4, 40), { primary: true }),
  def("battery0", "level", "bat-charge", wave(87, 0.5, 60), { primary: true }),
  def("battery0", "power", "bat-rate", wave(-14, 4, 10), { primary: true }),
  def("battery0", "level", "bat-wear", () => 15.3),
  def("net0", "throughput", "net-down", (t) => Math.max(0, 2e6 * Math.sin(t / 2)), { primary: true }),
  def("net0", "throughput", "net-up", (t) => Math.max(0, 3e5 * Math.cos(t / 2)), { primary: true }),
];
const stats = new Map<string, { min: number; max: number }>();
let samples = 0;

function snapshot(): SensorSnapshot {
  samples++;
  const t = Date.now() / 1000;
  const sensors = DEFS.map((d) => {
    const id = `${d.device}/${d.label}${d.index != null ? `/${d.index}` : ""}`;
    const value = d.gen(t);
    const st = stats.get(id) ?? { min: value, max: value };
    st.min = Math.min(st.min, value);
    st.max = Math.max(st.max, value);
    stats.set(id, st);
    const { gen: _gen, ...info } = d;
    return { ...info, id, value, min: st.min, max: st.max };
  });
  return {
    devices: [
      { id: "cpu", kind: "cpu", name: inventory.cpu.name },
      { id: "memory", kind: "memory", name: "Memory" },
      { id: "gpu0", kind: "gpu", name: "Intel(R) UHD Graphics" },
      { id: "gpu1", kind: "gpu", name: "NVIDIA GeForce RTX 4050 Laptop GPU" },
      { id: "disk0", kind: "storage", name: "Samsung SSD 980 PRO 1TB" },
      { id: "disk1", kind: "storage", name: "SAMSUNG MZVLB512HAJQ-000L7" },
      { id: "battery0", kind: "battery", name: "BAT" },
      { id: "net0", kind: "network", name: "Intel(R) Wi-Fi 6E AX211 160MHz" },
    ],
    sensors,
    notes: ["temperature", "power", "voltage"].map((kind) => ({ device: "cpu", kind: kind as Sensor["kind"], reason: "needs-pawnio" })),
    samples,
    sample_ms: 14.2,
  };
}

export const hwHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  hw_inventory: () => ({ ...inventory, collected_at: Math.floor(Date.now() / 1000) }),
  hw_export: () => undefined,
  sensors_snapshot: () => snapshot(),
  sensors_reset_minmax: () => stats.clear(),
  sensors_history: ({ id, n }) => {
    const d = DEFS.find((x) => id === `${x.device}/${x.label}${x.index != null ? `/${x.index}` : ""}`);
    const now = Date.now() / 1000;
    return d ? Array.from({ length: n }, (_, i) => d.gen(now - (n - 1 - i))) : [];
  },
};
