// Drive health report (PLAN §4.6 "export JSON/HTML to send to tech support"): one
// self-contained HTML file — no scripts, no external resources — colored with the
// theme on screen. Serial numbers only when the user asks for them.

import { esc } from "../report.ts";
import { formatSize } from "../format.ts";
import type { HealthReport } from "./types";
import { hours, nvmeRows, reasonArgs, statusToken, volumeRows, type Translate } from "./view.ts";

export interface HealthReportInput {
  report: HealthReport;
  /** CSS variables (`paletteVars`), without the `--` prefix. */
  vars: Record<string, string>;
  dark: boolean;
  version: string;
  locale: string;
  serials: boolean;
}

function table(head: string[], rows: string[][], numeric: number[] = []): string {
  const th = head.map((h, i) => `<th${numeric.includes(i) ? ' class="num"' : ""}>${esc(h)}</th>`).join("");
  const body = rows
    .map((r) => `<tr>${r.map((c, i) => `<td${numeric.includes(i) ? ' class="num"' : ""}>${c}</td>`).join("")}</tr>`)
    .join("");
  return `<table><thead><tr>${th}</tr></thead><tbody>${body}</tbody></table>`;
}

export function buildHealthReport(input: HealthReportInput, t: Translate): string {
  const { report: r, locale } = input;
  const vars = Object.entries(input.vars)
    .filter(([k, v]) => /^[\w-]+$/.test(k) && !/[;{}<]/.test(v))
    .map(([k, v]) => `--${k}: ${v};`)
    .join(" ");
  const drives = r.drives
    .map((d) => {
      const facts: [string, string][] = [
        [t("health-col-bus"), d.bus],
        [t("health-col-size"), d.size ? formatSize(d.size, locale) : "—"],
        [t("health-firmware"), d.firmware ?? "—"],
      ];
      if (input.serials && d.serial) facts.push([t("health-serial"), d.serial]);
      if (d.temperature != null) facts.push([t("health-temperature"), `${Math.round(d.temperature)} °C`]);
      if (d.lifeLeft != null) facts.push([t("health-life"), `${d.lifeLeft} %`]);
      if (d.powerOnHours != null) facts.push([t("health-power-on"), hours(d.powerOnHours, t, locale)]);
      if (d.powerCycles != null) facts.push([t("health-power-cycles"), String(d.powerCycles)]);
      if (d.bytesWritten != null) facts.push([t("health-written"), formatSize(d.bytesWritten, locale)]);
      facts.push(...nvmeRows(d, t, locale, true));
      const reasons = d.reasons.length
        ? `<ul>${d.reasons.map((x) => `<li>${esc(t(`health-reason-${x.key}`, reasonArgs(x, d)))}</li>`).join("")}</ul>`
        : "";
      const note = d.note ? `<p class="muted">${esc(t(`health-note-${d.note}`))}</p>` : "";
      const attrs = d.ata?.attributes.length
        ? table(
            ["ID", t("health-attr-name"), t("health-attr-current"), t("health-attr-worst"), t("health-attr-threshold"), t("health-attr-raw")],
            d.ata.attributes.map((a) => [
              a.id.toString(16).toUpperCase().padStart(2, "0"),
              esc(a.name ?? t("health-attr-vendor")),
              String(a.current),
              String(a.worst),
              a.threshold == null ? "—" : String(a.threshold),
              String(a.raw),
            ]),
            [2, 3, 4, 5],
          )
        : "";
      return `<section><h2>${esc(d.model)} <span class="badge" style="color:var(${statusToken(d.status)});border-color:var(${statusToken(d.status)})">${esc(
        t(`health-status-${d.status}`),
      )}</span></h2>${reasons}${note}<dl>${facts.map(([k, v]) => `<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join("")}</dl>${attrs}</section>`;
    })
    .join("\n");
  const volumes = r.volumes
    .map((v) => {
      const d = r.drives.find((x) => x.index === v.disk);
      const rows = volumeRows(v, d, t, locale);
      return `<section><h2>${esc(v.root)} ${v.label ? `<span class="muted">${esc(v.label)}</span>` : ""}</h2><dl>${rows
        .map(([k, val, bad]) => `<dt>${esc(k)}</dt><dd${bad ? ' class="bad"' : ""}>${esc(val)}</dd>`)
        .join("")}</dl></section>`;
    })
    .join("\n");
  const generated = new Date(r.generatedAt * 1000).toLocaleString(locale);
  return `<!doctype html>
<html lang="${esc(locale.split("-")[0])}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="NANOFreeDisk ${esc(input.version)}">
<title>${esc(t("health-report-title"))}</title>
<style>
:root { ${vars} color-scheme: ${input.dark ? "dark" : "light"}; }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--text); font: 14px/1.45 "Segoe UI", system-ui, -apple-system, sans-serif; }
main { max-width: 1000px; margin: 0 auto; padding: 28px 24px 40px; }
header { border-bottom: 1px solid var(--border); padding-bottom: 14px; }
h1 { margin: 0; font-size: 1.5em; }
h1 b { color: var(--accent); }
h2 { font-size: 1.1em; margin: 0 0 10px; display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
.muted { color: var(--text-muted); }
.badge { border: 1px solid; border-radius: 999px; padding: 0 10px; font-size: .85em; }
section { background: var(--surface); border: 1px solid var(--border); border-radius: 10px; padding: 14px 16px; margin-top: 16px; overflow-x: auto; }
dl { display: grid; grid-template-columns: minmax(180px, auto) 1fr; gap: 3px 14px; margin: 0 0 10px; }
dt { color: var(--text-muted); }
dd { margin: 0; }
dd.bad { color: var(--risk-protected); font-weight: 600; }
table { width: 100%; border-collapse: collapse; }
th { text-align: left; font-weight: normal; font-size: .8em; text-transform: uppercase; color: var(--text-muted); border-bottom: 1px solid var(--border); padding: 6px 8px; }
td { padding: 4px 8px; border-bottom: 1px solid var(--border); }
.num { text-align: right; font-variant-numeric: tabular-nums; }
footer { margin-top: 24px; font-size: .85em; color: var(--text-muted); }
@media print { section { break-inside: avoid; } }
</style>
</head>
<body>
<main>
<header>
<h1>NANO<b>FreeDisk</b> · ${esc(t("health-report-title"))}</h1>
<div class="muted">${esc(t("report-generated", { date: generated, version: input.version }))}${
    r.elevated ? "" : ` · ${esc(t("health-report-standard-user"))}`
  }</div>
</header>
<h2 style="margin-top:20px">${esc(t("health-drives"))}</h2>
${drives || `<p class="muted">${esc(t("health-no-drives"))}</p>`}
<h2 style="margin-top:20px">${esc(t("health-volumes"))}</h2>
${volumes}
<footer>${esc(t("health-report-footer"))}</footer>
</main>
</body>
</html>
`;
}
