// Self-contained HTML report (PLAN §13 A6 / G10): one file, no scripts, no external
// resources, a static SVG treemap and the key tables, colored with the theme the user
// has on screen. Pure so it can be unit-tested; the UI gathers the data and saves the
// string (see `actions.ts`).

import { CATEGORIES, categoryOf, tokenOf } from "./colors.ts";
import { formatCount, formatDate, formatPercent, formatSize } from "./format.ts";
import { layoutTreemap } from "./treemap.ts";
import type { Detail, ExtRow, Located, Node, OldLargeRows, Slice, Summary } from "./types";

export type Translate = (id: string, args?: Record<string, string | number>) => string;

export interface ReportInput {
  /** The folder the report covers (usually the scan root). */
  folder: Detail;
  summary: Summary;
  slice: Slice;
  children: Node[];
  topFiles: Located[];
  extensions: ExtRow[];
  oldLarge: OldLargeRows | null;
  /** CSS variables (`paletteVars`), without the `--` prefix. */
  vars: Record<string, string>;
  dark: boolean;
  version: string;
  /** Unix milliseconds. */
  generatedAt: number;
  /** BCP-47 tag for number/date formatting and `<html lang>`. */
  locale: string;
}

const W = 1000;
const H = 560;

/** Escape text for HTML element content and attribute values. */
export function esc(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

const n1 = (v: number) => (Math.round(v * 10) / 10).toString();

/** Static treemap of `slice` as an SVG string (tiles with a tooltip each). */
export function treemapSvg(slice: Slice, t: Translate, locale: string): string {
  const l = layoutTreemap(slice, { x: 0, y: 0, w: W, h: H }, { pad: 2, header: 16, minFrame: 8 });
  const out: string[] = [];
  for (let k = 1; k < l.count; k++) {
    const i = l.order[k];
    const [x, y, w, h] = [l.x[i], l.y[i], l.w[i], l.h[i]];
    if (!(w >= 0.5 && h >= 0.5)) continue;
    const kind = slice.kinds[i];
    const name = kind === 3 ? t("analyze-small-items") : slice.names[i];
    const fill =
      kind === 1 ? "var(--tm-dir)" : kind === 2 ? "var(--tm-link)" : kind === 3 ? "var(--tm-rest)" : `var(${tokenOf(categoryOf(name))})`;
    const tip = `${name} — ${formatSize(slice.sizes[i], locale)}`;
    out.push(
      `<g><title>${esc(tip)}</title><rect x="${n1(x)}" y="${n1(y)}" width="${n1(w)}" height="${n1(h)}" fill="${fill}"/>`,
    );
    const label = kind === 1 ? w > 60 && h > 40 : w > 70 && h > 18;
    if (label) {
      const max = Math.floor((w - 8) / 6.4);
      const text = name.length > max ? `${name.slice(0, Math.max(1, max - 1))}…` : name;
      const cls = kind === 1 ? "dl" : "fl";
      out.push(`<text class="${cls}" x="${n1(x + 4)}" y="${n1(y + 12)}">${esc(text)}</text>`);
    }
    out.push("</g>");
  }
  return `<svg class="tm" viewBox="0 0 ${W} ${H}" role="img" aria-label="${esc(t("report-treemap"))}" xmlns="http://www.w3.org/2000/svg"><rect width="${W}" height="${H}" fill="var(--tm-border)"/>${out.join("")}</svg>`;
}

function table(head: string[], rows: string[][], numeric: number[]): string {
  const th = head.map((h, i) => `<th${numeric.includes(i) ? ' class="num"' : ""}>${esc(h)}</th>`).join("");
  const body = rows
    .map((r) => `<tr>${r.map((c, i) => `<td${numeric.includes(i) ? ' class="num"' : ""}>${c}</td>`).join("")}</tr>`)
    .join("\n");
  return `<table><thead><tr>${th}</tr></thead><tbody>\n${body}\n</tbody></table>`;
}

const bar = (ratio: number) =>
  `<span class="bar"><span style="width:${n1(Math.max(0, Math.min(1, ratio)) * 100)}%"></span></span>`;

/** The whole report as one HTML document. */
export function buildReport(input: ReportInput, t: Translate): string {
  const { folder, summary: s, locale } = input;
  const size = (b: number) => esc(formatSize(b, locale));
  const count = (c: number) => esc(formatCount(c, locale));
  const total = folder.onDisk || 1;
  const vars = Object.entries(input.vars)
    .filter(([k, v]) => /^[\w-]+$/.test(k) && !/[;{}<]/.test(v))
    .map(([k, v]) => `--${k}: ${v};`)
    .join(" ");

  const tiles: [string, string][] = [
    [t("stat-size"), size(folder.onDisk)],
    [t("stat-files"), count(folder.files)],
    [t("stat-dirs"), count(s.stats.dirs)],
  ];
  if (s.volumeTotal) tiles.push([t("report-drive-share"), esc(formatPercent(folder.onDisk / s.volumeTotal, locale))]);

  const legend = [
    `<span><i style="background:var(--tm-dir);border:1px solid var(--border)"></i>${esc(t("analyze-legend-folder"))}</span>`,
    ...CATEGORIES.map((c) => `<span><i style="background:var(${tokenOf(c)})"></i>${esc(t(`cat-${c}`))}</span>`),
    `<span><i style="background:var(--tm-other)"></i>${esc(t("cat-other"))}</span>`,
  ].join("");

  const children = table(
    [t("col-name"), t("col-size"), t("col-share"), ""],
    input.children.slice(0, 30).map((c) => [
      `<i class="sw" style="background:${c.kind === "dir" ? "var(--tm-dir);border:1px solid var(--border)" : `var(${tokenOf(categoryOf(c.name))})`}"></i>${esc(c.name)}`,
      size(c.onDisk),
      esc(formatPercent(c.onDisk / total, locale)),
      bar(c.onDisk / total),
    ]),
    [1, 2],
  );
  const files = table(
    [t("col-name"), t("col-size"), t("col-modified"), t("col-path")],
    input.topFiles.slice(0, 50).map((f) => [esc(f.name), size(f.onDisk), esc(formatDate(f.mtime, locale)), `<span class="path">${esc(f.path)}</span>`]),
    [1, 2],
  );
  const extMax = input.extensions[0]?.onDisk || 1;
  const exts = table(
    [t("col-type"), t("col-files"), t("col-size"), ""],
    input.extensions.slice(0, 25).map((x) => [
      `<i class="sw" style="background:var(${tokenOf(categoryOf(`a.${x.ext}`))})"></i>${esc(x.ext ? `.${x.ext}` : t("ext-none"))}`,
      count(x.files),
      size(x.onDisk),
      bar(x.onDisk / extMax),
    ]),
    [1, 2],
  );

  let oldLarge = "";
  if (input.oldLarge) {
    const ol = input.oldLarge;
    const rows: [string, { matched: number; matchedOnDisk: number }][] = [
      [t("ol-large-title"), ol.large],
      [t("ol-stale-title"), ol.stale],
      [t("ol-installers-title"), ol.installers],
    ];
    oldLarge = `<section><h2>${esc(t("nav-oldlarge"))}</h2><p class="muted">${esc(
      t("ol-summary", { files: ol.distinctFiles, size: formatSize(ol.distinctOnDisk, locale) }),
    )}</p>${table(
      [t("col-type"), t("col-files"), t("col-size")],
      rows.map(([label, r]) => [esc(label), count(r.matched), size(r.matchedOnDisk)]),
      [1, 2],
    )}</section>`;
  }

  const generated = new Date(input.generatedAt).toLocaleString(locale);
  const truncated = input.slice.truncated ? `<p class="muted">${esc(t("analyze-truncated"))}</p>` : "";
  return `<!doctype html>
<html lang="${esc(locale.split("-")[0])}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="NANOFreeDisk ${esc(input.version)}">
<title>${esc(t("report-title"))} — ${esc(folder.path)}</title>
<style>
:root { ${vars} color-scheme: ${input.dark ? "dark" : "light"}; }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--text); font: 14px/1.45 "Segoe UI", system-ui, -apple-system, sans-serif; }
main { max-width: 1100px; margin: 0 auto; padding: 28px 24px 40px; }
header { display: flex; justify-content: space-between; gap: 16px; align-items: flex-end; flex-wrap: wrap; border-bottom: 1px solid var(--border); padding-bottom: 14px; }
h1 { margin: 0; font-size: 1.5em; }
h1 b { color: var(--accent); }
h2 { font-size: 1.1em; margin: 0 0 10px; }
.muted { color: var(--text-muted); }
.path { font-family: "Cascadia Mono", Consolas, monospace; font-size: .9em; word-break: break-all; color: var(--text-muted); }
.tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(170px, 1fr)); gap: 12px; margin: 18px 0; }
.tile, section { background: var(--surface); border: 1px solid var(--border); border-radius: 10px; padding: 14px 16px; }
section { margin-top: 16px; overflow-x: auto; }
.tile b { display: block; font-size: 1.4em; }
.tm { width: 100%; height: auto; display: block; border-radius: 6px; }
.tm rect { stroke: var(--tm-border); stroke-width: .6; }
.tm text { font-size: 11px; pointer-events: none; fill: var(--tm-text); }
.tm .dl { font-weight: 600; }
.tm .fl { paint-order: stroke; stroke: var(--bg); stroke-width: 2.5px; stroke-opacity: .6; }
.legend { display: flex; flex-wrap: wrap; gap: 4px 14px; margin-top: 8px; font-size: .85em; color: var(--text-muted); }
.legend i, .sw { display: inline-block; width: 10px; height: 10px; border-radius: 2px; margin-right: 6px; vertical-align: -1px; }
table { width: 100%; border-collapse: collapse; }
th { text-align: left; font-weight: normal; font-size: .8em; text-transform: uppercase; color: var(--text-muted); border-bottom: 1px solid var(--border); padding: 6px 8px; }
td { padding: 4px 8px; border-bottom: 1px solid var(--border); vertical-align: top; }
.num { text-align: right; white-space: nowrap; font-variant-numeric: tabular-nums; }
.bar { display: inline-block; width: 120px; height: 6px; border-radius: 3px; background: var(--surface-2); overflow: hidden; vertical-align: middle; }
.bar span { display: block; height: 100%; background: var(--accent); }
footer { margin-top: 24px; font-size: .85em; color: var(--text-muted); }
@media print { section, .tile { break-inside: avoid; } }
</style>
</head>
<body>
<main>
<header>
<div><h1>NANO<b>FreeDisk</b> · ${esc(t("report-title"))}</h1><div class="path">${esc(folder.path)}</div></div>
<div class="muted">${esc(t("report-generated", { date: generated, version: input.version }))}</div>
</header>
<div class="tiles">${tiles.map(([k, v]) => `<div class="tile"><span class="muted">${esc(k)}</span><b>${v}</b></div>`).join("")}</div>
<section><h2>${esc(t("report-treemap"))}</h2>${treemapSvg(input.slice, t, locale)}<div class="legend">${legend}</div>${truncated}</section>
<section><h2>${esc(t("report-children"))}</h2>${children}</section>
<section><h2>${esc(t("analyze-tab-top"))}</h2>${files}</section>
<section><h2>${esc(t("analyze-tab-ext"))}</h2>${exts}</section>
${oldLarge}
<footer>${esc(t("report-footer", { version: input.version }))}</footer>
</main>
</body>
</html>
`;
}
