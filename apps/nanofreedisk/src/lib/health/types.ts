// Mirrors nfd_health::HealthReport (`nanofreedisk.health.v1`, docs/schema/health-v1.schema.json).

export type DriveStatus = "unknown" | "good" | "caution" | "bad";
export type Heat = "unknown" | "normal" | "warning" | "critical";
export type HealthSource = "nvme" | "ata-driver" | "ata-smart" | "sat" | "none";

export interface Reason {
  key: string;
  value: number | null;
  attr: number | null;
}

export interface NvmeLog {
  criticalWarning: number;
  temperature: number | null;
  availableSpare: number;
  spareThreshold: number;
  percentageUsed: number;
  dataUnitsRead: number;
  dataUnitsWritten: number;
  hostReads: number;
  hostWrites: number;
  controllerBusy: number;
  powerCycles: number;
  powerOnHours: number;
  unsafeShutdowns: number;
  mediaErrors: number;
  errorLogEntries: number;
  warningTempMinutes: number;
  criticalTempMinutes: number;
  sensors: number[];
}

export interface Attribute {
  id: number;
  flags: number;
  current: number;
  worst: number;
  threshold: number | null;
  raw: number;
  /** Standard English name (shown as is), null for vendor-specific ids. */
  name: string | null;
}

export interface AtaSmart {
  attributes: Attribute[];
  thresholds: boolean;
  predictFailure: boolean | null;
}

export interface Drive {
  index: number;
  model: string;
  serial: string | null;
  firmware: string | null;
  bus: string;
  size: number | null;
  ssd: boolean | null;
  removable: boolean;
  trim: boolean | null;
  status: DriveStatus;
  reasons: Reason[];
  temperature: number | null;
  tempWarning: number | null;
  tempCritical: number | null;
  heat: Heat;
  lifeLeft: number | null;
  powerOnHours: number | null;
  powerCycles: number | null;
  bytesWritten: number | null;
  bytesRead: number | null;
  unsafeShutdowns: number | null;
  source: HealthSource;
  note: string | null;
  nvme: NvmeLog | null;
  ata: AtaSmart | null;
  volumes: string[];
}

export interface HealthVolume {
  root: string;
  label: string;
  fs: string;
  total: number;
  free: number;
  system: boolean;
  removable: boolean;
  disk: number | null;
  offset: number | null;
  aligned: boolean | null;
  cluster: number | null;
  trimEnabled: boolean | null;
  bitlocker: string | null;
  dirty: boolean | null;
  daysToFull: number | null;
  hasTrend: boolean;
}

export interface HealthReport {
  schema: string;
  generatedAt: number;
  elevated: boolean;
  drives: Drive[];
  volumes: HealthVolume[];
}
