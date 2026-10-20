// Hardware → Sensors: tree building, labels and value formatting. Pure (tested in
// hw.test.ts); the view only renders what this returns.

import type { Translate } from "./overview";
import { tempLevel, type TempKind } from "./overview.ts";
import type { AdvancedStatus, DeviceKind, Sensor, SensorDevice, SensorKind, SensorNote, SensorSnapshot } from "./types";

/** LibreHardwareMonitor's group order. */
export const KIND_ORDER: SensorKind[] = [
  "voltage",
  "power",
  "clock",
  "temperature",
  "load",
  "fan",
  "control",
  "level",
  "energy",
  "data",
  "throughput",
];

export interface Group {
  kind: SensorKind;
  sensors: Sensor[];
  notes: SensorNote[];
}

export interface DeviceNode {
  device: SensorDevice;
  groups: Group[];
}

/** Devices → kind groups → sensors; Simple mode keeps primary sensors only. An
 * estimate is dropped once a measured sensor of the same device and kind exists
 * (the estimated clock once Advanced sensors report real clocks). */
export function tree(snap: SensorSnapshot, simple: boolean): DeviceNode[] {
  const measured = new Set(snap.sensors.filter((s) => !s.estimated).map((s) => `${s.device}/${s.kind}`));
  return snap.devices
    .map((device) => {
      const mine = snap.sensors.filter(
        (s) => s.device === device.id && (!simple || s.primary) && !(s.estimated && measured.has(`${s.device}/${s.kind}`)),
      );
      const notes = snap.notes.filter((n) => n.device === device.id);
      const groups = KIND_ORDER.map((kind) => ({
        kind,
        sensors: mine.filter((s) => s.kind === kind),
        notes: notes.filter((n) => n.kind === kind),
      })).filter((g) => g.sensors.length || g.notes.length);
      return { device, groups };
    })
    .filter((d) => d.groups.length);
}

export function sensorLabel(s: Sensor, t: Translate): string {
  const base = t(`hw-s-${s.label}`, { n: s.index ?? 0, detail: s.detail ?? "" });
  if (s.detail === "P" || s.detail === "E") return `${base} · ${t(`hw-core-${s.detail.toLowerCase()}`)}`;
  return base;
}

