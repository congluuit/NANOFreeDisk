// Browser-only stand-in for the Hardware commands (`pnpm dev` without Tauri), shaped
// like the reference machine (i9-13900HX laptop) so the views can be designed.

import type { EndResult, HwInventory, Sensor, SensorSnapshot, Task, TaskSnapshot, TaskVerdict } from "./types";

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

const BLOCKED = ["system", "registry", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "fontdrvhost.exe", "dwm.exe", "memory compression"];
/** Paths are written with "/" here and shown with "\" like Windows does. */
const win = (p: string) => p.replace(/\//g, "\\");
const SYS = "C:/Windows/System32/";
type Proto = [number, number, string, Task["group"], string | null, string | null, string | null];
const PROCS: Proto[] = [
  [4, 0, "System", "windows", null, null, null],
  [128, 4, "Registry", "windows", null, null, null],
  [612, 4, "smss.exe", "windows", SYS + "smss.exe", null, "Microsoft Windows"],
  [860, 840, "csrss.exe", "windows", SYS + "csrss.exe", null, "Microsoft Windows"],
  [952, 840, "wininit.exe", "windows", SYS + "wininit.exe", null, "Microsoft Windows"],
  [1020, 952, "services.exe", "windows", SYS + "services.exe", null, "Microsoft Windows"],
  [1040, 952, "lsass.exe", "windows", SYS + "lsass.exe", null, "Microsoft Windows"],
  [1200, 1020, "svchost.exe", "windows", SYS + "svchost.exe", null, "Microsoft Windows"],
  [1240, 1020, "svchost.exe", "windows", SYS + "svchost.exe", null, "Microsoft Windows"],
  [1500, 900, "dwm.exe", "windows", SYS + "dwm.exe", null, "Microsoft Windows"],
  [4400, 4300, "explorer.exe", "app", "C:/Windows/explorer.exe", "File Explorer", "Microsoft Windows"],
  [6100, 4400, "brave.exe", "app", "C:/Program Files/BraveSoftware/Brave-Browser/Application/brave.exe", "NANOFreeDisk — GitHub", "Brave Software, Inc."],
  [6120, 6100, "brave.exe", "app", "C:/Program Files/BraveSoftware/Brave-Browser/Application/brave.exe", null, "Brave Software, Inc."],
  [6140, 6100, "brave.exe", "app", "C:/Program Files/BraveSoftware/Brave-Browser/Application/brave.exe", null, "Brave Software, Inc."],
  [7200, 4400, "Code.exe", "app", "C:/Users/nano/AppData/Local/Programs/Microsoft VS Code/Code.exe", "main.rs — NANOFreeDisk", "Microsoft Corporation"],
  [7300, 7200, "rust-analyzer.exe", "background", "C:/Users/nano/.rustup/toolchains/stable/bin/rust-analyzer.exe", null, null],
  [8000, 4400, "nanofreedisk.exe", "app", "C:/Program Files/NANOFreeDisk/nanofreedisk.exe", "NANOFreeDisk", "Lưu Đức Thành Công"],
  [8800, 1020, "MsMpEng.exe", "windows", "C:/ProgramData/Microsoft/Windows Defender/Platform/MsMpEng.exe", null, "Microsoft Corporation"],
  [9100, 4400, "OneDrive.exe", "background", "C:/Users/nano/AppData/Local/Microsoft/OneDrive/OneDrive.exe", null, "Microsoft Corporation"],
  [9300, 4400, "Steam.exe", "app", "C:/Program Files (x86)/Steam/steam.exe", "Steam", "Valve Corp."],
];
const ended = new Set<number>();

let thermalOn = false;
function thermalMock() {
  const now = Math.floor(Date.now() / 60000);
  const minutes = Array.from({ length: 240 }, (_, i) => {
    const t = now - 239 + i;
    return { t, cpu: 62 + 25 * Math.max(0, Math.sin(i / 17)), gpu: 50 + 8 * Math.sin(i / 23), disk: 48 + 6 * Math.sin(i / 41), hottest: { cpu: inventory.cpu.name, gpu: "NVIDIA GeForce RTX 4050 Laptop GPU", disk: "SAMSUNG MZVLB512HAJQ-000L7" } };
  });
  return { enabled: thermalOn, minutes };
}

function tasks(): TaskSnapshot {
  const t = Date.now() / 1000;
  const list: Task[] = PROCS.filter((p) => !ended.has(p[0])).map(([pid, ppid, name, group, path, title, publisher], i) => ({
    pid, ppid, name, group, path: path && win(path), title, publisher,
    create_time: String(133_700_000_000_000_000 + pid * 1000),
    cpu: Math.max(0, wave(name === "brave.exe" ? 6 : 0.6, name === "Code.exe" ? 4 : 1, 3 + (i % 5), i)(t)),
    memory: (20 + ((pid * 37) % 900)) * 2 ** 20,
    io_read: Math.max(0, wave(20e3, 60e3, 4, i)(t)),
    io_write: Math.max(0, wave(5e3, 20e3, 5, i)(t)),
    gpu: name === "brave.exe" && pid === 6140 ? Math.max(0, wave(8, 6, 3)(t)) : name === "dwm.exe" ? 2.4 : 0,
    threads: 4 + (pid % 60), handles: 100 + (pid % 2000), session: group === "windows" ? 0 : 1, priority: 8,
    blocked: BLOCKED.includes(name.toLowerCase()) || name === "nanofreedisk.exe",
  }));
  return {
    tasks: list,
    summary: { cpu: 14 + 6 * Math.sin(t / 7), memory_used: 20.5 * GB, memory_total: 31.8 * GB, disk: 1.2e6, gpu: 8, processes: list.length, threads: 4100 },
    elapsed_ms: 9.4,
  };
}

function mockVerdict(pid: number): TaskVerdict {
  const p = PROCS.find((x) => x[0] === pid);
  if (!p || ended.has(pid)) return { verdict: "unavailable", reasons: { status: "changed" } };
  const name = p[2].toLowerCase();
  if (pid === 4) return { verdict: "blocked", reasons: "kernel" };
  if (name === "nanofreedisk.exe") return { verdict: "blocked", reasons: "own" };
  if (BLOCKED.includes(name)) return { verdict: "blocked", reasons: "system-core" };
  if (name === "msmpeng.exe") return { verdict: "blocked", reasons: "protected" };
  const warn: ("service-host" | "explorer" | "windows-component")[] = [];
  if (name === "svchost.exe") warn.push("service-host");
  if (name === "explorer.exe") warn.push("explorer");
  if (p[4]?.toLowerCase().startsWith("c:/windows/")) warn.push("windows-component");
  return warn.length ? { verdict: "warn", reasons: warn } : { verdict: "allowed" };
}

function mockEnd(pid: number, mode: string, confirmed: boolean): EndResult[] {
  const name = PROCS.find((x) => x[0] === pid)?.[2] ?? "";
  const v = mockVerdict(pid);
  if (v.verdict === "blocked") return [{ pid, name, outcome: { status: "blocked", detail: v.reasons } }];
  if (v.verdict === "unavailable") return [{ pid, name, outcome: { status: "changed" } }];
  if (v.verdict === "warn" && !confirmed) return [{ pid, name, outcome: { status: "needs-confirm", detail: v.reasons } }];
  if (mode === "close") {
    const p = PROCS.find((x) => x[0] === pid);
    if (!p?.[5]) return [{ pid, name, outcome: { status: "no-window" } }];
    setTimeout(() => ended.add(pid), 1500);
    return [{ pid, name, outcome: { status: "close-requested" } }];
  }
  const out: EndResult[] = [];
  if (mode === "kill-tree") {
    for (const c of PROCS.filter((x) => x[1] === pid)) {
      ended.add(c[0]);
      out.push({ pid: c[0], name: c[2], outcome: { status: "ended" } });
    }
  }
  ended.add(pid);
  out.push({ pid, name, outcome: { status: "ended" } });
  return out;
}

export const hwHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  pawnio_status: () => ({ pawnio: { installed: true, running: true, version: "2.2.0.0", memory_integrity: false, intel_module: true, accessible: false }, active: false, unavailable: null }),
  advanced_sensors_start: () => ({ pawnio: { installed: true, running: true, version: "2.2.0.0", memory_integrity: false, intel_module: true, accessible: false }, active: false, unavailable: null }),
  pawnio_open_site: () => undefined,
  thermal_set: ({ enabled }) => ((thermalOn = enabled), thermalMock()),
  thermal_status: () => thermalMock(),
  tasks_snapshot: () => tasks(),
  task_verdict: ({ pid }) => mockVerdict(pid),
  task_details: ({ pid }) => {
    const p = PROCS.find((x) => x[0] === pid);
    return {
      command_line: p?.[4] ? `"${win(p[4])}"${p[2] === "svchost.exe" ? " -k netsvcs -p" : ""}` : null,
      user: win(p?.[3] === "windows" ? "NT AUTHORITY/SYSTEM" : "NANO-PC/nano"),
      publisher: p?.[6] ?? null,
      verified: !!p?.[6],
      services: p?.[2] === "svchost.exe" ? ["Background Intelligent Transfer Service", "Windows Update"] : [],
      children: PROCS.filter((x) => x[1] === pid).length,
    };
  },
  task_end: ({ pid, mode, confirmed }) => mockEnd(pid, mode, confirmed),
  task_reveal: () => undefined,
  open_task_manager: () => undefined,
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
