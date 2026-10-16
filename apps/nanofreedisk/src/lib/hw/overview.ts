// Hardware → Overview: turns the inventory into labelled sections, used both for the
// cards and for "Copy as text" so the two always match. Pure (tested in hw.test.ts).

import type { HwInventory } from "./types";

export type Translate = (id: string, args?: Record<string, string | number>) => string;

export interface Row {
  label: string;
  value: string;
  /** Temperature in °C, colored by level in the UI. */
  temp?: { c: number; kind: TempKind };
  /** Only shown in Expert mode. */
  expert?: boolean;
}

export interface Item {
  heading?: string;
  tag?: string;
  rows: Row[];
}

export interface Section {
  id: "system" | "cpu" | "board" | "memory" | "gpu" | "storage" | "display" | "network" | "battery";
  icon: string;
  title: string;
  items: Item[];
  chips?: string[];
}

export type TempKind = "cpu" | "gpu" | "disk" | "other";

/** Warm / hot thresholds in °C per component (conservative, below throttle points). */
export const TEMP_LIMITS: Record<TempKind, [number, number]> = {
  cpu: [80, 95],
  gpu: [80, 90],
  disk: [60, 70],
  other: [70, 85],
};

export function tempLevel(c: number, kind: TempKind): "ok" | "warm" | "hot" {
  const [warm, hot] = TEMP_LIMITS[kind];
  return c >= hot ? "hot" : c >= warm ? "warm" : "ok";
}

export interface Fmt {
  size: (bytes: number) => string;
  number: (n: number) => string;
}

const join = (...parts: (string | null | undefined)[]) => parts.filter(Boolean).join(" ");

function uptime(t: Translate, secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return t("hw-uptime-value", { days: d, hours: h, minutes: m });
}

function mhz(t: Translate, v: number | null): string | null {
  if (!v) return null;
  return v >= 1000 ? t("hw-ghz", { value: (v / 1000).toFixed(2) }) : t("hw-mhz", { value: v });
}

