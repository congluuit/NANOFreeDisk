<script lang="ts">
  import { api } from "../api";
  import { CATEGORIES, categoryOf, readPalette } from "../colors";
  import { formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app } from "../state.svelte";
  import { tileStyle, type TileStyle } from "../mapstyle";
  import { hitTest, layoutTreemap, minBytesFor, type Layout } from "../treemap";
  import { REST_ID, type Slice } from "../types";
  import { VERDICTS, VERDICT_TOKEN, verdictOfCode } from "../advice";
  import VerdictBadge from "./VerdictBadge.svelte";

  interface Props {
    rootId: number;
    total: number;
    version: number;
    selected: number[];
    onselect: (id: number, additive: boolean) => void;
    onopen: (id: number) => void;
    oncontext: (id: number, x: number, y: number) => void;
    /** Tile colors: by file type, or by verdict (what can go / must stay). */
    colorBy?: "type" | "verdict";
  }
  let { rootId, total, version, selected, onselect, onopen, oncontext, colorBy = "type" }: Props = $props();

  let host: HTMLDivElement;
  let base: HTMLCanvasElement;
  let overlay: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let slice = $state<Slice | null>(null);
  let layout: Layout | null = null;
  /** Tiles drawn (a folder below the drawn levels is one tile of its own). */
  let shown: Uint8Array = new Uint8Array(0);
  let hover = $state(-1);
  let tip = $state({ x: 0, y: 0 });
  let stats = $state({ nodes: 0, layoutMs: 0, drawMs: 0 });
  let themeTick = $state(0);
  const loc = $derived(intlLocale());

  // Paint bucket per tile, computed once per slice: category (or verdict) index, then
  // "small items", links and what is safe to clear (green, hatched); DIR for frames.
  const BUCKETS = [...CATEGORIES, "other"] as const;
  const REST = BUCKETS.length;
  const LINK = BUCKETS.length + 1;
  const SAFE = BUCKETS.length + 2;
  const NB = SAFE + 1;
  const DIR = 255;
  // by verdict: bucket = verdict code (unclaimed tiles count as "unknown")
  const hasVerdicts = $derived(!!slice?.verdicts?.length);
  const byVerdict = $derived(colorBy === "verdict" && hasVerdicts);
  const paint = $derived.by(() => {
    const p = new Uint8Array(slice?.ids.length ?? 0);
    if (!slice) return p;
    const codes = slice.verdicts ?? [];
    for (let i = 0; i < p.length; i++) {
      const k = slice.kinds[i];
      if (k === 1) p[i] = DIR;
      else if (k === 2) p[i] = LINK;
      else if (k === 3) p[i] = REST;
      else if (hasVerdicts && codes[i] === 0) p[i] = SAFE;
      else if (byVerdict) p[i] = Math.min(codes[i] ?? 4, 4);
      else p[i] = BUCKETS.indexOf(categoryOf(slice.names[i]));
    }
    return p;
  });
  /** Bucket holding most of each folder's bytes (its frame takes that tint). */
  const dominant = $derived.by(() => {
    const n = slice?.ids.length ?? 0;
    const dom = new Uint8Array(n).fill(REST);
    if (!slice) return dom;
    const sums = new Float64Array(n * NB);
    for (let i = n - 1; i >= 1; i--) {
      const p = slice.parents[i];
      if (p < 0) continue;
      if (paint[i] === DIR) {
        for (let b = 0; b < NB; b++) sums[p * NB + b] += sums[i * NB + b];
      } else {
        sums[p * NB + paint[i]] += slice.sizes[i];
      }
    }
    for (let i = 0; i < n; i++) {
      if (paint[i] !== DIR) continue;
      let best: number = REST;
      let max = 0;
      for (let b = 0; b < NB; b++) {
        if (b === REST || b === LINK) continue;
        if (sums[i * NB + b] > max) {
          max = sums[i * NB + b];
          best = b;
        }
      }
      dom[i] = best;
    }
    return dom;
  });

  // refetch when the directory, the data or the size changes
  $effect(() => {
    const id = rootId;
    void version;
    const w = width;
    const h = height;
    if (w < 10 || h < 10) return;
    let cancelled = false;
    const handle = setTimeout(async () => {
      try {
        // tiles under ~5×5 px fold into "small items" (they only tint their folder)
        const s = await api.slice(id, 14, minBytesFor(total, w, h, 30), 60_000);
        if (!cancelled) slice = s;
      } catch {
        if (!cancelled) slice = null;
      }
    }, 60);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
  });

  // repaint on theme changes (tokens live on <html style>)
  $effect(() => {
    const obs = new MutationObserver(() => themeTick++);
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ["style", "data-theme"] });
    return () => obs.disconnect();
  });

  $effect(() => {
    void themeTick;
    void paint;
    void dominant;
    if (slice && width && height) draw(slice, width, height);
  });

  $effect(() => {
    void selected;
    void hover;
    drawOverlay();
  });

  function setup(c: HTMLCanvasElement, w: number, h: number): CanvasRenderingContext2D {
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
    c.style.width = `${w}px`;
    c.style.height = `${h}px`;
    const ctx = c.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return ctx;
  }

  /** Styles per bucket for the current theme. */
  function styles(): TileStyle[] {
    const pal = readPalette();
    const css = getComputedStyle(document.documentElement);
    const v = (name: string) => css.getPropertyValue(name).trim() || pal.other;
    const surface = v("--surface");
    const dark = document.documentElement.dataset.theme !== "light";
    const base = byVerdict
      ? [...VERDICTS.map((x) => v(VERDICT_TOKEN[x])), ...Array(BUCKETS.length - VERDICTS.length).fill(pal.other)]
      : BUCKETS.map((c) => pal[c]);
    return [...base, pal.other, pal.link, v("--risk-safe")].map((c) => tileStyle(c, surface, dark));
  }

  function hatchPattern(ctx: CanvasRenderingContext2D, color: string): CanvasPattern | null {
    const dpr = window.devicePixelRatio || 1;
    const size = 7;
    const c = document.createElement("canvas");
    c.width = c.height = Math.round(size * dpr);
    const g = c.getContext("2d");
    if (!g) return null;
    g.scale(dpr, dpr);
    g.strokeStyle = color;
    g.lineWidth = 1.4;
    g.beginPath();
    for (const o of [-size, 0, size]) {
      g.moveTo(o, size);
      g.lineTo(o + size, 0);
    }
    g.stroke();
    const pat = ctx.createPattern(c, "repeat");
    pat?.setTransform(new DOMMatrix([1 / dpr, 0, 0, 1 / dpr, 0, 0]));
    return pat;
  }

  function radius(w: number, h: number): number {
    return Math.max(0, Math.min(6, w / 5, h / 5));
  }

  function draw(s: Slice, w: number, h: number) {
    const t0 = performance.now();
    const fs = app.settings.fontScale;
    const l = layoutTreemap(s, { x: 0, y: 0, w, h }, { pad: 4, header: Math.round(22 * fs), minFrame: 8 });
    const t1 = performance.now();
    layout = l;
    const ctx = setup(base, w, h);
    const st = styles();
    // Folder frames depth by depth (a deeper frame lands on its parent), batched by
    // tint; then leaves batched by bucket (leaves never overlap each other). Tiles
    // are inset by 1 px so the parent shows through as a thin gap.
    //
    // Only two levels are drawn, like a person reads a disk: the folders of the
    // folder shown, and what is in them. A deeper folder is one tile in the tint of
    // what it mostly holds (double-click zooms in), unless it is big enough on screen
    // to open it too.
    const n = s.ids.length;
    const open = new Uint8Array(n);
    const vis = new Uint8Array(n);
    const bucket = new Uint8Array(n);
    open[0] = 1;
    vis[0] = 1;
    const big = 0.1 * w * h;
    const maxDepth = s.depths.reduce((m, d) => Math.max(m, d), 0);
    const frames: number[][] = Array.from({ length: maxDepth + 1 }, () => []);
    const leaves: number[][] = Array.from({ length: NB }, () => []);
    for (let k = 1; k < l.count; k++) {
      const i = l.order[k];
      if (!open[s.parents[i]] || !(l.w[i] > 0.6 && l.h[i] > 0.6)) continue;
      vis[i] = 1;
      const d = s.depths[i] - s.depths[0];
      const room = l.head[i] && (d < 2 || (d < 6 && l.w[i] * l.h[i] >= big));
      if (paint[i] === DIR && room) {
        open[i] = 1;
        frames[s.depths[i]].push(i);
      } else {
        bucket[i] = paint[i] === DIR ? dominant[i] : paint[i];
        leaves[bucket[i]].push(i);
      }
    }
    shown = vis;
    const shape = (list: number[], big: boolean) => {
      ctx.beginPath();
      for (const i of list) {
        const rw = l.w[i];
        const rh = l.h[i];
        const isBig = rw >= 8 && rh >= 8;
        if (isBig !== big) continue;
        if (big) ctx.roundRect(l.x[i] + 1, l.y[i] + 1, rw - 2, rh - 2, radius(rw, rh));
        else if (rw > 2.5 && rh > 2.5) ctx.rect(l.x[i] + 0.5, l.y[i] + 0.5, rw - 1, rh - 1);
        else ctx.rect(l.x[i], l.y[i], rw, rh);
      }
    };
    ctx.lineWidth = 1;
    for (const list of frames) {
      if (!list.length) continue;
      for (let b = 0; b < NB; b++) {
        const mine = list.filter((i) => dominant[i] === b);
        if (!mine.length) continue;
        ctx.fillStyle = st[b].frame;
        ctx.strokeStyle = st[b].frameEdge;
        shape(mine, true);
        ctx.fill();
        ctx.stroke();
        shape(mine, false);
        ctx.fill();
      }
    }
    for (let b = 0; b < NB; b++) {
      const list = leaves[b];
      if (!list.length) continue;
      ctx.fillStyle = st[b].fill;
      ctx.strokeStyle = st[b].edge;
      shape(list, true);
      ctx.fill();
      ctx.stroke();
      const pat = b === SAFE ? hatchPattern(ctx, st[b].hatch) : null;
      if (pat) {
        ctx.fillStyle = pat;
        ctx.fill();
      }
      ctx.fillStyle = st[b].fill;
      shape(list, false);
      ctx.fill();
    }
    // labels where they can be read (hover shows the rest): a folder's name and size
    // in its label strip; a tile's name top-left and its size big bottom-left
    ctx.textBaseline = "middle";
    const font = (px: number, weight = 400) => `${weight} ${px * fs}px "Segoe UI Variable Text", "Segoe UI", sans-serif`;
    for (let k = 1; k < l.count; k++) {
      const i = l.order[k];
      const x = l.x[i];
      const y = l.y[i];
      const rw = l.w[i];
      const rh = l.h[i];
      if (!vis[i]) continue;
      if (open[i]) {
        if (rw < 70) continue;
        const sty = st[dominant[i]];
        const mid = y + 4 + 9 * fs;
        const size = formatSize(s.sizes[i], loc);
        ctx.font = font(11);
        const sw = ctx.measureText(size).width;
        const room = rw - 18 - sw;
        ctx.font = font(12, 600);
        ctx.fillStyle = sty.frameInk;
        label(ctx, s.names[i], x + 9, mid, room > 40 ? room - 8 : rw - 18);
        if (room > 40) {
          ctx.font = font(11);
          ctx.globalAlpha = 0.8;
          ctx.fillText(size, x + rw - 9 - sw, mid);
          ctx.globalAlpha = 1;
        }
      } else if (rw > 52 && rh > 26) {
        const sty = st[bucket[i]];
        ctx.fillStyle = sty.ink;
        ctx.font = font(12, 600);
        const name = s.kinds[i] === 3 ? t("analyze-small-items") : s.names[i];
        label(ctx, name, x + 9, y + 6 + 8 * fs, rw - 18);
        if (rh > 50) {
          const px = Math.max(13, Math.min(rw / 6, rh / 3.2, 30));
          ctx.font = font(px, 700);
          ctx.textBaseline = "alphabetic";
          label(ctx, formatSize(s.sizes[i], loc), x + 9, y + rh - 9, rw - 18);
          ctx.textBaseline = "middle";
        }
      }
    }
    stats = { nodes: s.ids.length, layoutMs: t1 - t0, drawMs: performance.now() - t1 };
    drawOverlay();
  }

  function label(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, max: number) {
    if (max < 12) return;
    let s = text;
    if (ctx.measureText(s).width > max) {
      while (s.length > 1 && ctx.measureText(s + "…").width > max) s = s.slice(0, -1);
      s += "…";
    }
    ctx.fillText(s, x, y);
  }

  function drawOverlay() {
    if (!overlay || !width || !height) return;
    const ctx = setup(overlay, width, height);
    if (!layout || !slice) return;
    const pal = readPalette();
    const sel = new Set(selected);
    const l = layout;
    const ring = (i: number, inset: number) => {
      ctx.beginPath();
      ctx.roundRect(l.x[i] + inset, l.y[i] + inset, Math.max(0, l.w[i] - 2 * inset), Math.max(0, l.h[i] - 2 * inset), radius(l.w[i], l.h[i]));
      ctx.stroke();
    };
    ctx.lineWidth = 2.5;
    ctx.strokeStyle = pal.selected;
    for (let k = 0; k < l.count; k++) {
      const i = l.order[k];
      if (i > 0 && shown[i] && sel.has(slice.ids[i])) ring(i, 1.5);
    }
    if (hover > 0) {
      ctx.strokeStyle = pal.text;
      ctx.globalAlpha = 0.7;
      ctx.lineWidth = 1.5;
      ring(hover, 1);
    }
  }

  function at(e: MouseEvent): number {
    if (!layout || !slice) return -1;
    const r = overlay.getBoundingClientRect();
    let i = hitTest(layout, e.clientX - r.left, e.clientY - r.top);
    // what is inside a folder drawn as one tile belongs to that tile
    while (i > 0 && !shown[i]) i = slice.parents[i];
    return i;
  }

  function move(e: MouseEvent) {
    const i = at(e);
    hover = i;
    const r = host.getBoundingClientRect();
    tip = { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  function click(e: MouseEvent) {
    const i = at(e);
    if (i < 0 || !slice || slice.ids[i] === REST_ID) return;
    onselect(slice.ids[i], e.ctrlKey || e.metaKey);
  }

  function dbl(e: MouseEvent) {
    const i = at(e);
    if (i <= 0 || !slice) return;
    // zoom into the directory under the cursor (or the file's folder)
    let j = i;
    while (j > 0 && slice.kinds[j] !== 1) j = slice.parents[j];
    if (j > 0) onopen(slice.ids[j]);
  }

  function context(e: MouseEvent) {
    e.preventDefault();
    const i = at(e);
    if (i < 0 || !slice || slice.ids[i] === REST_ID) return;
    oncontext(slice.ids[i], e.clientX, e.clientY);
  }

  const hovered = $derived(
    slice && hover >= 0
      ? {
          name: slice.kinds[hover] === 3 ? t("analyze-small-items") : slice.names[hover],
          size: slice.sizes[hover],
          share: slice.sizes[0] ? slice.sizes[hover] / slice.sizes[0] : 0,
          kind: slice.kinds[hover],
          verdict: verdictOfCode(slice.verdicts?.[hover]),
        }
      : null,
  );
</script>

<div
  class="treemap"
  bind:this={host}
  bind:clientWidth={width}
  bind:clientHeight={height}
  data-nodes={stats.nodes}
  data-layout-ms={stats.layoutMs.toFixed(1)}
  data-draw-ms={stats.drawMs.toFixed(1)}
>
  <canvas bind:this={base}></canvas>
  <canvas
    bind:this={overlay}
    class="overlay"
    onmousemove={move}
    onmouseleave={() => (hover = -1)}
    onclick={click}
    ondblclick={dbl}
    oncontextmenu={context}
    aria-label={t("analyze-view-treemap")}
  ></canvas>
  {#if hovered && hover > 0}
    <div
      class="tip"
      style="left: {Math.min(tip.x + 14, width - 240)}px; top: {Math.min(tip.y + 14, height - 60)}px"
    >
      <strong class="ellipsis">{hovered.name}</strong>
      <span>{formatSize(hovered.size, loc)} · {formatPercent(hovered.share, loc)}</span>
      {#if hovered.verdict}<span><VerdictBadge verdict={hovered.verdict} long /></span>{/if}
    </div>
  {/if}
  {#if slice?.truncated}
    <div class="note muted">{t("analyze-truncated")}</div>
  {/if}
</div>

<style>
  .treemap {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
  canvas {
    position: absolute;
    inset: 0;
  }
  .overlay {
    cursor: pointer;
  }
  .tip {
    position: absolute;
    pointer-events: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-width: 230px;
    padding: 8px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow);
    font-size: 0.9em;
  }
  .note {
    position: absolute;
    right: 8px;
    bottom: 6px;
    font-size: 0.8em;
    background: var(--surface);
    padding: 2px 8px;
    border-radius: 4px;
    pointer-events: none;
  }
</style>
