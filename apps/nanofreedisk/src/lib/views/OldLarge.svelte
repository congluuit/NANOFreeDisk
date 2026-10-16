<script lang="ts">
  // Old & Large (PLAN §13 A2): very large files, large files untouched for a long
  // time and old installers in Downloads. The user picks; deleting goes through the
  // usual Recycle Bin dialog and its safety checks.
  import { api } from "../api";
  import { itemMenu } from "../actions";
  import { nfdScan, sizeFlag } from "../cli";
  import { formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, askDelete, expert, fail, oldLargeOptions } from "../state.svelte";
  import CliHint from "../components/CliHint.svelte";
  import ContextMenu, { type MenuItem } from "../components/ContextMenu.svelte";
  import Icon from "../components/Icon.svelte";
  import ItemTable from "../components/ItemTable.svelte";
  import type { OldLargeRows, SearchRows } from "../types";

  type List = "large" | "stale" | "installers";
  const LISTS: { id: List; icon: string }[] = [
    { id: "large", icon: "drive" },
    { id: "stale", icon: "refresh" },
    { id: "installers", icon: "export" },
  ];
  const GB = [0.5, 1, 2, 5, 10, 20, 50];
  const YEARS = [0.5, 1, 2, 3, 5];
  const MIN_MB = [10, 50, 100, 500, 1024];
  const DAYS = [7, 30, 90, 180, 365];

  let list = $state<List>("large");
  let data = $state<OldLargeRows | null>(null);
  let loading = $state(false);
  let onlyFolder = $state(false);
  let selected = $state<number[]>([]);
  let menu: { x: number; y: number; items: MenuItem[] } | null = $state(null);
  const loc = $derived(intlLocale());
  const rootId = $derived(app.summary?.rootId ?? 0);
  const scope = $derived(onlyFolder ? app.current : rootId);
  const rows = $derived<SearchRows | null>(data ? data[list] : null);
  const selectedSize = $derived(
    (rows?.rows ?? []).filter((r) => selected.includes(r.id)).reduce((s, r) => s + r.onDisk, 0),
  );

  $effect(() => {
    void app.version;
    const id = scope;
    const { largeGb, staleYears, staleMinMb, installerDays } = app.settings;
    void [largeGb, staleYears, staleMinMb, installerDays];
    if (!app.summary) return;
    let cancelled = false;
    loading = true;
    api
      .oldAndLarge(id, oldLargeOptions())
      .then((r) => {
        if (cancelled) return;
        data = r;
        const live = new Set([...r.large.rows, ...r.stale.rows, ...r.installers.rows].map((x) => x.id));
        selected = selected.filter((x) => live.has(x));
      })
      .catch(fail)
      .finally(() => !cancelled && (loading = false));
    return () => {
      cancelled = true;
    };
  });

  function toggle(id: number, additive: boolean) {
    if (additive) selected = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
    else selected = selected.length === 1 && selected[0] === id ? [] : [id];
  }

  function context(id: number, x: number, y: number) {
    if (!selected.includes(id)) selected = [id];
    menu = { x, y, items: itemMenu(id, { selection: selected }) };
  }

  const fmtYears = (y: number) => (y < 1 ? t("ol-months", { count: Math.round(y * 12) }) : t("ol-years", { count: y }));
  const fmtGb = (g: number) => formatSize(g * 2 ** 30, loc);
</script>

