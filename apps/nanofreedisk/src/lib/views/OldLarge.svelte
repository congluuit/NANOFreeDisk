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
  import SelectionBar from "../components/SelectionBar.svelte";
  import type { SelItem } from "../selection";
  import type { Located, OldLargeRows, SearchRows } from "../types";

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
  let page = $state<HTMLElement | null>(null);
  const asItem = (r: Located): SelItem => ({ id: String(r.id), bytes: r.onDisk, files: r.kind === "dir" ? r.files : 1 });
  /** The shown list (Select all, Ctrl+A) and every list (totals: a file can be in two). */
  const listItems = $derived((rows?.rows ?? []).map(asItem));
  const allItems = $derived.by(() => {
    const seen = new Map<string, SelItem>();
    for (const r of data ? [...data.large.rows, ...data.stale.rows, ...data.installers.rows] : []) seen.set(String(r.id), asItem(r));
    return [...seen.values()];
  });
  const picked = $derived(selected.map(String));
  const setPicked = (ids: string[]) => (selected = ids.map(Number));

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
  <section class="page oldlarge" bind:this={page}>
    <header class="page-head">
      <div>
        <h1>{t("ol-title")}</h1>
        <p>{t("ol-intro")}</p>
      </div>
      <div class="page-actions">
        {#if app.current !== rootId}
          <label class="check">
            <input type="checkbox" bind:checked={onlyFolder} />
            {t("ol-only-folder")}
          </label>
        {/if}
        {#if data}
          <span class="tag accent total">{t("ol-summary", { files: data.distinctFiles, size: formatSize(data.distinctOnDisk, loc) })}</span>
        {/if}
      </div>
    </header>

    <div class="tabs" role="tablist">
      {#each LISTS as l (l.id)}
        {@const r = data?.[l.id]}
        <button role="tab" class="tab card" class:on={list === l.id} aria-selected={list === l.id} onclick={() => (list = l.id)} title={t(`ol-${l.id}-hint`, { size: fmtGb(app.settings.largeGb), age: fmtYears(app.settings.staleYears), days: app.settings.installerDays })}>
          <span class="label"><Icon name={l.icon} size={15} />{t(`ol-${l.id}-title`)}</span>
          <span class="value"><b>{r ? formatSize(r.matchedOnDisk, loc) : "—"}</b><span class="muted">{r ? t("ol-files", { count: r.matched }) : ""}</span></span>
        </button>
      {/each}
    </div>

    <div class="listcard panel">
      <header class="panel-head">
        <div>
          <h2>{t(`ol-${list}-title`)}</h2>
          <p>{t(`ol-${list}-hint`, { size: fmtGb(app.settings.largeGb), age: fmtYears(app.settings.staleYears), days: app.settings.installerDays })} · {list === "stale" ? t("ol-stale-note") : t("action-select-hint")}</p>
        </div>
      </header>
    <div class="controls">
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
    <SelectionBar
      items={listItems}
      all={allItems}
      {picked}
      onchange={setPicked}
      expert={expert()}
      scope={page}
      primary
      noLevels
      sticky={false}
      action={{ label: (count, size) => t("sel-recycle-n", { count, size }), icon: "trash", hint: t("action-recycle-hint"), testid: "old-large", run: () => askDelete(selected) }}
    />
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
  .oldlarge {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-bottom: 18px;
  }
  .oldlarge > .page-head {
    margin-bottom: 0;
  }
  .total {
    font-weight: 600;
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
    gap: 8px 18px;
    padding: 0 0 12px;
    font-size: 0.92em;
  }
  .controls label {
    display: inline-flex;
    gap: 8px;
    align-items: center;
  }
  .controls label span {
    color: var(--text-muted);
  }
  .tabs {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }
  .tab {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 13px 16px 14px;
    text-align: left;
    min-width: 0;
    font: inherit;
    color: var(--text);
    transition: border-color 0.12s;
  }
  .tab.on {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .tab:hover:not(.on) {
    border-color: var(--text-muted);
  }
  .label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.86em;
    color: var(--text-muted);
  }
  .tab.on .label {
    color: var(--accent-ink);
  }
  .value {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
  }
  .value b {
    font-size: 1.85em;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.15;
    font-variant-numeric: tabular-nums;
  }
  .value .muted {
    font-size: 0.86em;
  }
  .listcard {
    flex: 1 0 260px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding-bottom: 0;
  }
  .listcard .panel-head {
    margin-bottom: 8px;
  }
  .body {
    flex: 1;
    overflow: auto;
    min-height: 0;
    margin: 0 -18px;
    border-top: 1px solid var(--border);
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
</style>
