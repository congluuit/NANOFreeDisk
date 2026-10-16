// Pure helpers for the Health view and its HTML report: tones, message arguments,
// the rows of the NVMe log and the volume checks. No Svelte, so they are unit-tested.

import { formatCount, formatPercent, formatSize } from "../format.ts";
import type { Drive, DriveStatus, Heat, HealthReport, HealthVolume, Reason } from "./types";

export type Translate = (id: string, args?: Record<string, string | number>) => string;

/** Theme token behind a verdict (risk colors, so every theme keeps them readable). */
export function statusToken(s: DriveStatus): string {
  switch (s) {
    case "good":
      return "--risk-safe";
    case "caution":
      return "--risk-caution";
    case "bad":
      return "--risk-protected";
    default:
      return "--text-muted";
  }
}

/** Class used for temperatures (same as the Hardware view). */
export function heatClass(h: Heat): string {
  return h === "critical" ? "hot" : h === "warning" ? "warm" : h === "normal" ? "ok" : "";
}

const hex = (id: number) => id.toString(16).toUpperCase().padStart(2, "0");

/** Arguments of a `health-reason-*` message. */
export function reasonArgs(r: Reason, d: Drive): Record<string, string | number> {
  const a = d.ata?.attributes.find((x) => x.id === r.attr);
  return {
    value: Math.round(r.value ?? 0),
    attr: r.attr != null ? hex(r.attr) : "",
    name: a?.name ?? "",
  };
}

/** Hours as "4 287 h (179 days)". */
export function hours(h: number, t: Translate, locale: string): string {
  return t("health-hours", { hours: formatCount(h, locale), days: Math.round(h / 24) });
}

/** Worst verdict over all drives ("unknown" when there are none). */
export function worst(r: HealthReport): DriveStatus {
  const order: DriveStatus[] = ["unknown", "good", "caution", "bad"];
  return r.drives.reduce<DriveStatus>((w, d) => (order.indexOf(d.status) > order.indexOf(w) ? d.status : w), "unknown");
}

/** Label/value rows of the NVMe health log; `full` adds the counters Simple mode hides. */
export function nvmeRows(d: Drive, t: Translate, locale: string, full: boolean): [string, string][] {
  const n = d.nvme;
  if (!n) return [];
  const rows: [string, string, boolean][] = [
    [t("health-nvme-warning"), n.criticalWarning ? `0x${hex(n.criticalWarning)}` : t("health-none"), false],
    [t("health-nvme-spare"), t("health-nvme-spare-value", { spare: n.availableSpare, threshold: n.spareThreshold }), false],
    [t("health-nvme-used"), `${n.percentageUsed} %`, false],
    [t("health-nvme-media-errors"), formatCount(n.mediaErrors, locale), false],
    [t("health-nvme-unsafe"), formatCount(n.unsafeShutdowns, locale), false],
    [t("health-nvme-read"), formatSize(n.dataUnitsRead * 512_000, locale), true],
    [t("health-nvme-written"), formatSize(n.dataUnitsWritten * 512_000, locale), true],
    [t("health-nvme-host-reads"), formatCount(n.hostReads, locale), true],
    [t("health-nvme-host-writes"), formatCount(n.hostWrites, locale), true],
    [t("health-nvme-busy"), t("health-minutes", { n: formatCount(n.controllerBusy, locale) }), true],
    [t("health-nvme-error-log"), formatCount(n.errorLogEntries, locale), true],
    [t("health-nvme-warn-time"), t("health-minutes", { n: formatCount(n.warningTempMinutes, locale) }), true],
    [t("health-nvme-crit-time"), t("health-minutes", { n: formatCount(n.criticalTempMinutes, locale) }), true],
    [t("health-nvme-sensors"), n.sensors.map((c) => `${c} °C`).join(" · ") || "—", true],
  ];
  return rows.filter((r) => full || !r[2]).map(([a, b]) => [a, b]);
}

/** Free-space forecast text of a volume. */
export function forecast(v: HealthVolume, t: Translate): string {
  if (v.daysToFull != null) return t("health-vol-full-in", { days: Math.max(1, Math.round(v.daysToFull)) });
  return v.hasTrend ? t("health-vol-not-filling") : t("health-vol-no-trend");
}

const yesNo = (b: boolean | null, t: Translate) => (b == null ? "—" : t(b ? "health-yes" : "health-no"));

/** Checks of a volume as [label, value, problem] rows. */
export function volumeRows(v: HealthVolume, d: Drive | undefined, t: Translate, locale: string): [string, string, boolean][] {
  const free = v.total ? v.free / v.total : 1;
  const ssd = d?.ssd === true;
  return [
    [t("health-vol-free"), `${formatSize(v.free, locale)} (${formatPercent(free, locale)})`, free < 0.1],
    [t("health-vol-forecast"), forecast(v, t), v.daysToFull != null && v.daysToFull < 30],
    [t("health-vol-fs"), v.fs || "—", false],
    [t("health-vol-cluster"), v.cluster ? formatSize(v.cluster, locale) : "—", false],
    [t("health-vol-aligned"), yesNo(v.aligned, t), v.aligned === false],
    // TRIM only matters on an SSD that supports it
    [t("health-vol-trim"), yesNo(v.trimEnabled, t), ssd && d?.trim === true && v.trimEnabled === false],
    [t("health-vol-bitlocker"), v.bitlocker ? t(`health-bitlocker-${v.bitlocker}`) : "—", false],
    [t("health-vol-dirty"), v.dirty == null ? t("health-not-checked") : yesNo(v.dirty, t), v.dirty === true],
  ];
}

/** The equivalent CLI command (Expert mode). */
export function cliCommand(elevated: boolean): string {
  return elevated ? "nfd health --raw   # in an administrator terminal" : "nfd health --raw";
}