{#if !app.summary}
  <div class="empty">
    <Icon name="drive" size={48} />
    <p class="muted">{t("analyze-empty")}</p>
    <button class="btn primary" onclick={() => (app.view = "home")}>{t("nav-home")}</button>
  </div>
{:else}
  <section class="page">
    <header>
      <div class="ellipsis">
        <h1>{t("ol-title")}</h1>
        <p class="muted">{t("ol-intro")}</p>
      </div>
      {#if app.current !== rootId}
        <label class="check">
          <input type="checkbox" bind:checked={onlyFolder} />
          {t("ol-only-folder")}
        </label>
      {/if}
    </header>

    <div class="controls card">
      <label>
        <span>{t("ol-large-min")}</span>
        <select bind:value={app.settings.largeGb}>
          {#each GB as g (g)}<option value={g}>{fmtGb(g)}</option>{/each}
        </select>
      </label>
      <label>
        <span>{t("ol-stale-age")}</span>
        <select bind:value={app.settings.staleYears}>
          {#each YEARS as y (y)}<option value={y}>{fmtYears(y)}</option>{/each}
        </select>
      </label>
      {#if expert()}
        <label>
          <span>{t("ol-stale-min")}</span>
          <select bind:value={app.settings.staleMinMb}>
            {#each MIN_MB as m (m)}<option value={m}>{formatSize(m * 2 ** 20, loc)}</option>{/each}
          </select>
        </label>
        <label>
          <span>{t("ol-installer-age")}</span>
          <select bind:value={app.settings.installerDays}>
            {#each DAYS as d (d)}<option value={d}>{t("ol-days", { count: d })}</option>{/each}
          </select>
        </label>
      {/if}
      <span class="grow"></span>
      {#if data}
        <strong class="total">
          {t("ol-summary", { files: data.distinctFiles, size: formatSize(data.distinctOnDisk, loc) })}
        </strong>
      {/if}
    </div>

    <div class="tabs" role="tablist">
      {#each LISTS as l (l.id)}
        {@const r = data?.[l.id]}
        <button role="tab" class="tab card" class:on={list === l.id} aria-selected={list === l.id} onclick={() => (list = l.id)}>
          <span class="ic"><Icon name={l.icon} size={20} /></span>
          <span class="txt">
            <strong>{t(`ol-${l.id}-title`)}</strong>
            <span class="muted small">{t(`ol-${l.id}-hint`, { size: fmtGb(app.settings.largeGb), age: fmtYears(app.settings.staleYears), days: app.settings.installerDays })}</span>
          </span>
          <span class="num">
            <b>{r ? formatSize(r.matchedOnDisk, loc) : "…"}</b>
            <span class="muted small">{r ? t("ol-files", { count: r.matched }) : ""}</span>
          </span>
        </button>
      {/each}
    </div>

    <div class="listcard card">
      <div class="listbar">
        {#if selected.length}
          <span>{t("ol-selected", { count: selected.length, size: formatSize(selectedSize, loc) })}</span>
          <button class="btn ghost" onclick={() => (selected = [])}>{t("filter-clear")}</button>
        {:else}
          <span class="muted small">{list === "stale" ? t("ol-stale-note") : t("action-select-hint")}</span>
        {/if}
        <span class="grow"></span>
        <button class="btn ghost" disabled={!rows?.rows.length} onclick={() => (selected = rows?.rows.map((r) => r.id) ?? [])}>
          {t("ol-select-all")}
        </button>
        <button class="btn danger-outline" disabled={!selected.length} data-effect title={t("action-recycle-hint")} onclick={() => askDelete(selected)}>
          <Icon name="trash" size={16} />{selected.length ? t("action-recycle-n", { count: selected.length }) : t("action-recycle")}
        </button>
      </div>
      <div class="body">
        {#if rows && rows.rows.length}
          <ItemTable
            rows={rows.rows}
            {selected}
            onselect={toggle}
            oncontext={context}
            ondelete={askDelete}
            onreveal={(id) => api.reveal(id).catch(fail)}
          />
          {#if rows.matched > rows.rows.length}
            <p class="muted small more">{t("ol-more", { shown: rows.rows.length, count: rows.matched })}</p>
          {/if}
        {:else if !loading}
          <p class="muted none"><Icon name="check" size={18} />{t("ol-none")}</p>
        {/if}
      </div>
    </div>
    {#if list === "large"}
      <CliHint command={nfdScan(app.summary.root, "--min-size", sizeFlag(app.settings.largeGb * 2 ** 30), "--top", "200")} />
    {/if}
  </section>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .empty {
    height: 100%;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 12px;
    color: var(--text-muted);
  }
  .page {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 24px;
    min-height: 0;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 1.45em;
  }
  header p {
    margin: 0;
    white-space: normal;
    max-width: 75ch;
  }
  .check {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    white-space: nowrap;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 18px;
    padding: 10px 14px;
  }
  .controls label {
    display: inline-flex;
    gap: 8px;
    align-items: center;
  }
  .controls label span {
    color: var(--text-muted);
  }
  .grow {
    flex: 1;
  }
  .total {
    color: var(--accent);
  }
  .tabs {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    text-align: left;
    min-width: 0;
  }
  .tab.on {
    border-color: var(--accent);
    box-shadow: inset 0 -3px 0 var(--accent);
  }
  .tab:hover:not(.on) {
    background: var(--hover);
  }
  .ic {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    flex: none;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .txt {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .tab .num {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }
  .small {
    font-size: 0.84em;
  }
  .listcard {
    flex: 1;
    min-height: 160px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .listbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .body {
    flex: 1;
    overflow: auto;
    min-height: 0;
  }
  .more {
    margin: 8px 10px;
  }
  .none {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 18px;
  }
  .btn.danger-outline {
    color: var(--danger);
    border-color: var(--danger);
    background: transparent;
  }
  .btn.danger-outline:hover:not(:disabled) {
    background: var(--danger-soft);
  }
</style>
