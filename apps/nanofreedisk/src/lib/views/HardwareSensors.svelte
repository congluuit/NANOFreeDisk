<script lang="ts">
  import { onMount } from "svelte";
  import { errorText } from "../api";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, expert, fail, toast } from "../state.svelte";
  import { hwApi } from "../hw/api";
  import { formatValue, ownHot, sensorLabel, sensorTempLevel, tree } from "../hw/sensors";
  import type { AdvancedStatus, DeviceKind, Sensor, SensorSnapshot } from "../hw/types";
  import Icon from "../components/Icon.svelte";
  import Thermal24h from "../components/Thermal24h.svelte";

  let snap = $state<SensorSnapshot | null>(null);
  let error = $state<string | null>(null);
  let collapsed = $state<Record<string, boolean>>({});
  let hover = $state<string | null>(null);
  let spark = $state<number[]>([]);
  let adv = $state<AdvancedStatus | null>(null);
  let starting = $state(false);

  const loc = $derived(intlLocale());
  const nodes = $derived(snap ? tree(snap, app.settings.mode === "simple") : []);
  const needsPawn = $derived(!!snap?.notes.some((n) => n.reason === "needs-pawnio"));

  const DEVICE_ICON: Record<DeviceKind, string> = {
    cpu: "cpu",
    gpu: "gpu",
    memory: "memory",
    storage: "drive",
    battery: "battery",
    board: "board",
    network: "network",
  };
  const KIND_ICON: Record<string, string> = {
    temperature: "thermo",
    voltage: "power",
    power: "power",
    clock: "clock",
    load: "gauge",
    fan: "fan",
    control: "fan",
    level: "battery",
    energy: "battery",
    data: "data",
    throughput: "data",
  };

  async function tick() {
    if (document.hidden) return;
    try {
      snap = await hwApi.sensors();
      error = null;
      if (hover) spark = await hwApi.history(hover, 60);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function refreshAdvanced() {
    try {
      adv = await hwApi.advancedStatus();
    } catch {
      adv = null;
    }
  }

  async function startAdvanced() {
    starting = true;
    try {
      adv = await hwApi.advancedStart();
      if (adv.unavailable) toast(t("hw-adv-unavailable", { reason: adv.unavailable }), "error");
      await tick();
    } catch (e) {
      const code = e && typeof e === "object" && "code" in e ? String((e as { code: unknown }).code) : "";
      toast(code === "uac-declined" ? t("hw-adv-declined") : t("hw-adv-failed", { message: errorText(e) }), "error");
    } finally {
      starting = false;
    }
  }

  onMount(() => {
    void refreshAdvanced();
    void tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  });

  async function enter(s: Sensor) {
    hover = s.id;
    try {
      spark = await hwApi.history(s.id, 60);
    } catch {
      spark = [];
    }
  }

  async function reset() {
    try {
      await hwApi.resetMinMax();
      await tick();
    } catch (e) {
      fail(e);
    }
  }

  function toggle(id: string) {
    collapsed[id] = !collapsed[id];
  }

  /** SVG polyline points for the last minute, scaled to its own range. */
  function points(values: number[], w = 90, h = 22): string {
    if (values.length < 2) return "";
    const lo = Math.min(...values);
    const hi = Math.max(...values);
    const span = hi - lo || 1;
    return values
      .map((v, i) => `${((i / (values.length - 1)) * w).toFixed(1)},${(h - 2 - ((v - lo) / span) * (h - 4)).toFixed(1)}`)
      .join(" ");
  }
</script>

<div class="toolbar">
  {#if adv?.active}
    <span class="adv on small" data-adv="on"><Icon name="check" size={14} />{t("hw-adv-on")}</span>
  {:else if needsPawn && adv?.pawnio}
    {@const p = adv.pawnio}
    <span class="hint small" data-adv="off">
      <Icon name="lock" size={14} />
      {#if !p.installed}
        {t("hw-adv-not-installed")}
      {:else if !p.running}
        {t("hw-adv-not-running")}
      {:else if !p.intel_module}
        {t("hw-adv-no-module")}
      {:else}
        {t("hw-sensors-pawn-hint")}
      {/if}
    </span>
    {#if !p.installed}
      <button class="btn" onclick={() => hwApi.pawnioSite().catch(fail)}><Icon name="open" />{t("hw-adv-get")}</button>
    {:else if p.running && p.intel_module}
      <button class="btn primary" onclick={startAdvanced} disabled={starting} title={t("hw-adv-start-hint")}>
        <Icon name="shield" />{starting ? t("hw-adv-starting") : t("hw-adv-start")}
      </button>
    {/if}
  {:else if needsPawn}
    <span class="hint small"><Icon name="lock" size={14} />{t("hw-sensors-pawn-hint")}</span>
  {/if}
  <span class="grow"></span>
  {#if snap && expert()}
    <span class="muted small">{t("hw-sensors-stats", { count: snap.sensors.length, ms: snap.sample_ms.toFixed(1) })}</span>
  {/if}
  <button class="btn" onclick={reset} disabled={!snap}><Icon name="refresh" />{t("hw-reset-minmax")}</button>
</div>

<Thermal24h />

<div class="table card" role="treegrid" aria-label={t("hw-tab-sensors")}>
  <div class="row head">
    <span>{t("hw-col-sensor")}</span>
    <span class="num">{t("hw-col-value")}</span>
    <span class="num">{t("hw-col-min")}</span>
    <span class="num">{t("hw-col-max")}</span>
    <span></span>
  </div>
  <div class="rows">
    {#if error}
      <p class="err pad">{t("hw-error", { message: error })}</p>
    {:else if !snap}
      <p class="muted pad">{t("hw-loading")}</p>
    {:else}
      {#each nodes as node (node.device.id)}
        {@const open = !collapsed[node.device.id]}
        <button class="row dev" aria-expanded={open} onclick={() => toggle(node.device.id)}>
          <span class="name">
            <span class="chev" class:open><Icon name="chevron" size={14} /></span>
            <Icon name={DEVICE_ICON[node.device.kind]} size={16} />
            <strong class="ellipsis">{node.device.name}</strong>
          </span>
        </button>
        {#if open}
          {#each node.groups as g (g.kind)}
            <div class="row grp">
              <span class="name"><Icon name={KIND_ICON[g.kind]} size={14} />{t(`hw-kind-${g.kind}`)}</span>
            </div>
            {#each g.sensors as s (s.id)}
              {@const own = snap ? ownHot(snap, node.device.id) : undefined}
              {@const lvl = sensorTempLevel(s, node.device, s.value, own)}
              {@const mx = sensorTempLevel(s, node.device, s.max, own)}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="row sensor"
                class:hovered={hover === s.id}
                data-sensor={s.id}
                onmouseenter={() => enter(s)}
                onmouseleave={() => (hover = null)}
                title={expert() ? `${s.id} · ${s.source}` : undefined}
              >
                <span class="name ellipsis">
                  {sensorLabel(s, t)}
                  {#if s.estimated}<span class="est" title={t("hw-estimated-hint")}>{t("hw-estimated")}</span>{/if}
                </span>
                <span class="num val {lvl ?? ''}">{formatValue(s.kind, s.value, loc)}</span>
                <span class="num muted">{formatValue(s.kind, s.min, loc)}</span>
                <span class="num {mx ?? 'muted'}">{formatValue(s.kind, s.max, loc)}</span>
                <span class="spark">
                  {#if hover === s.id && spark.length > 1}
                    <svg width="90" height="22" viewBox="0 0 90 22" aria-hidden="true">
                      <polyline points={points(spark)} />
                    </svg>
                  {/if}
                </span>
              </div>
            {/each}
            {#each g.notes as n (n.reason)}
              <div class="row note">
                <span class="name muted small">{t(`hw-note-${n.reason}`)}</span>
              </div>
            {/each}
          {/each}
        {/if}
      {:else}
        <p class="muted pad">{t("hw-none")}</p>
      {/each}
    {/if}
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }
  .grow {
    flex: 1;
  }
  .small {
    font-size: 0.86em;
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
  }
  .adv.on {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--risk-safe);
    font-weight: 600;
  }
  .table {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    margin-bottom: 24px;
    overflow: hidden;
  }
  .rows {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(200px, 1fr) 120px 120px 120px 100px;
    align-items: center;
    gap: 8px;
    min-height: var(--row);
    padding: 0 14px;
    width: 100%;
    border: none;
    background: transparent;
    text-align: left;
    color: inherit;
  }
  .head {
    font-weight: 600;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
    font-size: 0.88em;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .dev {
    border-top: 1px solid var(--border);
    cursor: pointer;
  }
  .dev:hover {
    background: var(--hover);
  }
  .chev {
    display: inline-flex;
    transition: transform 0.12s;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .grp .name {
    padding-left: 26px;
    color: var(--text-muted);
    font-size: 0.9em;
  }
  .sensor .name {
    padding-left: 50px;
    display: block;
  }
  .note .name {
    padding-left: 50px;
  }
  .sensor:hover,
  .sensor.hovered {
    background: var(--hover);
  }
  .val {
    font-weight: 600;
  }
  .ok {
    color: var(--temp-ok);
  }
  .warm {
    color: var(--temp-warm);
  }
  .hot {
    color: var(--temp-hot);
  }
  .est {
    margin-left: 6px;
    padding: 0 5px;
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 0.75em;
    color: var(--text-muted);
  }
  .spark {
    display: flex;
    justify-content: flex-end;
  }
  polyline {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
  }
  .pad {
    padding: 14px;
  }
  .err {
    color: var(--danger);
  }
</style>
