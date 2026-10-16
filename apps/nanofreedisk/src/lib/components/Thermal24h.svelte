<script lang="ts">
  // Hardware → Sensors: hottest CPU / GPU / disk per minute over the last 24 hours
  // (only minutes in which something sampled), plus the temperature-alert switch.
  import { onMount } from "svelte";
  import { t } from "../i18n.svelte";
  import { app } from "../state.svelte";
  import { hwApi } from "../hw/api";
  import { TEMP_LIMITS } from "../hw/overview";
  import type { ThermalMinute } from "../hw/types";

  const W = 720;
  const H = 120;
  const PARTS = [
    { key: "cpu", token: "--tm-1", limit: TEMP_LIMITS.cpu[1] },
    { key: "gpu", token: "--tm-3", limit: TEMP_LIMITS.gpu[1] },
    { key: "disk", token: "--tm-6", limit: TEMP_LIMITS.disk[1] },
  ] as const;

  let minutes = $state<ThermalMinute[]>([]);

  async function load() {
    try {
      minutes = (await hwApi.thermalStatus()).minutes;
    } catch {
      minutes = [];
    }
  }

  onMount(() => {
    void load();
    const timer = setInterval(load, 60_000);
    return () => clearInterval(timer);
  });

  const now = $derived(minutes.length ? minutes[minutes.length - 1].t : 0);
  const lo = 20;
  const hi = 105;
  const x = (tm: number) => W - ((now - tm) / (24 * 60)) * W;
  const y = (v: number) => H - ((Math.min(hi, Math.max(lo, v)) - lo) / (hi - lo)) * H;

  function path(key: "cpu" | "gpu" | "disk"): string {
    let d = "";
    let prev = -Infinity;
    for (const m of minutes) {
      const v = m[key];
      if (v == null) {
        prev = -Infinity;
        continue;
      }
      // gaps of more than 5 minutes (nothing sampled) break the line
      d += `${m.t - prev > 5 ? "M" : "L"}${x(m.t).toFixed(1)},${y(v).toFixed(1)}`;
      prev = m.t;
    }
    return d;
  }

  function peak(key: "cpu" | "gpu" | "disk") {
    let best: ThermalMinute | null = null;
    for (const m of minutes) if (m[key] != null && (!best || (m[key] ?? 0) > (best[key] ?? 0))) best = m;
    return best ? { value: Math.round(best[key] ?? 0), device: best.hottest[key] ?? "" } : null;
  }
</script>

<section class="card t24" aria-label={t("hw-24h-title")}>
  <header>
    <strong>{t("hw-24h-title")}</strong>
    <span class="legend">
      {#each PARTS as p (p.key)}
        {@const pk = peak(p.key)}
        <span class="lg" style="--c: var({p.token})" title={pk?.device}>
          <i></i>{t(`hw-part-${p.key}`)}{#if pk}<b>{t("hw-24h-peak", { value: pk.value })}</b>{/if}
        </span>
      {/each}
    </span>
    <span class="grow"></span>
    <label class="switch" title={t("hw-alerts-hint")}>
      <input type="checkbox" bind:checked={app.settings.tempAlerts} data-alerts />
      {t("hw-alerts")}
    </label>
  </header>
  {#if minutes.length < 2}
    <p class="muted small">{t("hw-24h-empty")}</p>
  {:else}
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label={t("hw-24h-title")}>
      {#each PARTS as p (p.key)}
        <line class="limit" x1="0" x2={W} y1={y(p.limit)} y2={y(p.limit)} style="--c: var({p.token})" />
        <path d={path(p.key)} style="--c: var({p.token})" />
      {/each}
    </svg>
    <div class="axis muted small"><span>{t("hw-24h-ago")}</span><span>{t("hw-24h-now")}</span></div>
  {/if}
</section>

<style>
  .t24 {
    padding: 10px 14px;
    margin-bottom: 12px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    margin-bottom: 6px;
  }
  .legend {
    display: flex;
    gap: 14px;
    font-size: 0.88em;
  }
  .lg {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .lg i {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: var(--c);
  }
  .lg b {
    margin-left: 4px;
    font-weight: 600;
  }
  .grow {
    flex: 1;
  }
  .switch {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.9em;
  }
  svg {
    width: 100%;
    height: 90px;
    display: block;
  }
  path {
    fill: none;
    stroke: var(--c);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  .limit {
    stroke: var(--c);
    stroke-dasharray: 4 4;
    opacity: 0.35;
    vector-effect: non-scaling-stroke;
  }
  .axis {
    display: flex;
    justify-content: space-between;
  }
  .small {
    font-size: 0.82em;
  }
</style>
