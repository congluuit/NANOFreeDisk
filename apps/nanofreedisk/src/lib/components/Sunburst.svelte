<script lang="ts">
  import { api } from "../api";
  import { CATEGORIES, categoryOf, readPalette } from "../colors";
  import { formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { hitArc, layoutSunburst, type Arcs } from "../sunburst";
  import { mix } from "../theme";
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

  const RINGS = 6;
  let canvas: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let slice = $state<Slice | null>(null);
  let arcs: Arcs | null = null;
  let hover = $state(-1);
  let themeTick = $state(0);
  const loc = $derived(intlLocale());

  $effect(() => {
    const id = rootId;
    void version;
    if (width < 10 || height < 10) return;
    // ~1/1500 of the circle is the smallest visible arc
    const minBytes = total / 1500;
    let cancelled = false;
    api
      .slice(id, RINGS, minBytes, 20_000)
      .then((s) => !cancelled && (slice = s))
      .catch(() => !cancelled && (slice = null));
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const obs = new MutationObserver(() => themeTick++);
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ["style", "data-theme"] });
    return () => obs.disconnect();
  });

  const geom = $derived.by(() => {
    const R = Math.max(10, Math.min(width, height) / 2 - 8);
    const r0 = R * 0.24;
    return { cx: width / 2, cy: height / 2, R, r0, ring: (R - r0) / RINGS };
  });

  $effect(() => {
    void themeTick;
    void selected;
    void hover;
    if (slice && width && height) draw(slice);
  });

  function draw(s: Slice) {
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const pal = readPalette();
    const bg = getComputedStyle(document.documentElement).getPropertyValue("--bg").trim() || "#000000";
    arcs = layoutSunburst(s, RINGS);
    const a = arcs;
    const { cx, cy, r0, ring } = geom;
    const sel = new Set(selected);
    // branch color: top-level child index → treemap palette, fading with depth
    const branch = new Int32Array(s.ids.length).fill(-1);
    let nextBranch = 0;
    for (let i = 1; i < s.ids.length; i++) {
      branch[i] = s.parents[i] === 0 ? nextBranch++ : branch[s.parents[i]];
    }
    const branchColors = CATEGORIES.map((c) => pal[c]);
    for (let d = 1; d < a.rings.length; d++) {
      for (const i of a.rings[d]) {
        const inner = r0 + (d - 1) * ring;
        const outer = inner + ring - 1;
        let fill: string;
        if (s.kinds[i] === 3) fill = pal.rest;
        else if (s.kinds[i] === 2) fill = pal.link;
        else if (s.kinds[i] === 0) fill = pal[categoryOf(s.names[i])];
        else fill = mix(branchColors[branch[i] % branchColors.length], bg, Math.max(0.28, 0.62 - d * 0.06));
        ctx.beginPath();
        ctx.arc(cx, cy, outer, a.start[i] - Math.PI / 2, a.end[i] - Math.PI / 2);
        ctx.arc(cx, cy, inner, a.end[i] - Math.PI / 2, a.start[i] - Math.PI / 2, true);
        ctx.closePath();
        ctx.fillStyle = fill;
        ctx.fill();
        ctx.strokeStyle = pal.border;
        ctx.lineWidth = 1;
        ctx.stroke();
        if (sel.has(s.ids[i]) || i === hover) {
          ctx.strokeStyle = sel.has(s.ids[i]) ? pal.selected : pal.hover;
          ctx.lineWidth = 2;
          ctx.stroke();
        }
      }
    }
    // center disc
    ctx.beginPath();
    ctx.arc(cx, cy, r0 - 2, 0, Math.PI * 2);
    ctx.fillStyle = pal.dir;
    ctx.fill();
    const focus = hover > 0 ? hover : 0;
    ctx.fillStyle = pal.text;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.font = `600 13px "Segoe UI", sans-serif`;
    const name = s.kinds[focus] === 3 ? t("analyze-small-items") : s.names[focus];
    ctx.fillText(clip(ctx, name, r0 * 1.7), cx, cy - 10);
    ctx.font = `12px "Segoe UI", sans-serif`;
    const share = s.sizes[0] ? s.sizes[focus] / s.sizes[0] : 0;
    ctx.fillText(`${formatSize(s.sizes[focus], loc)} · ${formatPercent(share, loc)}`, cx, cy + 10);
  }

  function clip(ctx: CanvasRenderingContext2D, text: string, max: number): string {
    let s = text;
    if (ctx.measureText(s).width <= max) return s;
    while (s.length > 1 && ctx.measureText(s + "…").width > max) s = s.slice(0, -1);
    return s + "…";
  }

  function at(e: MouseEvent): number {
    if (!arcs) return -1;
    const r = canvas.getBoundingClientRect();
    const dx = e.clientX - r.left - geom.cx;
    const dy = e.clientY - r.top - geom.cy;
    const dist = Math.hypot(dx, dy);
    if (dist < geom.r0) return 0;
    const ringIdx = Math.floor((dist - geom.r0) / geom.ring) + 1;
    if (ringIdx > RINGS) return -1;
    const angle = Math.atan2(dy, dx) + Math.PI / 2;
    return hitArc(arcs, angle, ringIdx);
  }

  function click(e: MouseEvent) {
    const i = at(e);
    if (i < 0 || !slice || slice.ids[i] === REST_ID) return;
    if (i === 0 && slice.parents.length) {
      onselect(slice.ids[0], false);
      return;
    }
    onselect(slice.ids[i], e.ctrlKey || e.metaKey);
  }

  function dbl(e: MouseEvent) {
    const i = at(e);
    if (i > 0 && slice && slice.kinds[i] === 1) onopen(slice.ids[i]);
  }

  function context(e: MouseEvent) {
    e.preventDefault();
    const i = at(e);
    if (i < 0 || !slice || slice.ids[i] === REST_ID) return;
    oncontext(slice.ids[i], e.clientX, e.clientY);
  }
</script>

<div class="sunburst" bind:clientWidth={width} bind:clientHeight={height}>
  <canvas
    bind:this={canvas}
    onmousemove={(e) => (hover = at(e))}
    onmouseleave={() => (hover = -1)}
    onclick={click}
    ondblclick={dbl}
    oncontextmenu={context}
    aria-label={t("analyze-view-sunburst")}
  ></canvas>
</div>

<style>
  .sunburst {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
  canvas {
    position: absolute;
    inset: 0;
    cursor: pointer;
  }
</style>
