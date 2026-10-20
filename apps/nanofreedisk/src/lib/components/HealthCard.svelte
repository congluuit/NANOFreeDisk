<script lang="ts">
  // Health Score on Home (PLAN §13 F1 / G1): one number, the three biggest reasons
  // and a button that goes where each one is fixed.
  import { api } from "../api";
  import { formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, startScan } from "../state.svelte";
  import { clean } from "../clean/state.svelte";
  import { dev } from "../devcache/state.svelte";
  import Icon from "./Icon.svelte";
  import type { Finding, HealthScore } from "../types";

  let health = $state<HealthScore | null>(null);
  const loc = $derived(intlLocale());
  const system = $derived(app.volumes.find((v) => v.system && v.ready));
  const shown = $derived(health?.findings.slice(0, 3) ?? []);
  // ring geometry
  const R = 42;
  const C = 2 * Math.PI * R;

  $effect(() => {
    void app.version;
    void app.volumes.length;
    void app.summary;
    // the rule scan adds the "junk" finding, the AI leftovers scan "ai-leftovers"
    void clean.scan;
    void dev.ai;
    let cancelled = false;
    api
      .health()
      .then((h) => !cancelled && (health = h))
      .catch(() => !cancelled && (health = null));
    return () => {
      cancelled = true;
    };
  });

  const drive = (s: string) => s.replace(/\\$/, "");

  function text(f: Finding): string {
    return t(`health-${f.kind}`, {
      drive: drive(f.subject) || (system ? drive(system.root) : ""),
      folder: f.subject,
      percent: formatPercent(f.value / 100, loc),
      days: Math.max(1, Math.round(f.value)),
      size: formatSize(f.bytes, loc),
      value: Math.round(f.value),
    });
  }

  function fix(f: Finding) {
    if (f.fix === "home") {
      const root = f.subject || system?.root;
      if (root) void startScan(root);
    } else if (f.fix === "oldlarge" && !app.summary) {
      void startScan(f.subject || system?.root || "");
    } else if (f.fix === "suggestions") {
      app.view = "analyze";
    } else {
      if (f.kind === "ai-leftovers") dev.tab = "ai";
      app.view = f.fix;
    }
  }
</script>

{#if health}
  <section class="health panel {health.grade}" aria-label={t("health-title")}>
    <div class="top">
      <div class="ring" title={t("health-basis")}>
        <svg viewBox="0 0 100 100" width="76" height="76" aria-hidden="true">
          <circle cx="50" cy="50" r={R} class="track" />
          <circle cx="50" cy="50" r={R} class="value" stroke-dasharray={`${(health.score / 100) * C} ${C}`} transform="rotate(-90 50 50)" />
        </svg>
        <div class="num"><b>{health.score}</b><span class="muted">/100</span></div>
      </div>
      <div class="head">
        <h2>{t("health-title")}</h2>
        <span class="tag grade">{t(`health-${health.grade}`)}</span>
      </div>
    </div>
    <div class="body">
      {#if shown.length}
        <ul>
          {#each shown as f, i (i)}
            <li class={f.severity}>
              <span class="dot"></span>
              <span class="txt">{text(f)}</span>
              {#if f.penalty}<span class="pts muted">−{f.penalty}</span>{/if}
              <button class="btn ghost small" onclick={() => fix(f)} title={f.fix === "home" || !app.summary ? t("hint-scan") : undefined}>{t(`health-fix-${f.fix}`)}<Icon name="chevron" size={14} /></button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="ok"><Icon name="check" size={16} />{t("health-all-good")}</p>
      {/if}
      <p class="muted small">{t("health-basis")}</p>
    </div>
  </section>
{/if}

<style>
  .health {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 18px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .ring {
    position: relative;
    flex: none;
    width: 76px;
    height: 76px;
  }
  circle {
    fill: none;
    stroke-width: 9;
  }
  .track {
    stroke: var(--surface-2);
  }
  .value {
    stroke: var(--risk-safe);
    stroke-linecap: round;
    transition: stroke-dasharray 0.4s;
  }
  .fair .value {
    stroke: var(--risk-caution);
  }
  .poor .value {
    stroke: var(--risk-protected);
  }
  .num {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    justify-items: center;
    line-height: 1.05;
  }
  .num b {
    font-size: 1.6em;
    letter-spacing: -0.02em;
  }
  .num span {
    font-size: 0.72em;
  }
  .head {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 1.06em;
  }
  .grade {
    background: var(--risk-safe-soft);
    color: var(--risk-safe-ink);
  }
  .fair .grade {
    background: var(--risk-caution-soft);
    color: var(--risk-caution-ink);
  }
  .poor .grade {
    background: var(--risk-protected-soft);
    color: var(--risk-protected-ink);
  }
  .body {
    min-width: 0;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 0;
    border-top: 1px solid var(--border);
    font-size: 0.92em;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-muted);
  }
  .warn .dot {
    background: var(--risk-caution);
  }
  .bad .dot {
    background: var(--risk-protected);
  }
  .txt {
    flex: 1;
    min-width: 0;
  }
  .pts {
    font-variant-numeric: tabular-nums;
    font-size: 0.9em;
  }
  li .btn {
    padding-right: 0.3em;
  }
  .ok {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--risk-safe-ink);
    margin: 4px 0;
  }
  .small {
    font-size: 0.8em;
    margin: 6px 0 0;
  }
</style>
