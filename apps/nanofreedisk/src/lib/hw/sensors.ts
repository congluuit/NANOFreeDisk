// Hardware → Sensors: tree building, labels and value formatting. Pure (tested in
// hw.test.ts); the view only renders what this returns.

import type { Translate } from "./overview";
import { tempLevel, type TempKind } from "./overview.ts";
import type { DeviceKind, Sensor, SensorDevice, SensorKind, SensorNote, SensorSnapshot } from "./types";

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

/** Devices → kind groups → sensors; Simple mode keeps primary sensors only. */
export function tree(snap: SensorSnapshot, simple: boolean): DeviceNode[] {
  return snap.devices
    .map((device) => {
      const mine = snap.sensors.filter((s) => s.device === device.id && (!simple || s.primary));
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

export function sensorTempLevel(s: Sensor, dev: SensorDevice | undefined, v: number | null): "ok" | "warm" | "hot" | null {
  if (s.kind !== "temperature" || v == null) return null;
  return tempLevel(v, TEMP_KIND[dev?.kind ?? "board"]);
}

export interface HomeTemp {
  kind: "cpu" | "gpu" | "disk";
  value: number | null;
  /** Where the value comes from when it is not a real sensor of that part. */
  via?: "acpi";
  /** Device name. */
  name?: string;
  /** Explains a missing value (`needs-pawnio`, `gpu-asleep`). */
  reason?: string;
  load?: number | null;
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
  const cpu = hottest(snap, (s, d) => d?.kind === "cpu" && (s.label === "cpu-package" || s.primary));
  const acpi = hottest(snap, (_, d) => d?.kind === "board");
  const cpuTemp: HomeTemp = cpu
    ? { kind: "cpu", value: cpu.s.value, name: cpu.d?.name, load: cpuLoad }
    : acpi
      ? { kind: "cpu", value: acpi.s.value, via: "acpi", load: cpuLoad }
      : { kind: "cpu", value: null, reason: snap.notes.find((n) => n.device === "cpu" && n.kind === "temperature")?.reason, load: cpuLoad };
  const gpu = hottest(snap, (_, d) => d?.kind === "gpu");
  const gpus = snap.devices.filter((d) => d.kind === "gpu");
  const asleep = snap.notes.find((n) => n.reason === "gpu-asleep");
  const gpuTemp: HomeTemp = gpu
    ? { kind: "gpu", value: gpu.s.value, name: gpu.d?.name }
    : { kind: "gpu", value: null, reason: asleep ? "gpu-asleep" : gpus.length ? "not-reported" : "none", name: asleep ? dev(asleep.device)?.name : undefined };
  const disk = hottest(snap, (_, d) => d?.kind === "storage");
  const diskTemp: HomeTemp = disk ? { kind: "disk", value: disk.s.value, name: disk.d?.name } : { kind: "disk", value: null, reason: "not-reported" };
  return [cpuTemp, gpuTemp, diskTemp];
}