function trim(v: number, digits: number, locale: string): string {
  return v.toLocaleString(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

function bytes(v: number, locale: string): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return `${trim(v, u === 0 ? 0 : 1, locale)} ${units[u]}`;
}

export function formatValue(kind: SensorKind, v: number | null, locale = "en"): string {
  if (v == null || !Number.isFinite(v)) return "—";
  switch (kind) {
    case "temperature":
      return `${trim(v, 1, locale)} °C`;
    case "voltage":
      return `${trim(v, 3, locale)} V`;
    case "power":
      return `${trim(v, 1, locale)} W`;
    case "clock":
      return `${trim(v, 0, locale)} MHz`;
    case "load":
    case "level":
    case "control":
      return `${trim(v, 1, locale)} %`;
    case "fan":
      return `${trim(v, 0, locale)} RPM`;
    case "data":
      return bytes(v, locale);
    case "throughput":
      return `${bytes(v, locale)}/s`;
    case "energy":
      return `${trim(v / 1000, 1, locale)} Wh`;
  }
}

const TEMP_KIND: Record<DeviceKind, TempKind> = {
  cpu: "cpu",
  gpu: "gpu",
  storage: "disk",
  memory: "other",
  battery: "other",
  board: "other",
  network: "other",
};

/** A drive's own warning temperature, when it reports one. */
export function ownHot(snap: SensorSnapshot, device: string): number | undefined {
  return snap.sensors.find((s) => s.device === device && s.label === "disk-temp-warning")?.value ?? undefined;
}

/** Level against the part's limits, or a device's own hot limit (warm 10 below). */
export function levelFor(v: number, kind: TempKind, hot?: number): "ok" | "warm" | "hot" {
  if (hot == null) return tempLevel(v, kind);
  return v >= hot ? "hot" : v >= hot - 10 ? "warm" : "ok";
}

export function sensorTempLevel(
  s: Sensor,
  dev: SensorDevice | undefined,
  v: number | null,
  hot?: number,
): "ok" | "warm" | "hot" | null {
  // the limits themselves are not readings
  if (s.kind !== "temperature" || v == null || /-(warning|critical)$/.test(s.label)) return null;
  return levelFor(v, TEMP_KIND[dev?.kind ?? "board"], hot);
}

export interface HomeTemp {
  kind: "cpu" | "gpu" | "disk";
  value: number | null;
  /** Device name. */
  name?: string;
  /** Explains a missing value (`needs-pawnio`, `gpu-asleep`). */
  reason?: string;
  load?: number | null;
  /** The device's own hot limit (drives). */
  hot?: number;
}

function hottest(snap: SensorSnapshot, pred: (s: Sensor, d: SensorDevice | undefined) => boolean) {
  let best: { s: Sensor; d?: SensorDevice } | null = null;
  for (const s of snap.sensors) {
    const d = snap.devices.find((x) => x.id === s.device);
    if (s.kind !== "temperature" || s.value == null || !pred(s, d)) continue;
    if (!best || s.value > (best.s.value ?? -1)) best = { s, d };
  }
  return best;
}

/** CPU / GPU / hottest disk for the Home card. */
export function homeTemps(snap: SensorSnapshot): HomeTemp[] {
  const dev = (id: string) => snap.devices.find((d) => d.id === id);
  const cpuLoad = snap.sensors.find((s) => s.id === "cpu/cpu-total")?.value ?? null;
  const pkg = snap.sensors.find((s) => s.label === "cpu-package" && s.value != null);
  const cpu = pkg ? { s: pkg, d: dev(pkg.device) } : hottest(snap, (s, d) => d?.kind === "cpu" && s.primary);
  // an ACPI thermal zone is not the CPU (often a fixed firmware value): it stays
  // under its board in Sensors and never stands in for the CPU here
  const cpuTemp: HomeTemp = cpu
    ? { kind: "cpu", value: cpu.s.value, name: cpu.d?.name, load: cpuLoad }
    : { kind: "cpu", value: null, reason: snap.notes.find((n) => n.device === "cpu" && n.kind === "temperature")?.reason, load: cpuLoad };
  const gpu = hottest(snap, (_, d) => d?.kind === "gpu");
  const gpus = snap.devices.filter((d) => d.kind === "gpu");
  const asleep = snap.notes.find((n) => n.reason === "gpu-asleep");
  const gpuTemp: HomeTemp = gpu
    ? { kind: "gpu", value: gpu.s.value, name: gpu.d?.name }
    : { kind: "gpu", value: null, reason: asleep ? "gpu-asleep" : gpus.length ? "not-reported" : "none", name: asleep ? dev(asleep.device)?.name : undefined };
  // the drive closest to its own limit (default 70)
  let disk: { s: Sensor; d?: SensorDevice; hot?: number } | null = null;
  for (const s of snap.sensors) {
    if (s.label !== "disk-temp" || s.value == null) continue;
    const hot = ownHot(snap, s.device);
    const margin = (hot ?? 70) - s.value;
    if (!disk || margin < (disk.hot ?? 70) - (disk.s.value ?? 0)) disk = { s, d: dev(s.device), hot };
  }
  const diskTemp: HomeTemp = disk
    ? { kind: "disk", value: disk.s.value, name: disk.d?.name, hot: disk.hot }
    : { kind: "disk", value: null, reason: "not-reported" };
  return [cpuTemp, gpuTemp, diskTemp];
}

/** What Hardware → Sensors offers for the current Advanced sensors (PawnIO) state. */
export type AdvancedStep = "on" | "setup" | "start" | "not-running" | "restart" | "none";

/** "Start without the Windows prompt" (ADR 0003) is offered once Advanced sensors
 * are on, and stays visible while it is set up so it can always be removed. */
export function showNoPrompt(s: AdvancedStatus | null): boolean {
  const task = s?.helperTask;
  return !!task && (advancedStep(s) === "on" || task !== "off");
}

export function advancedStep(s: AdvancedStatus | null): AdvancedStep {
  if (!s?.pawnio) return "none";
  if (s.active) return "on";
  const p = s.pawnio;
  if (s.restart && !p.running) return "restart";
  if (!p.installed) return "setup";
  if (!p.running) return "not-running";
  if (!p.intel_module) return "setup";
  return "start";
}
