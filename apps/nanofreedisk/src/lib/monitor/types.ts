// Shapes of the Monitor commands (src-tauri/src/commands/monitor.rs, nfd-monitor).
// Rates are per second (bytes or operations); times are Unix milliseconds.

export type Source = "counters" | "etw" | "helper";

export interface ProcRate {
  pid: number;
  /** FILETIME as text; empty when the process has exited. */
  createTime: string;
  name: string;
  path: string | null;
  read: number;
  write: number;
  readOps: number;
  writeOps: number;
  /** Files touched in the interval (file-level sources only). */
  files: number;
}

export interface FileRate {
  pid: number;
  process: string;
  path: string;
  read: number;
  write: number;
  ops: number;
}

export interface DiskStat {
  /** PDH instance, e.g. "0 C: E:". */
  name: string;
  read: number;
  write: number;
  transfers: number;
  queue: number;
  latencyMs: number;
  idle: number;
}

export type TouchOp = "create" | "write" | "delete" | "rename";

export interface TouchRow {
  at: number;
  pid: number;
  process: string;
  op: TouchOp;
  path: string;
  target: string | null;
}

export type Alert =
  | { kind: "log-spam"; pid: number; createTime: string; name: string; path: string | null; rate: number; minutes: number; since: number }
  | { kind: "big-log"; pid: number; name: string; path: string; size: number };

export type StampedAlert = Alert & { at: number };

export interface Snapshot {
  schema: string;
  at: number;
  source: Source;
  intervalMs: number;
  read: number;
  write: number;
  ops: number;
  processes: ProcRate[];
  files: FileRate[];
  disks: DiskStat[];
  touches: TouchRow[];
  lost: number;
  undecoded: number;
  alerts: Alert[];
}

export interface SpamConfig {
  enabled: boolean;
  /** MB/s. */
  mbps: number;
  minutes: number;
}

export interface Detail {
  /** The user asked for file-level detail. */
  wanted: boolean;
  /** A trace session is running. */
  running: boolean;
  /** It runs in the elevated helper. */
  remote: boolean;
  error: string | null;
}

export interface MonitorView {
  snapshot: Snapshot | null;
  paused: boolean;
  elevated: boolean;
  detail: Detail;
  watch: string | null;
  alerts: StampedAlert[];
  touches: TouchRow[];
  config: SpamConfig;
}

export interface Point {
  /** Unix seconds. */
  t: number;
  read: number;
  write: number;
}

export interface TopProc {
  name: string;
  read: number;
  write: number;
}

export interface Minute {
  /** Unix seconds of the minute's start. */
  ts: number;
  read: number;
  write: number;
  peakRead: number;
  peakWrite: number;
  seconds: number;
  top: TopProc[];
}

export interface MonitorHistory {
  minutes: Minute[];
  top: TopProc[];
}

export interface MonitorTexts {
  spamTitle: string;
  spamBody: string;
  logTitle: string;
  logBody: string;
}
