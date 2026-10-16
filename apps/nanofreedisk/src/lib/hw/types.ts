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
