<script lang="ts">
  import { api, inTauri, pickSaveFile } from "../api";
  import { exportHtmlReport, itemMenu } from "../actions";
  import { nfdAdvise, nfdScan } from "../cli";
  import { CATEGORIES, categoryOf, tokenOf } from "../colors";
  import { VERDICTS, VERDICT_TOKEN } from "../advice";
  import { advice, listenAdvice, loadAdvice } from "../advice/state.svelte";
  import AdviceDialog from "../components/AdviceDialog.svelte";
  import Suggestions from "../components/Suggestions.svelte";
  import { formatCount, formatDuration, formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, askDelete, expert, fail, navigate, select, startScan, toast } from "../state.svelte";
  import AgePanel from "../components/AgePanel.svelte";
  import ChangesPanel from "../components/ChangesPanel.svelte";
  import CliHint from "../components/CliHint.svelte";
  import ContextMenu, { type MenuItem } from "../components/ContextMenu.svelte";
  import Details from "../components/Details.svelte";
  import FilterBar from "../components/FilterBar.svelte";
  import Icon from "../components/Icon.svelte";
  import ItemTable from "../components/ItemTable.svelte";
  import SelectionBar from "../components/SelectionBar.svelte";
  import type { SelItem } from "../selection";
  import Sunburst from "../components/Sunburst.svelte";
  import Treemap from "../components/Treemap.svelte";
  import TreeList from "../components/TreeList.svelte";
  import type { Detail, ExtRow, Located, Node, NodeAdvice, ScanIssue } from "../types";

  type Tab = "top" | "ext" | "age" | "changes" | "search" | "issues";
  let tab = $state<Tab>("top");
  let current = $state<Detail | null>(null);
  let top = $state<Located[]>([]);
  let exts = $state<ExtRow[]>([]);
  let issues = $state<ScanIssue[]>([]);
  let menu: { x: number; y: number; items: MenuItem[] } | null = $state(null);
  const loc = $derived(intlLocale());
  const s = $derived(app.summary);
  const total = $derived(current?.onDisk ?? 0);
  let section = $state<HTMLElement | null>(null);
  let children = $state<Node[]>([]);
  /** Sizes of selected entries not in a list shown here (picked in the treemap). */
  let known = $state<Record<number, { onDisk: number; files: number }>>({});

  $effect(() => {
    const id = app.current;
    void app.version;
    if (!app.summary) return;
    api.detail(id).then((d) => (current = d)).catch(() => navigate(app.summary?.rootId ?? 0));
    api.topFiles(id, 200).then((r) => (top = r)).catch(fail);
    api.extensions(id, 60).then((r) => (exts = r)).catch(fail);
    api.children(id).then((r) => (children = r)).catch(() => (children = []));
  });

  // what the selection bar counts: the selected entries, whatever list they came from
  const listed = $derived.by(() => {
    const m = new Map<number, { onDisk: number; files: number; kind: string; advice?: NodeAdvice }>();
    for (const n of [...children, ...top, ...(app.search?.result.rows ?? [])]) m.set(n.id, n);
    return m;
  });
  $effect(() => {
    for (const id of app.selected) {
      if (listed.has(id) || id in known) continue;
      api.detail(id).then((d) => (known = { ...known, [id]: { onDisk: d.onDisk, files: d.files } })).catch(() => {});
    }
  });
  const rootId = $derived(app.summary?.rootId ?? 0);
  const sizeItem = (id: number): SelItem => {
    const n = listed.get(id);
    const k = n ?? known[id];
    // what must stay (or only a system tool may shrink) is never ticked for the Recycle Bin
    const kept = n?.advice?.verdict === "keep" || n?.advice?.verdict === "guide";
    return { id: String(id), bytes: k?.onDisk ?? 0, files: n && n.kind !== "dir" ? 1 : (k?.files ?? 0), locked: id === rootId || kept };
  };
  /** Select all = the entries of the folder shown. */
  const childItems = $derived(children.map((n) => sizeItem(n.id)));
  const selItems = $derived.by(() => {
    const seen = new Map(childItems.map((i) => [i.id, i]));
    for (const id of app.selected) if (!seen.has(String(id))) seen.set(String(id), sizeItem(id));
    return [...seen.values()];
  });
  const picked = $derived(app.selected.map(String));
  const setPicked = (ids: string[]) => (app.selected = ids.map(Number));

  $effect(() => {
    if (tab === "issues" && app.summary) api.issues(1000).then((r) => (issues = r)).catch(fail);
  });

  // the advice of this scan (computed in the background; `advice-ready` reloads it)
  $effect(() => {
    void listenAdvice();
  });
  $effect(() => {
    void app.summary;
    void app.version;
    void loadAdvice();
  });
  const panel = $derived(app.settings.analyzePanel);
  const verdictColors = $derived(app.settings.mapColor === "verdict");

  function up() {
    const anc = current?.ancestors;
    if (anc && anc.length > 1) navigate(anc[anc.length - 2].id);
  }

  function open(id: number) {
    navigate(id);
  }

  function contextFor(id: number, x: number, y: number) {
    if (!app.selected.includes(id)) select(id);
    menu = { x, y, items: itemMenu(id, { selection: app.selected }) };
  }

  async function exportAs(format: "json" | "csv") {
    if (!inTauri) return;
    const path = await pickSaveFile(t("analyze-export"), `nanofreedisk-scan.${format}`, format);
    if (!path) return;
    try {
      await api.exportReport(path, format);
      toast(t("analyze-exported", { path }), "ok");
    } catch (e) {
      fail(e);
    }
  }

  function exportMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: r.left,
      y: r.bottom + 4,
      items: [
        { icon: "export", label: "analyze-export-html", hint: t("hint-export"), run: () => void exportHtmlReport() },
        ...(expert()
          ? [
              { icon: "export", label: "analyze-export-json", hint: t("hint-export"), disabled: !inTauri, run: () => exportAs("json") },
              { icon: "export", label: "analyze-export-csv", hint: t("hint-export"), disabled: !inTauri, run: () => exportAs("csv") },
            ]
          : []),
      ],
    };
  }

  const extMax = $derived(exts[0]?.onDisk || 1);
