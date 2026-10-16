<script lang="ts">
  import { pickFolder } from "../api";
  import { formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, refreshVolumes, startScan } from "../state.svelte";
  import HealthCard from "../components/HealthCard.svelte";
  import Icon from "../components/Icon.svelte";
  import type { Volume } from "../types";

  const loc = $derived(intlLocale());
  const system = $derived(app.volumes.find((v) => v.system && v.ready));

  async function scanFolder() {
    const path = await pickFolder(t("home-pick-folder"));
    if (path) await startScan(path);
  }

  function analyzeSystem() {
    if (app.summary) app.view = "analyze";
    else if (system) void startScan(system.root);
  }

  const icon = (v: Volume) =>
    v.kind === "removable" ? "usb" : v.kind === "network" ? "network" : v.kind === "optical" ? "disc" : "drive";

  function level(v: Volume): "ok" | "warn" | "bad" {
    if (!v.total) return "ok";
    const free = v.free / v.total;
    return free < 0.1 ? "bad" : free < 0.2 ? "warn" : "ok";
  }
</script>

<section class="page">
  <header>
    <div>
      <h1>{t("home-title")}</h1>
      <p class="muted">{t("home-subtitle")}</p>
    </div>
    <div class="actions-top">
      <button class="btn ghost" onclick={refreshVolumes} title={t("home-refresh")}>
        <Icon name="refresh" />
      </button>
      <button class="btn primary" onclick={scanFolder} data-effect title={t("hint-scan")}>
        <Icon name="folder" />{t("home-scan-folder")}
      </button>
    </div>
  </header>

  <HealthCard />

  <div class="big">
    <button class="action card" disabled title={t("home-action-free-hint")}>
      <span class="ic safe"><Icon name="clean" size={26} /></span>
      <strong>{t("home-action-free")}</strong>
      <span class="muted">{t("home-action-free-hint")}</span>
    </button>
    <button class="action card" onclick={analyzeSystem} disabled={!system && !app.summary} data-effect title={t("hint-scan")}>
      <span class="ic"><Icon name="analyze" size={26} /></span>
      <strong>{t("home-action-analyze")}</strong>
      <span class="muted">{t("home-action-analyze-hint")}</span>
    </button>
    <button class="action card" disabled title={t("home-action-health-hint")}>
      <span class="ic health"><Icon name="health" size={26} /></span>
      <strong>{t("home-action-health")}</strong>
      <span class="muted">{t("home-action-health-hint")}</span>
    </button>
  </div>

  <div class="drives">
    {#each app.volumes as v (v.root)}
      {@const used = v.total - v.free}
      <article class="drive card" class:off={!v.ready}>
        <div class="top">
          <span class="dic"><Icon name={icon(v)} size={26} /></span>
          <div class="ellipsis">
            <div class="title ellipsis">
              {v.label || t(`drive-kind-${v.kind}`)} <span class="muted">({v.root.replace(/\\$/, "")})</span>
            </div>
            <div class="muted small">
              {t(`drive-kind-${v.kind}`)}{v.fs ? ` · ${v.fs}` : ""}
              {#if v.system}<span class="tag">{t("drive-system")}</span>{/if}
            </div>
          </div>
        </div>
        {#if v.ready && v.total}
          <div class="bar usage {level(v)}"><span style="width: {(used / v.total) * 100}%"></span></div>
          <div class="nums small">
            <span>{t("drive-used", { used: formatSize(used, loc), total: formatSize(v.total, loc) })}</span>
            <span class="muted">{t("drive-free", { free: formatSize(v.free, loc) })} · {formatPercent(v.free / v.total, loc)}</span>
          </div>
        {:else}
          <div class="muted small">{t("drive-not-ready")}</div>
        {/if}
        <button class="btn" disabled={!v.ready} onclick={() => startScan(v.root)} data-effect title={t("hint-scan")}>
          <Icon name="analyze" />{t("drive-scan")}
        </button>
      </article>
    {:else}
      <p class="muted">{t("home-no-drives")}</p>
    {/each}
  </div>
</section>

<style>
  .page {
    height: 100%;
    overflow: auto;
    padding: 28px 32px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 22px;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 1.6em;
  }
  header p {
    margin: 0;
    max-width: 60ch;
  }
  .actions-top {
    display: flex;
    gap: 8px;
  }
  .big {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 14px;
    margin-bottom: 26px;
  }
  .action {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 18px;
    text-align: left;
    transition: border-color 0.12s, transform 0.12s;
  }
  .action:hover:not(:disabled) {
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .action:disabled {
    cursor: default;
    opacity: 0.7;
  }
  .action strong {
    font-size: 1.05em;
  }
  .action .muted {
    font-size: 0.88em;
  }
  .ic {
    display: grid;
    place-items: center;
    width: 46px;
    height: 46px;
    border-radius: 12px;
    background: var(--accent-soft);
    color: var(--accent);
    margin-bottom: 6px;
  }
  .ic.safe {
    color: var(--risk-safe);
  }
  .ic.health {
    color: var(--risk-advanced);
  }
  .drives {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 14px;
  }
  .drive {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
  }
  .drive.off {
    opacity: 0.6;
  }
  .drive .btn {
    align-self: flex-start;
  }
  .top {
    display: flex;
    gap: 12px;
    align-items: center;
    min-width: 0;
  }
  .dic {
    color: var(--text-muted);
  }
  .title {
    font-weight: 600;
  }
  .small {
    font-size: 0.86em;
  }
  .tag {
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--text);
    font-size: 0.85em;
  }
  .usage {
    height: 10px;
    border-radius: 5px;
  }
  .usage span {
    border-radius: 5px;
  }
  .usage.warn span {
    background: var(--risk-caution);
  }
  .usage.bad span {
    background: var(--risk-protected);
  }
  .nums {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
</style>
