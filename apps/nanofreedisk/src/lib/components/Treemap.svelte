<script lang="ts">
  import { api } from "../api";
  import { CATEGORIES, categoryOf, readPalette } from "../colors";
  import { formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app } from "../state.svelte";
  import { bestText } from "../theme";
  import { hitTest, layoutTreemap, minBytesFor, type Layout } from "../treemap";
  import { REST_ID, type Slice } from "../types";

  interface Props {
    rootId: number;
    total: number;
    version: number;
    selected: number[];
    onselect: (id: number, additive: boolean) => void;
    onopen: (id: number) => void;
    oncontext: (id: number, x: number, y: number) => void;
  }
  let { rootId, total, version, selected, onselect, onopen, oncontext }: Props = $props();

  let host: HTMLDivElement;
  let base: HTMLCanvasElement;
  let overlay: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let slice = $state<Slice | null>(null);
  let layout: Layout | null = null;
  let hover = $state(-1);
  let tip = $state({ x: 0, y: 0 });
  let stats = $state({ nodes: 0, layoutMs: 0, drawMs: 0 });
  let themeTick = $state(0);
  const loc = $derived(intlLocale());

  // Paint bucket per tile, computed once per slice: category index, then "small
  // items", then links; DIR for directory frames.
  const BUCKETS = [...CATEGORIES, "other"] as const;
  const REST = BUCKETS.length;
  const LINK = BUCKETS.length + 1;
  const DIR = 255;
  const paint = $derived.by(() => {
    const p = new Uint8Array(slice?.ids.length ?? 0);
    if (!slice) return p;
    for (let i = 0; i < p.length; i++) {
      const k = slice.kinds[i];
      p[i] = k === 1 ? DIR : k === 3 ? REST : k === 2 ? LINK : BUCKETS.indexOf(categoryOf(slice.names[i]));
    }
    return p;
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
        const s = await api.slice(id, 14, minBytesFor(total, w, h, 5), 60_000);
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

  function draw(s: Slice, w: number, h: number) {
    const t0 = performance.now();
    const l = layoutTreemap(s, { x: 0, y: 0, w, h });
    const t1 = performance.now();
    layout = l;
    const ctx = setup(base, w, h);
    const pal = readPalette();
    const textOn = new Map<string, string>();
    const textFor = (c: string) => {
      let v = textOn.get(c);
      if (!v) textOn.set(c, (v = bestText(c)));
      return v;
    };
    ctx.fillStyle = pal.border;
    ctx.fillRect(0, 0, w, h);
    // Paint in buckets of one color (fewer fillStyle switches: ~15 ms for 50k tiles).
    // Directories first, in paint order, so nested frames land on their parents;
    // leaves never overlap each other, so their order does not matter.
    const fills = [...BUCKETS.map((c) => pal[c]), pal.rest, pal.link];
    const tile = (i: number) => {
      const rw = l.w[i];
      const rh = l.h[i];
      if (rw > 3 && rh > 3) ctx.fillRect(l.x[i] + 0.5, l.y[i] + 0.5, rw - 1, rh - 1);
      else if (rw > 0.6 && rh > 0.6) ctx.fillRect(l.x[i], l.y[i], rw, rh);
    };
    ctx.fillStyle = pal.dir;
    for (let k = 0; k < l.count; k++) if (paint[l.order[k]] === DIR) tile(l.order[k]);
    for (let b = 0; b < fills.length; b++) {
      ctx.fillStyle = fills[b];
      for (let k = 0; k < l.count; k++) if (paint[l.order[k]] === b) tile(l.order[k]);
    }
    // labels on tiles large enough to read
    ctx.font = `${12 * app.settings.fontScale}px "Segoe UI", sans-serif`;
    ctx.textBaseline = "middle";
    for (let k = 1; k < l.count; k++) {
      const i = l.order[k];
      const x = l.x[i];
      const y = l.y[i];
      const rw = l.w[i];
      const rh = l.h[i];
      if (paint[i] === DIR) {
        if (rw > 60 && rh > 40) {
          ctx.fillStyle = pal.text;
          label(ctx, `${s.names[i]}  ${formatSize(s.sizes[i], loc)}`, x + 5, y + 9, rw - 10);
        }
      } else if (rw > 54 && rh > 18) {
        ctx.fillStyle = textFor(fills[paint[i]]);
        const name = s.kinds[i] === 3 ? t("analyze-small-items") : s.names[i];
        label(ctx, name, x + 5, y + 10, rw - 10);
        if (rh > 34) label(ctx, formatSize(s.sizes[i], loc), x + 5, y + 25, rw - 10);
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
    ctx.lineWidth = 2;
    ctx.strokeStyle = pal.selected;
    for (let k = 0; k < layout.count; k++) {
      const i = layout.order[k];
      if (i > 0 && sel.has(slice.ids[i])) {
        ctx.strokeRect(layout.x[i] + 1, layout.y[i] + 1, layout.w[i] - 2, layout.h[i] - 2);
      }
    }
    if (hover > 0) {
      ctx.strokeStyle = pal.hover;
      ctx.globalAlpha = 0.85;
      ctx.lineWidth = 1.5;
      ctx.strokeRect(layout.x[hover] + 0.75, layout.y[hover] + 0.75, layout.w[hover] - 1.5, layout.h[hover] - 1.5);
    }
  }

  function at(e: MouseEvent): number {
    if (!layout) return -1;
    const r = overlay.getBoundingClientRect();
    return hitTest(layout, e.clientX - r.left, e.clientY - r.top);
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
    border-radius: var(--radius-sm);
    background: var(--bg);
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
    padding: 6px 10px;
    background: var(--surface-2);
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
