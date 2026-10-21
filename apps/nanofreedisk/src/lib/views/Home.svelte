<script lang="ts">
  // Home: the figures of this PC at a glance (free space, what can go safely, the
  // last scan, the drives), the drives to analyze, and the PC's health next to them.
  import { pickFolder } from "../api";
  import { formatDateTime, formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, refreshVolumes, startScan } from "../state.svelte";
  import { clean, defaultPicks, scanJunk } from "../clean/state.svelte";
  import { advice, listenAdvice, loadAdvice } from "../advice/state.svelte";
  import HealthCard from "../components/HealthCard.svelte";
  import TempCard from "../components/TempCard.svelte";
  import AiCard from "../components/AiCard.svelte";
  import Icon from "../components/Icon.svelte";
  import StatCard from "../components/StatCard.svelte";
  import type { Volume } from "../types";
  import { currentOs, hasFeature } from "../platform";

  const os = $derived(currentOs(app.info?.os));

  const loc = $derived(intlLocale());
  const system = $derived(app.volumes.find((v) => v.system && v.ready));
  const ready = $derived(app.volumes.filter((v) => v.ready && v.total));
  const low = $derived(ready.filter((v) => level(v) !== "ok").length);

  // G1: what the rule packs would free safely, found in the background on Home
  $effect(() => {
    if (!clean.scan && !clean.scanning && !clean.running) void scanJunk();
  });

  // with a scan loaded, the free-space figure is the advice's: what the suggestions
  // can free in it (plus the junk the rules found elsewhere)
  $effect(() => {
    void listenAdvice();
  });
  $effect(() => {
    void app.summary;
    void app.version;
    if (app.summary) void loadAdvice();
  });
  const advised = $derived(
    app.summary && advice.summary?.suggestions
      ? advice.summary.suggestions.reclaim + advice.summary.elsewhere.filter((g) => g.verdict === "safe" && g.actionable).reduce((s, g) => s + g.bytes, 0)
      : null,
  );
  const freeable = $derived(advised ?? clean.scan?.safe ?? null);

  function freeSafely() {
    if (advised != null) {
      app.view = "analyze";
      return;
    }
    app.view = "clean";
    if (clean.scan) {
      const ids = defaultPicks(clean.scan);
      if (ids.length) clean.confirm = ids;
    }
  }

  async function scanFolder() {
    const path = await pickFolder(t("home-pick-folder"));
    if (path) await startScan(path);
  }

  function openScan() {
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
  const drive = (root: string) => root.replace(/\\$/, "");
</script>

<section class="page home">
  <header class="page-head">
    <div>
      <h1>{t("home-title")}</h1>
      <p>{t("home-subtitle")}</p>
    </div>
    <div class="page-actions actions-top">
      <button class="btn ghost icon" onclick={refreshVolumes} title={t("home-refresh")} aria-label={t("home-refresh")}>
        <Icon name="refresh" />
      </button>
      <button class="btn" onclick={scanFolder} data-effect title={t("hint-scan")}>
        <Icon name="folder" />{t("home-scan-folder")}
      </button>
      <button
        class="btn primary"
        data-home-free={advised != null ? "advice" : "rules"}
        onclick={freeSafely}
        disabled={!freeable}
        data-effect
        title={advised != null ? t("home-free-advice-sub") : t("home-free-hint")}
      >
        <Icon name="clean" />
        {#if freeable}
          {t("home-free-btn", { size: formatSize(freeable, loc) })}
        {:else if freeable === 0}
          {t("home-free-nothing")}
        {:else}
          {t("home-free-checking")}
        {/if}
      </button>
    </div>
  </header>

  <div class="stats">
    <StatCard
      label={system ? t("stat-free-on", { drive: drive(system.root) }) : t("stat-free")}
      value={system ? formatSize(system.free, loc) : null}
      unit={system ? t("stat-of", { total: formatSize(system.total, loc) }) : ""}
      tone={system && level(system) !== "ok" ? "caution" : null}
      data-stat="free"
    />
    <StatCard
      label={t("stat-safe")}
      tone="safe"
      value={freeable != null ? formatSize(freeable, loc) : null}
      unit={freeable == null ? t("stat-checking") : advised != null ? t("stat-safe-scan") : t("stat-safe-junk")}
      hint={advised != null ? t("home-free-advice-sub") : t("home-free-sub")}
      onclick={freeable ? freeSafely : undefined}
      data-stat="safe"
    />
    <StatCard
      label={t("stat-last-scan")}
      value={app.summary ? formatSize(app.summary.stats.on_disk, loc) : null}
      unit={app.summary ? `${drive(app.summary.root)} · ${formatDateTime(app.summary.scannedAt, loc)}` : t("stat-no-scan")}
      hint={t("home-action-analyze-hint")}
      onclick={app.summary || system ? openScan : undefined}
      data-stat="scan"
    />
    <StatCard
      label={t("stat-drives")}
      value={String(ready.length)}
      unit={low ? t("stat-drives-low", { count: low }) : t("stat-drives-ok")}
      tone={low ? "caution" : null}
      hint={hasFeature(os, "health") ? t("home-action-health-hint") : undefined}
      onclick={hasFeature(os, "health") ? () => (app.view = "health") : undefined}
      data-home-health
      data-stat="drives"
    />
  </div>

  <div class="cols">
    <section class="panel drives">
      <header class="panel-head">
        <div>
          <h2>{t("home-drives-title")}</h2>
          <p>{t("home-drives-sub")}</p>
        </div>
      </header>
      <ul>
        {#each app.volumes as v (v.root)}
          {@const used = v.total - v.free}
          <li class="drive" class:off={!v.ready}>
            <span class="dic"><Icon name={icon(v)} size={22} /></span>
            <div class="info">
              <div class="name">
                <strong class="ellipsis">{v.label || t(`drive-kind-${v.kind}`)} <span class="muted">({drive(v.root)})</span></strong>
                {#if v.system}<span class="tag accent">{t("drive-system")}</span>{/if}
              </div>
              {#if v.ready && v.total}
                <div class="bar usage {level(v)}"><span style="width: {(used / v.total) * 100}%"></span></div>
                <div class="nums">
                  <span>{t("drive-used", { used: formatSize(used, loc), total: formatSize(v.total, loc) })}</span>
                  <span class="muted">{t("drive-free", { free: formatSize(v.free, loc) })} · {formatPercent(v.free / v.total, loc)}</span>
                </div>
              {:else}
                <div class="muted nums">{t(`drive-kind-${v.kind}`)}{v.fs ? ` · ${v.fs}` : ""} · {t("drive-not-ready")}</div>
              {/if}
            </div>
            <button class="btn small" disabled={!v.ready} onclick={() => startScan(v.root)} data-effect title={t("hint-scan")}>
              <Icon name="analyze" size={15} />{t("drive-scan")}
            </button>
          </li>
        {:else}
          <li class="muted">{t("home-no-drives")}</li>
        {/each}
      </ul>
    </section>

    <div class="side-col">
      {#if hasFeature(os, "health")}<HealthCard />{/if}
      {#if hasFeature(os, "sensors")}<TempCard />{/if}
      <AiCard />
    </div>
  </div>
</section>

<style>
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1.35fr) minmax(320px, 1fr);
    gap: 14px;
    align-items: start;
  }
  .side-col {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }
  .side-col > :global(*) {
    margin: 0 !important;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .drive {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 0;
    border-bottom: 1px solid var(--border);
  }
  .drive:last-child {
    border-bottom: none;
    padding-bottom: 2px;
  }
  .drive.off {
    opacity: 0.6;
  }
  .dic {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .name strong {
    font-weight: 600;
  }
  .usage {
    height: 8px;
    border-radius: 4px;
  }
  .usage span {
    border-radius: 4px;
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
    font-size: 0.84em;
  }
  @media (max-width: 1100px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
