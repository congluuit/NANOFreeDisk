// Hardware view data (mirrors crates/nfd-hwinfo, nfd-sensors, nfd-tasks).

export interface MemoryModule {
  locator: string | null;
  bank: string | null;
  manufacturer: string | null;
  part_number: string | null;
  size: number;
  kind: string | null;
  form_factor: string | null;
  speed_mts: number | null;
  configured_mts: number | null;
  rank: number | null;
}

/** `nanofreedisk.hwinfo.v1` */
export interface HwInventory {
  schema: string;
  collected_at: number;
  system: {
    manufacturer: string | null;
    product: string | null;
    version: string | null;
    family: string | null;
    sku: string | null;
    chassis: string | null;
    os_name: string;
    os_version: string | null;
    os_build: string | null;
    os_edition: string | null;
    computer: string | null;
    uptime_secs: number;
  };
  cpu: {
    vendor: string;
    name: string;
    family: number;
    model: number;
    stepping: number;
    packages: number;
    cores: number;
    threads: number;
    p_cores: number | null;
    e_cores: number | null;
    l1: number;
    l2: number;
    l3: number;
    base_mhz: number | null;
    max_mhz: number | null;
    bus_mhz: number | null;
    socket: string | null;
    features: string[];
  };
  board: {
    manufacturer: string | null;
    product: string | null;
    version: string | null;
    bios_vendor: string | null;
    bios_version: string | null;
    bios_date: string | null;
    smbios_version: string | null;
  };
  memory: {
    total: number;
    available: number;
    installed: number | null;
    slots: number | null;
    max_capacity: number | null;
    modules: MemoryModule[];
  };
  gpus: {
    name: string;
    vendor: string | null;
    vram: number | null;
    driver_version: string | null;
    driver_date: string | null;
    location: string | null;
  }[];
  disks: {
    index: number;
    model: string;
    firmware: string | null;
    bus: string;
    size: number | null;
    ssd: boolean | null;
    removable: boolean;
    temperature_c: number | null;
  }[];
  displays: {
    name: string | null;
    width: number;
    height: number;
    refresh_hz: number | null;
    connector: string | null;
    primary: boolean;
  }[];
  network: { name: string; alias: string | null; kind: string; connected: boolean; link_bps: number | null }[];
  batteries: {
    name: string | null;
    manufacturer: string | null;
    chemistry: string | null;
    design_mwh: number;
    full_mwh: number;
    cycles: number | null;
    charge_percent: number | null;
  }[];
}

export type SensorKind =
  | "temperature"
  | "voltage"
  | "power"
  | "clock"
  | "load"
  | "fan"
  | "control"
  | "data"
  | "throughput"
  | "level"
  | "energy";

export type DeviceKind = "cpu" | "gpu" | "memory" | "storage" | "battery" | "board" | "network";

export interface SensorDevice {
  id: string;
  kind: DeviceKind;
  name: string;
}

export interface Sensor {
  id: string;
  device: string;
  kind: SensorKind;
  label: string;
  index: number | null;
  detail: string | null;
  primary: boolean;
  estimated: boolean;
  source: string;
  value: number | null;
  min: number | null;
  max: number | null;
}

export interface SensorNote {
  device: string;
  kind: SensorKind;
  reason: string;
}

export interface SensorSnapshot {
  devices: SensorDevice[];
  sensors: Sensor[];
  notes: SensorNote[];
  samples: number;
  sample_ms: number;
}

export interface AdvancedStatus {
  pawnio?: {
    installed: boolean;
    running: boolean;
    version: string | null;
    memory_integrity: boolean;
    intel_module: boolean;
    accessible: boolean;
  };
  active: boolean;
  unavailable: string | null;
}

export interface ThermalMinute {
  /** Unix minutes. */
  t: number;
  cpu: number | null;
  gpu: number | null;
  disk: number | null;
  hottest: Partial<Record<"cpu" | "gpu" | "disk", string>>;
}

export interface ThermalStatus {
  enabled: boolean;
  minutes: ThermalMinute[];
}

export type TaskGroup = "app" | "background" | "windows";

export interface Task {
  pid: number;
  ppid: number;
  name: string;
  /** FILETIME as a decimal string; sent back with every action. */
  create_time: string;
  path: string | null;
  title: string | null;
  group: TaskGroup;
  cpu: number;
  memory: number;
  io_read: number;
  io_write: number;
  gpu: number;
  threads: number;
  handles: number;
  session: number;
  priority: number;
  publisher: string | null;
  blocked: boolean;
}

export interface TaskSnapshot {
  tasks: Task[];
  summary: {
    cpu: number;
    memory_used: number;
    memory_total: number;
    disk: number;
    gpu: number;
    processes: number;
    threads: number;
  };
  elapsed_ms: number;
}

export type BlockReason = "kernel" | "system-core" | "critical" | "protected" | "own";
export type WarnReason = "service-host" | "explorer" | "windows-component";
export type EndMode = "close" | "kill" | "kill-tree";

export type TaskOutcome =
  | { status: "ended" }
  | { status: "close-requested" }
  | { status: "no-window" }
  | { status: "blocked"; detail: BlockReason }
  | { status: "needs-confirm"; detail: WarnReason[] }
  | { status: "changed" }
  | { status: "access-denied" }
  | { status: "failed"; detail: string };

export type TaskVerdict =
  | { verdict: "blocked"; reasons: BlockReason }
  | { verdict: "warn"; reasons: WarnReason[] }
  | { verdict: "allowed" }
  | { verdict: "unavailable"; reasons: TaskOutcome };

export interface EndResult {
  pid: number;
  name: string;
  outcome: TaskOutcome;
}

export interface TaskDetails {
  command_line: string | null;
  user: string | null;
  publisher: string | null;
  verified: boolean;
  services: string[];
  children: number;
}