</script>

{#if !s}
  <div class="empty">
    <Icon name="analyze" size={48} />
    <p class="muted">{t("analyze-empty")}</p>
    <button class="btn primary" onclick={() => (app.view = "home")}>{t("nav-home")}</button>
  </div>
{:else}
  <section class="analyze" bind:this={section}>
    <div class="toolbar">
      <button class="btn ghost icon" onclick={up} disabled={(current?.ancestors.length ?? 0) < 2} title={`${t("analyze-up")} (Backspace)`}>
        <Icon name="up" />
      </button>
      <nav class="crumbs" aria-label="breadcrumb">
        {#each current?.ancestors ?? [] as c, i (c.id)}
          {#if i > 0}<Icon name="chevron" size={14} />{/if}
          <button class="crumb" class:last={i === (current?.ancestors.length ?? 0) - 1} onclick={() => navigate(c.id)}>
            {c.name}
          </button>
        {/each}
      </nav>
      <button class="btn ghost" onclick={() => startScan(s.root)} data-effect title={t("hint-rescan")}>
        <Icon name="refresh" size={16} />{t("analyze-rescan")}
      </button>
      <button class="btn ghost" onclick={exportMenu} data-effect title={t("hint-export")}>
        <Icon name="export" size={16} />{t("analyze-export")}
      </button>
    </div>

    <div class="summary">
      <span>
        {t("analyze-summary", {
          size: formatSize(s.stats.on_disk, loc),
          files: formatCount(s.stats.files, loc),
          dirs: formatCount(s.stats.dirs, loc),
          time: formatDuration(s.elapsedMs, loc),
        })}
      </span>
      {#if s.volumeTotal}
        <span class="pill">{t("analyze-of-drive", { percent: formatPercent(s.stats.on_disk / s.volumeTotal, loc) })}</span>
      {/if}
      {#if s.issues}
        <button class="link" onclick={() => (tab = "issues")} disabled={!expert()} title={expert() ? undefined : t("mode-expert-hint")}>
          <Icon name="alert" size={14} />{t("analyze-issues", { count: s.issues })}
        </button>
      {/if}
      <span class="grow"></span>
      <CliHint command={panel === "suggest" ? nfdAdvise(s.root) : nfdScan(s.root, "--top", "50")} />
    </div>

    <FilterBar onresults={() => (tab = "search")} />

    <div class="main">
      <div class="col left card">
        <div class="list">
          <TreeList
            rootId={app.current}
            {total}
            version={app.version}
            selected={app.selected}
            onselect={select}
            onopen={open}
            oncontext={contextFor}
            ondelete={askDelete}
          />
        </div>
        <Details />
      </div>
      <div class="col right">
        <div class="panelbar" role="tablist" aria-label={t("advice-tab")}>
          <button role="tab" data-panel="suggest" aria-selected={panel === "suggest"} class:on={panel === "suggest"} onclick={() => (app.settings.analyzePanel = "suggest")}>
            <Icon name="clean" size={15} />{t("advice-tab")}
            {#if advice.summary?.suggestions?.reclaim}<span class="count">{formatSize(advice.summary.suggestions.reclaim, loc)}</span>{/if}
          </button>
          <button role="tab" data-panel="treemap" aria-selected={panel === "map" && app.settings.viz === "treemap"} class:on={panel === "map" && app.settings.viz === "treemap"} onclick={() => { app.settings.analyzePanel = "map"; app.settings.viz = "treemap"; }}>
            <Icon name="treemap" size={15} />{t("analyze-view-treemap")}
          </button>
          <button role="tab" data-panel="sunburst" aria-selected={panel === "map" && app.settings.viz === "sunburst"} class:on={panel === "map" && app.settings.viz === "sunburst"} onclick={() => { app.settings.analyzePanel = "map"; app.settings.viz = "sunburst"; }}>
            <Icon name="sunburst" size={15} />{t("analyze-view-sunburst")}
          </button>
          {#if panel === "map"}
            <span class="grow"></span>
            <div class="seg" role="group">
              <button class:on={!verdictColors} onclick={() => (app.settings.mapColor = "type")} data-map-color="type">{t("advice-color-type")}</button>
              <button class:on={verdictColors} onclick={() => (app.settings.mapColor = "verdict")} data-map-color="verdict">{t("advice-color-verdict")}</button>
            </div>
          {/if}
        </div>
        <div class="viz card" class:suggest={panel === "suggest"}>
          {#if panel === "suggest"}
            <Suggestions />
          {:else if app.settings.viz === "treemap"}
            <Treemap rootId={app.current} {total} version={app.version} selected={app.selected} onselect={select} onopen={open} oncontext={contextFor} colorBy={app.settings.mapColor} />
          {:else}
            <Sunburst rootId={app.current} {total} version={app.version} selected={app.selected} onselect={select} onopen={open} oncontext={contextFor} />
          {/if}
        </div>
        {#if panel === "map"}
          <div class="legend">
            {#if verdictColors && app.settings.viz === "treemap"}
              {#each VERDICTS as v (v)}
                <span><i style="background: var({VERDICT_TOKEN[v]})"></i>{t(`advice-verdict-${v}`)}</span>
              {/each}
            {:else}
              <span><i style="background: var(--tm-dir); border: 1px solid var(--border)"></i>{t("analyze-legend-folder")}</span>
              {#each CATEGORIES as c (c)}
                <span><i style="background: var({tokenOf(c)})"></i>{t(`cat-${c}`)}</span>
              {/each}
              <span><i style="background: var(--tm-other)"></i>{t("cat-other")}</span>
            {/if}
          </div>
        {/if}
        <div class="tabs card" class:low={panel === "suggest"}>
          <div class="tabbar" role="tablist">
            <button role="tab" aria-selected={tab === "top"} class:on={tab === "top"} onclick={() => (tab = "top")}>{t("analyze-tab-top")}</button>
            <button role="tab" aria-selected={tab === "ext"} class:on={tab === "ext"} onclick={() => (tab = "ext")}>{t("analyze-tab-ext")}</button>
            <button role="tab" data-tab="age" aria-selected={tab === "age"} class:on={tab === "age"} onclick={() => (tab = "age")}>{t("analyze-tab-age")}</button>
            <button role="tab" data-tab="changes" aria-selected={tab === "changes"} class:on={tab === "changes"} onclick={() => (tab = "changes")}>{t("analyze-tab-changes")}</button>
            <button role="tab" aria-selected={tab === "search"} class:on={tab === "search"} onclick={() => (tab = "search")} disabled={!app.search}>
              {t("analyze-tab-search")}{#if app.search}<span class="count">{formatCount(app.search.result.matched, loc)}</span>{/if}
            </button>
            {#if s.issues && expert()}
              <button role="tab" aria-selected={tab === "issues"} class:on={tab === "issues"} onclick={() => (tab = "issues")}>{t("analyze-tab-issues")}</button>
            {/if}
          </div>
          <div class="tabbody">
            {#if tab === "top"}
              <ItemTable rows={top} selected={app.selected} onselect={select} oncontext={contextFor} ondelete={askDelete} onreveal={(id) => api.reveal(id).catch(fail)} />
            {:else if tab === "ext"}
              <table class="ext">
                <thead>
                  <tr><th>{t("col-type")}</th><th class="num">{t("col-files")}</th><th class="num">{t("col-size")}</th><th></th></tr>
                </thead>
                <tbody>
                  {#each exts as x (x.ext)}
                    <tr>
                      <td>
                        <i class="sw" style="background: var({tokenOf(categoryOf('a.' + x.ext))})"></i>
                        <span class="mono">{x.ext ? "." + x.ext : t("ext-none")}</span>
                        <span class="muted small">{t(`cat-${categoryOf("a." + x.ext)}`)}</span>
                      </td>
                      <td class="num muted">{formatCount(x.files, loc)}</td>
                      <td class="num">{formatSize(x.onDisk, loc)}</td>
                      <td class="barcell"><span class="bar"><span style="width: {(x.onDisk / extMax) * 100}%"></span></span></td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {:else if tab === "age"}
              <AgePanel />
            {:else if tab === "changes"}
              <ChangesPanel />
            {:else if tab === "search" && app.search}
              <p class="muted small res">
                {t("filter-results", { count: formatCount(app.search.result.matched, loc), size: formatSize(app.search.result.matchedOnDisk, loc) })}
              </p>
              <ItemTable rows={app.search.result.rows} selected={app.selected} onselect={select} oncontext={contextFor} ondelete={askDelete} onreveal={(id) => api.reveal(id).catch(fail)} />
            {:else if tab === "issues"}
              <ul class="issues">
                {#each issues as i (i.path)}
                  <li><span class="mono ellipsis" title={i.path}>{i.path}</span><span class="muted small">{i.message}</span></li>
                {/each}
              </ul>
            {/if}
          </div>
        </div>
      </div>
    </div>
    <SelectionBar
      items={childItems}
      all={selItems}
      {picked}
      onchange={setPicked}
      expert={expert()}
      scope={section}
      primary
      noLevels
      sticky={false}
      action={{ label: (count, size) => t("sel-recycle-n", { count, size }), icon: "trash", hint: t("action-recycle-hint"), testid: "analyze", run: () => askDelete(app.selected) }}
    />
  </section>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if advice.confirm}
  <AdviceDialog req={advice.confirm} />
{/if}

<svelte:window
  onkeydown={(e) => {
    const typing = (e.target as HTMLElement)?.closest("input, select, textarea");
    if (typing || app.deleting || advice.confirm || !app.summary) return;
    if (e.key === "Backspace" || (e.altKey && e.key === "ArrowUp")) {
      e.preventDefault();
      up();
    }
  }}
/>

<style>
  .empty {
    height: 100%;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 12px;
    color: var(--text-muted);
  }
  .analyze {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .crumbs {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    overflow: hidden;
    color: var(--text-muted);
  }
  .crumb {
    border: none;
    background: none;
    padding: 4px 6px;
    border-radius: 4px;
    white-space: nowrap;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-muted);
  }
  .crumb:hover {
    background: var(--hover);
    color: var(--text);
  }
  .crumb.last {
    color: var(--text);
    font-weight: 600;
  }
  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .seg button {
    border: none;
    background: var(--surface);
    padding: 6px 10px;
    color: var(--text-muted);
    display: grid;
  }
  .seg button.on {
    background: var(--accent-soft);
    color: var(--text);
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    font-size: 0.92em;
  }
  .pill {
    padding: 1px 8px;
    border-radius: 10px;
    background: var(--accent-soft);
  }
  .grow {
    flex: 1;
  }
  .link:disabled {
    cursor: default;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: none;
    background: none;
    color: var(--risk-caution);
    padding: 0;
  }
  .main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(400px, 1fr) minmax(380px, 1.15fr);
    gap: 12px;
  }
  .col {
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .left {
    overflow: hidden;
  }
  .list {
    flex: 1;
    min-height: 0;
  }
  .right {
    gap: 8px;
  }
  .viz {
    flex: 1.5;
    min-height: 180px;
    padding: 4px;
    overflow: hidden;
  }
  .viz.suggest {
    flex: 2.2;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .panelbar {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-wrap: wrap;
  }
  .panelbar > button {
    border: none;
    background: none;
    padding: 6px 12px;
    color: var(--text-muted);
    border-bottom: 2px solid transparent;
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .panelbar > button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .seg button {
    font-size: 0.82em;
    padding: 4px 10px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    font-size: 0.78em;
    color: var(--text-muted);
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .legend i,
  .sw {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }
  .tabs.low {
    flex: 0.75;
  }
  .tabs {
    flex: 1;
    min-height: 150px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .tabbar {
    display: flex;
    gap: 2px;
    padding: 4px 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .tabbar button {
    border: none;
    background: none;
    padding: 7px 12px;
    color: var(--text-muted);
    border-bottom: 2px solid transparent;
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .tabbar button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .tabbar button:disabled {
    opacity: 0.5;
  }
  .count {
    font-size: 0.8em;
    padding: 0 6px;
    border-radius: 8px;
    background: var(--accent-soft);
  }
  .tabbody {
    flex: 1;
    overflow: auto;
    min-height: 0;
  }
  .ext {
    width: 100%;
    border-collapse: collapse;
  }
  .ext th {
    position: sticky;
    top: 0;
    background: var(--surface);
    font-size: 0.82em;
    color: var(--text-muted);
    text-align: left;
    font-weight: normal;
    text-transform: uppercase;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }
  .ext td {
    padding: 4px 10px;
  }
  .ext td:first-child {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .barcell {
    width: 30%;
  }
  .small {
    font-size: 0.86em;
  }
  .res {
    margin: 6px 10px;
  }
  .issues {
    list-style: none;
    margin: 0;
    padding: 6px 10px;
  }
  .issues li {
    display: flex;
    flex-direction: column;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
</style>