export function sections(inv: HwInventory, t: Translate, f: Fmt): Section[] {
  const out: Section[] = [];
  const row = (label: string, value: string | null | undefined, expert = false): Row[] =>
    value ? [{ label: t(label), value, expert }] : [];

  const s = inv.system;
  out.push({
    id: "system",
    icon: "monitor",
    title: t("hw-sec-system"),
    items: [
      {
        rows: [
          ...row("hw-model", join(s.manufacturer, s.product) || null),
          ...row("hw-chassis", s.chassis ? t(`hw-chassis-${s.chassis}`) : null),
          ...row("hw-os", join(s.os_name, s.os_version)),
          ...row("hw-os-build", s.os_build),
          ...row("hw-computer", s.computer, true),
          ...row("hw-uptime", uptime(t, s.uptime_secs)),
        ],
      },
    ],
  });

  const c = inv.cpu;
  const cores =
    c.p_cores != null && c.e_cores != null
      ? t("hw-cores-hybrid", { cores: c.cores, p: c.p_cores, e: c.e_cores })
      : String(c.cores);
  const caches = [c.l1 && `L1 ${f.size(c.l1)}`, c.l2 && `L2 ${f.size(c.l2)}`, c.l3 && `L3 ${f.size(c.l3)}`].filter(Boolean).join(" · ");
  out.push({
    id: "cpu",
    icon: "cpu",
    title: t("hw-sec-cpu"),
    items: [
      {
        heading: c.name || c.vendor,
        rows: [
          ...row("hw-cores", cores),
          ...row("hw-threads", String(c.threads)),
          ...row("hw-clock-base", mhz(t, c.base_mhz)),
          ...row("hw-clock-max", mhz(t, c.max_mhz)),
          ...row("hw-cache", caches || null),
          ...row("hw-socket", c.socket, true),
          ...row("hw-cpuid", `${t("hw-family")} ${c.family.toString(16).toUpperCase()}h · ${t("hw-model-id")} ${c.model.toString(16).toUpperCase()}h · ${t("hw-stepping")} ${c.stepping}`, true),
        ],
      },
    ],
    chips: c.features,
  });

  const b = inv.board;
  out.push({
    id: "board",
    icon: "board",
    title: t("hw-sec-board"),
    items: [
      {
        rows: [
          ...row("hw-board", join(b.manufacturer, b.product) || null),
          ...row("hw-board-version", b.version, true),
          ...row("hw-bios", join(b.bios_vendor, b.bios_version) || null),
          ...row("hw-bios-date", b.bios_date),
          ...row("hw-smbios", b.smbios_version, true),
        ],
      },
    ],
  });

  const m = inv.memory;
  const used = m.total - m.available;
  out.push({
    id: "memory",
    icon: "memory",
    title: t("hw-sec-memory"),
    items: [
      {
        rows: [
          ...row("hw-mem-installed", m.installed ? f.size(m.installed) : null),
          ...row("hw-mem-usable", f.size(m.total)),
          ...row("hw-mem-in-use", m.total ? t("hw-mem-in-use-value", { used: f.size(used), percent: Math.round((used / m.total) * 100) }) : null),
          ...row("hw-mem-slots", m.slots ? t("hw-mem-slots-value", { used: m.modules.length, total: m.slots }) : null),
          ...row("hw-mem-max", m.max_capacity ? f.size(m.max_capacity) : null, true),
        ],
      },
      ...m.modules.map((d) => ({
        heading: join(f.size(d.size), d.kind, d.form_factor),
        tag: d.locator ?? undefined,
        rows: [
          ...row("hw-mem-speed", d.speed_mts ? t("hw-mts", { value: d.configured_mts ?? d.speed_mts }) : null),
          ...row("hw-mem-rated", d.speed_mts && d.configured_mts && d.configured_mts !== d.speed_mts ? t("hw-mts", { value: d.speed_mts }) : null),
          ...row("hw-maker", d.manufacturer),
          ...row("hw-part", d.part_number),
          ...row("hw-rank", d.rank ? String(d.rank) : null, true),
          ...row("hw-bank", d.bank, true),
        ],
      })),
    ],
  });

  out.push({
    id: "gpu",
    icon: "gpu",
    title: t("hw-sec-gpu"),
    items: inv.gpus.map((g) => ({
      heading: g.name,
      rows: [
        ...row("hw-vram", g.vram ? f.size(g.vram) : null),
        ...row("hw-driver", g.driver_version),
        ...row("hw-driver-date", g.driver_date),
        ...row("hw-location", g.location, true),
      ],
    })),
  });

  out.push({
    id: "storage",
    icon: "drive",
    title: t("hw-sec-storage"),
    items: inv.disks.map((d) => ({
      heading: d.model || t("hw-disk-n", { n: d.index }),
      tag: t("hw-disk-n", { n: d.index }),
      rows: [
        ...row("hw-capacity", d.size ? f.size(d.size) : null),
        ...row("hw-disk-type", join(d.bus, d.ssd == null ? null : d.ssd ? "SSD" : "HDD", d.removable ? t("hw-removable") : null)),
        ...(d.temperature_c != null
          ? [{ label: t("hw-temperature"), value: `${Math.round(d.temperature_c)} °C`, temp: { c: d.temperature_c, kind: "disk" as const } }]
          : []),
        ...row("hw-firmware", d.firmware, true),
      ],
    })),
  });

  out.push({
    id: "display",
    icon: "screen",
    title: t("hw-sec-display"),
    items: inv.displays.map((d) => ({
      heading: d.name ?? (d.connector === "Internal" || d.connector === "eDP" ? t("hw-display-builtin") : t("hw-display-generic")),
      tag: d.primary ? t("hw-primary") : undefined,
      rows: [
        ...row("hw-resolution", d.width ? `${d.width} × ${d.height}` : null),
        ...row("hw-refresh-rate", d.refresh_hz ? t("hw-hz", { value: Math.round(d.refresh_hz * 100) / 100 }) : null),
        ...row("hw-connector", d.connector),
      ],
    })),
  });

  out.push({
    id: "network",
    icon: "network",
    title: t("hw-sec-network"),
    items: inv.network.map((n) => ({
      heading: n.name,
      tag: t(`hw-net-${n.kind === "wifi" ? "wifi" : "ethernet"}`),
      rows: [
        ...row("hw-net-status", n.connected ? (n.link_bps ? t("hw-net-up-speed", { mbps: Math.round(n.link_bps / 1e6) }) : t("hw-net-up")) : t("hw-net-down")),
        ...row("hw-net-alias", n.alias, true),
      ],
    })),
  });

  if (inv.batteries.length) {
    out.push({
      id: "battery",
      icon: "battery",
      title: t("hw-sec-battery"),
      items: inv.batteries.map((bat) => {
        const health = bat.design_mwh && bat.full_mwh ? Math.min(100, Math.round((bat.full_mwh / bat.design_mwh) * 100)) : null;
        return {
          heading: join(bat.manufacturer, bat.name) || t("hw-sec-battery"),
          rows: [
            ...row("hw-bat-charge", bat.charge_percent != null ? `${Math.round(bat.charge_percent)} %` : null),
            ...row("hw-bat-health", health != null ? t("hw-bat-health-value", { percent: health, wear: 100 - health }) : null),
            ...row("hw-bat-capacity", bat.design_mwh ? t("hw-bat-capacity-value", { full: f.number(bat.full_mwh), design: f.number(bat.design_mwh) }) : null),
            ...row("hw-bat-cycles", bat.cycles != null ? String(bat.cycles) : null),
            ...row("hw-bat-chemistry", bat.chemistry, true),
          ],
        };
      }),
    });
  }
  return out;
}

/** "Copy as text": the same sections as the cards, in the UI language. */
export function asText(list: Section[], expert: boolean, header: string): string {
  const lines = [header, ""];
  for (const s of list) {
    if (!s.items.length) continue;
    lines.push(`== ${s.title} ==`);
    for (const it of s.items) {
      if (it.heading) lines.push(it.tag ? `${it.heading} (${it.tag})` : it.heading);
      for (const r of it.rows) if (expert || !r.expert) lines.push(`  ${r.label}: ${r.value}`);
    }
    if (s.chips?.length) lines.push(`  ${s.chips.join(", ")}`);
    lines.push("");
  }
  return lines.join("\n");
}
