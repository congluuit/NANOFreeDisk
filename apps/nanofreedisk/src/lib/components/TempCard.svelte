<script lang="ts">
  // Home: CPU / GPU / hottest disk temperature. Polls the sensor hub every 3 s only
  // while Home is shown; a click opens Hardware → Sensors.
  import { onMount } from "svelte";
  import { t } from "../i18n.svelte";
  import { app } from "../state.svelte";
  import { hwApi } from "../hw/api";
  import { autoStartAdvanced, refreshAdvanced } from "../hw/advanced.svelte";
  import { homeTemps, levelFor, type HomeTemp } from "../hw/sensors";
  import { hw } from "../hw/state.svelte";
  import Icon from "./Icon.svelte";

  let temps = $state<HomeTemp[] | null>(null);

  async function tick() {
    if (document.hidden) return;
    try {
      temps = homeTemps(await hwApi.sensors());
    } catch {
      temps = null;
    }
  }

  onMount(() => {
    // "turn on automatically": the first view showing temperatures starts the helper
    void refreshAdvanced().then(autoStartAdvanced).then(tick);
    void tick();
    const timer = setInterval(tick, 3000);
    return () => clearInterval(timer);
  });

  function open() {
    hw.tab = "sensors";
    app.view = "hardware";
  }

  const ICON = { cpu: "cpu", gpu: "gpu", disk: "drive" } as const;
</script>

<button class="temps card" onclick={open} title={t("home-temps-open")}>
  <span class="head"><Icon name="thermo" size={18} /><strong>{t("home-temps-title")}</strong></span>
  {#each temps ?? [] as h (h.kind)}
    {@const lvl = h.value != null ? levelFor(h.value, h.kind, h.hot) : null}
    <span class="cell" data-temp={h.kind}>
      <span class="label muted"><Icon name={ICON[h.kind]} size={15} />{t(`home-temp-${h.kind}`)}</span>
      <b class="v {lvl ?? 'none'}">{h.value != null ? `${Math.round(h.value)} °C` : "—"}</b>
      <span class="sub muted ellipsis">
        {#if h.value == null}
          {t(`home-temp-why-${h.reason ?? "none"}`)}
        {:else}
          {h.name ?? ""}
        {/if}
        {#if h.kind === "cpu" && h.load != null}
          · {t("home-temp-load", { percent: Math.round(h.load) })}
        {/if}
      </span>
    </span>
  {:else}
    <span class="muted small">{t("hw-loading")}</span>
  {/each}
</button>

<style>
  .temps {
    display: grid;
    grid-template-columns: auto repeat(3, 1fr);
    align-items: center;
    gap: 18px;
    width: 100%;
    padding: 14px 18px;
    margin-bottom: 22px;
    text-align: left;
    color: inherit;
    transition: border-color 0.12s;
  }
  .temps:hover {
    border-color: var(--accent);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--accent);
  }
  .head strong {
    color: var(--text);
  }
  .cell {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.85em;
  }
  .v {
    font-size: 1.35em;
    font-variant-numeric: tabular-nums;
  }
  .v.ok {
    color: var(--temp-ok);
  }
  .v.warm {
    color: var(--temp-warm);
  }
  .v.hot {
    color: var(--temp-hot);
  }
  .v.none {
    color: var(--text-muted);
  }
  .sub {
    font-size: 0.8em;
  }
  .small {
    font-size: 0.86em;
  }
</style>
