<script lang="ts">
  // Analyze: the figures that matter first (free space, what can go safely, what needs
  // a look, what this scan did not cover), then the Explorer (map, folders and the
  // other lists of the scan) next to the Recommendations the user ticks to clean.
  import { api, inTauri, pickSaveFile } from "../api";
  import { exportHtmlReport, itemMenu } from "../actions";
  import { nfdAdvise } from "../cli";
  import { CATEGORIES, categoryOf, tokenOf } from "../colors";
  import { VERDICTS, VERDICT_TOKEN, sections } from "../advice";
  import { advice, listenAdvice, loadAdvice } from "../advice/state.svelte";
  import AdviceDialog from "../components/AdviceDialog.svelte";
  import OverrideDialog from "../components/OverrideDialog.svelte";
  import Suggestions from "../components/Suggestions.svelte";
  import StatCard from "../components/StatCard.svelte";
  import { formatCount, formatDuration, formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, askDelete, expert, fail, navigate, select, startScan, toast, type ExplorerLens } from "../state.svelte";
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

  type Lens = ExplorerLens | "top" | "ext" | "age" | "changes" | "search" | "issues";
  let lens = $state<Lens>(app.settings.explorer === "folders" ? "folders" : "map");
  let detailsOpen = $state(false);
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

  function setLens(l: Lens) {
    lens = l;
    if (l === "map" || l === "folders") app.settings.explorer = l;
  }

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
    if (lens === "issues" && app.summary) api.issues(1000).then((r) => (issues = r)).catch(fail);
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
  const verdictColors = $derived(app.settings.mapColor === "verdict");

  // the stat row: what the recommendations can free, what needs a look, the drive
  const sugg = $derived(advice.summary?.suggestions ?? null);
  const parts = $derived(sections(sugg?.groups ?? [], advice.summary?.elsewhere ?? []));
  const sumOf = (gs: { bytes: number }[]) => gs.reduce((n, g) => n + g.bytes, 0);
  const outside = $derived(s && s.volumeTotal ? Math.max(0, s.volumeTotal - s.volumeFree - s.stats.on_disk) : null);
  const rootName = $derived(current?.ancestors[current.ancestors.length - 1]?.name ?? s?.root ?? "");

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
  const isMap = $derived(lens === "map");
</script>

{#if !s}
  <div class="empty">
    <Icon name="analyze" size={48} />
    <p class="muted">{t("analyze-empty")}</p>
    <button class="btn primary" onclick={() => (app.view = "home")}>{t("nav-home")}</button>
  </div>
{:else}
  <section class="analyze" bind:this={section}>
    <header class="page-head">
      <div class="title">
        <h1 class="ellipsis">{t("analyze-title", { name: s.root })}</h1>
        <p class="sumline">
          <span>
            {t("analyze-summary", {
              size: formatSize(s.stats.on_disk, loc),
              files: formatCount(s.stats.files, loc),
              dirs: formatCount(s.stats.dirs, loc),
              time: formatDuration(s.elapsedMs, loc),
            })}
          </span>
          {#if s.issues}
            <button class="link" onclick={() => setLens("issues")} disabled={!expert()} title={expert() ? undefined : t("mode-expert-hint")}>
              <Icon name="alert" size={13} />{t("analyze-issues", { count: s.issues })}
            </button>
          {/if}
        </p>
      </div>
      <div class="page-actions">
        <div class="find"><FilterBar onresults={() => (lens = "search")} /></div>
        <button class="btn" onclick={() => startScan(s.root)} data-effect title={t("hint-rescan")}>
          <Icon name="refresh" size={16} />{t("analyze-rescan")}
        </button>
        <button class="btn" onclick={exportMenu} data-effect title={t("hint-export")}>
          <Icon name="export" size={16} />{t("analyze-export")}
        </button>
      </div>
    </header>

    <div class="stats">
      <StatCard
        label={t("stat-free")}
        value={s.volumeTotal ? formatSize(s.volumeFree, loc) : null}
        unit={s.volumeTotal ? t("stat-of", { total: formatSize(s.volumeTotal, loc) }) : ""}
        hint={s.volumeTotal ? t("analyze-of-drive", { percent: formatPercent(s.stats.on_disk / s.volumeTotal, loc) }) : undefined}
        data-stat="free"
      />
      <StatCard
        label={t("stat-safe")}
        tone="safe"
        value={sugg ? formatSize(sumOf(parts.safe), loc) : null}
        unit={sugg ? t("stat-items", { count: parts.safe.length }) : t("stat-checking")}
        hint={t("advice-section-safe-hint")}
        data-stat="safe"
      />
      <StatCard
        label={t("stat-review")}
        tone="caution"
        value={sugg ? formatSize(sumOf(parts.caution), loc) : null}
        unit={sugg ? t("stat-items", { count: parts.caution.length }) : t("stat-checking")}
        hint={t("advice-section-caution-hint")}
        data-stat="review"
      />
      <StatCard
        label={t("stat-outside")}
        value={outside != null ? formatSize(outside, loc) : null}
        unit={t("stat-outside-unit")}
        hint={t("stat-outside-hint")}
        data-stat="outside"
      />
    </div>

    <div class="main">
      <section class="panel explorer">
        <header class="panel-head">
          <div class="ellipsis">
            <h2>{t("explorer-title")}</h2>
            <p class="ellipsis">{t("explorer-sub", { name: rootName })}</p>
          </div>
          {#if isMap}
            <div class="tools">
              <div class="segmented" role="group" aria-label={t("advice-color-type")}>
                <button class:on={!verdictColors} onclick={() => (app.settings.mapColor = "type")} data-map-color="type" title={t("advice-color-type")}>{t("explorer-color-type")}</button>
                <button class:on={verdictColors} onclick={() => (app.settings.mapColor = "verdict")} data-map-color="verdict" title={t("advice-color-verdict")}>{t("explorer-color-cleanup")}</button>
              </div>
              <div class="segmented" role="group" aria-label={t("explorer-lens-map")}>
                <button class:on={app.settings.viz === "treemap"} data-panel="treemap" onclick={() => (app.settings.viz = "treemap")} title={t("analyze-view-treemap")} aria-label={t("analyze-view-treemap")}><Icon name="treemap" size={15} /></button>
                <button class:on={app.settings.viz === "sunburst"} data-panel="sunburst" onclick={() => (app.settings.viz = "sunburst")} title={t("analyze-view-sunburst")} aria-label={t("analyze-view-sunburst")}><Icon name="sunburst" size={15} /></button>
              </div>
            </div>
          {/if}
        </header>

        <div class="tabrow" role="tablist">
          <button role="tab" data-lens="map" aria-selected={lens === "map"} onclick={() => setLens("map")}>{t("explorer-lens-map")}</button>
          <button role="tab" data-lens="folders" aria-selected={lens === "folders"} onclick={() => setLens("folders")}>{t("explorer-lens-folders")}</button>
          <button role="tab" data-lens="top" aria-selected={lens === "top"} onclick={() => setLens("top")}>{t("analyze-tab-top")}</button>
          <button role="tab" data-lens="ext" aria-selected={lens === "ext"} onclick={() => setLens("ext")}>{t("analyze-tab-ext")}</button>
          <button role="tab" data-lens="age" data-tab="age" aria-selected={lens === "age"} onclick={() => setLens("age")}>{t("analyze-tab-age")}</button>
          <button role="tab" data-lens="changes" data-tab="changes" aria-selected={lens === "changes"} onclick={() => setLens("changes")}>{t("analyze-tab-changes")}</button>
          {#if app.search}
            <button role="tab" data-lens="search" aria-selected={lens === "search"} onclick={() => setLens("search")}>
              {t("analyze-tab-search")}<span class="count">{formatCount(app.search.result.matched, loc)}</span>
            </button>
          {/if}
          {#if s.issues && expert()}
            <button role="tab" data-lens="issues" aria-selected={lens === "issues"} onclick={() => setLens("issues")}>{t("analyze-tab-issues")}</button>
          {/if}
        </div>

        {#if lens === "map" || lens === "folders"}
          <div class="crumbrow">
            <button class="btn ghost icon" onclick={up} disabled={(current?.ancestors.length ?? 0) < 2} title={`${t("analyze-up")} (Backspace)`} aria-label={t("analyze-up")}>
              <Icon name="up" size={16} />
            </button>
            <nav class="crumbs" aria-label="breadcrumb">
              {#each current?.ancestors ?? [] as c, i (c.id)}
                {#if i > 0}<Icon name="chevron" size={12} />{/if}
                <button class="crumb" class:last={i === (current?.ancestors.length ?? 0) - 1} onclick={() => navigate(c.id)}>
                  {c.name}
                </button>
              {/each}
            </nav>
            <span class="mapped">{t("explorer-mapped", { size: formatSize(total, loc) })}</span>
          </div>
        {/if}

        <div class="body" class:map={isMap}>
          {#if lens === "map"}
            {#if app.settings.viz === "sunburst"}
              <Sunburst rootId={app.current} {total} version={app.version} selected={app.selected} onselect={select} onopen={open} oncontext={contextFor} />
            {:else}
              <Treemap rootId={app.current} {total} version={app.version} selected={app.selected} onselect={select} onopen={open} oncontext={contextFor} colorBy={app.settings.mapColor} />
            {/if}
          {:else if lens === "folders"}
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
          {:else if lens === "top"}
            <ItemTable rows={top} selected={app.selected} onselect={select} oncontext={contextFor} ondelete={askDelete} onreveal={(id) => api.reveal(id).catch(fail)} />
          {:else if lens === "ext"}
            <table class="ext">
              <thead>
                <tr><th>{t("col-type")}</th><th class="num">{t("col-files")}</th><th class="num">{t("col-size")}</th><th></th></tr>
              </thead>
              <tbody>
                {#each exts as x (x.ext)}
                  <tr>
                    <td>
                      <i class="sw" style="--c: var({tokenOf(categoryOf('a.' + x.ext))})"></i>
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
          {:else if lens === "age"}
            <AgePanel />
          {:else if lens === "changes"}
            <ChangesPanel />
          {:else if lens === "search" && app.search}
            <p class="muted small res">
              {t("filter-results", { count: formatCount(app.search.result.matched, loc), size: formatSize(app.search.result.matchedOnDisk, loc) })}
            </p>
            <ItemTable rows={app.search.result.rows} selected={app.selected} onselect={select} oncontext={contextFor} ondelete={askDelete} onreveal={(id) => api.reveal(id).catch(fail)} />
          {:else if lens === "issues"}
            <ul class="issues">
              {#each issues as i (i.path)}
                <li><span class="mono ellipsis" title={i.path}>{i.path}</span><span class="muted small">{i.message}</span></li>
              {/each}
            </ul>
          {/if}
        </div>

        {#if isMap}
          <div class="legend">
            {#if advice.summary?.suggestions}
              <span><i class="sw hatch"></i>{t("legend-safe")}</span>
            {/if}
            {#if verdictColors && app.settings.viz === "treemap"}
              {#each VERDICTS.slice(1) as v (v)}
                <span><i class="sw" style="--c: var({VERDICT_TOKEN[v]})"></i>{t(`advice-verdict-${v}`)}</span>
              {/each}
            {:else}
              {#each CATEGORIES as c (c)}
                <span><i class="sw" style="--c: var({tokenOf(c)})"></i>{t(`cat-${c}`)}</span>
              {/each}
              <span><i class="sw" style="--c: var(--tm-other)"></i>{t("cat-other")}</span>
            {/if}
          </div>
        {/if}

        <div class="details" class:open={detailsOpen}>
          <button class="details-toggle" onclick={() => (detailsOpen = !detailsOpen)} aria-expanded={detailsOpen} data-details-toggle>
            <Icon name="chevron" size={13} />
            <span>{detailsOpen ? t("details-hide") : t("details-show")}</span>
          </button>
          {#if detailsOpen}
            <div class="details-body"><Details /></div>
          {/if}
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
          compact
          idle={t("explorer-idle")}
          sticky={false}
          action={{ label: (count, size) => t("sel-recycle-n", { count, size }), icon: "trash", hint: t("action-recycle-hint"), testid: "analyze", run: () => askDelete(app.selected) }}
        />
      </section>

      <div class="recs">
        <Suggestions />
        <CliHint command={nfdAdvise(s.root)} />
      </div>
    </div>
  </section>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if advice.confirm}
  <AdviceDialog req={advice.confirm} />
{/if}

{#if advice.override}
  <OverrideDialog req={advice.override} />
{/if}

<svelte:window
  onkeydown={(e) => {
    const typing = (e.target as HTMLElement)?.closest("input, select, textarea");
    if (typing || app.deleting || advice.confirm || advice.override || !app.summary) return;
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
    padding: 18px 22px 18px;
    min-height: 0;
    overflow: auto;
  }
  .page-head {
    margin-bottom: 14px;
    align-items: center;
  }
  .title {
    min-width: 0;
  }
  .sumline {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 12px;
    align-items: center;
    font-size: 0.9em;
  }
  .page-actions {
    flex: 0 1 auto;
    flex-wrap: nowrap;
    min-width: 0;
  }
  .find {
    flex: 0 1 340px;
    min-width: 180px;
  }
  .stats {
    margin-bottom: 14px;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: none;
    background: none;
    color: var(--risk-caution-ink);
    padding: 0;
    font-size: inherit;
  }
  .link:disabled {
    cursor: default;
  }
  .main {
    flex: 1;
    min-height: 460px;
    display: grid;
    grid-template-columns: minmax(0, 1.55fr) minmax(330px, 1fr);
    gap: 14px;
  }
  .explorer {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding-bottom: 14px;
  }
  .explorer .panel-head {
    margin-bottom: 8px;
    align-items: center;
  }
  .tools {
    display: flex;
    gap: 8px;
    flex: none;
  }
  .tabrow {
    margin: 0 -18px;
    padding: 0 12px;
  }
  .crumbrow {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 0 6px;
    min-width: 0;
  }
  .crumbs {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 1px;
    min-width: 0;
    overflow: hidden;
    color: var(--text-muted);
  }
  .crumb {
    border: none;
    background: none;
    padding: 2px 5px;
    border-radius: 4px;
    white-space: nowrap;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-muted);
    font-size: 0.9em;
  }
  .crumb:hover {
    background: var(--hover);
    color: var(--text);
  }
  .crumb.last {
    color: var(--text);
    font-weight: 600;
  }
  .mapped {
    font-size: 0.84em;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .body {
    flex: 1;
    min-height: 200px;
    overflow: auto;
    margin: 0 -18px;
    padding: 0 18px;
    border-top: 1px solid transparent;
  }
  .body.map {
    overflow: hidden;
    margin: 0;
    padding: 0;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    padding-top: 10px;
    font-size: 0.8em;
    color: var(--text-muted);
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .sw {
    display: inline-block;
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--c) 22%, var(--surface));
    border: 1px solid color-mix(in srgb, var(--c) 45%, var(--surface));
  }
  .sw.hatch {
    --c: var(--risk-safe);
    background:
      repeating-linear-gradient(-45deg, color-mix(in srgb, var(--c) 55%, var(--surface)) 0 1.5px, transparent 1.5px 4px),
      color-mix(in srgb, var(--c) 22%, var(--surface));
  }
  .details {
    margin-top: 10px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .details.open {
    flex: 0 1 46%;
    min-height: 160px;
  }
  .details-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
    border: none;
    background: none;
    padding: 8px 0 2px;
    color: var(--text-muted);
    font-size: 0.86em;
  }
  .details-toggle:hover {
    color: var(--text);
  }
  .details-toggle :global(svg) {
    transition: transform 0.15s ease;
  }
  .details.open .details-toggle :global(svg) {
    transform: rotate(90deg);
  }
  .details-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0 -18px;
  }
  .details-body :global(.details) {
    border-top: none;
    max-height: none;
  }
  .recs {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    min-width: 0;
  }
  .recs > :global(.sugg) {
    flex: 1;
  }
  .ext {
    width: 100%;
    border-collapse: collapse;
  }
  .ext th {
    position: sticky;
    top: 0;
    background: var(--surface);
    font-size: 0.8em;
    color: var(--text-muted);
    text-align: left;
    font-weight: 600;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .ext td {
    padding: 5px 10px;
    border-bottom: 1px solid var(--border);
  }
  .ext td:first-child {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .barcell {
    width: 30%;
  }
  .res {
    margin: 8px 0;
  }
  .issues {
    list-style: none;
    margin: 0;
    padding: 6px 0;
  }
  .issues li {
    display: flex;
    flex-direction: column;
    padding: 5px 0;
    border-bottom: 1px solid var(--border);
  }
  @media (max-width: 1180px) {
    .main {
      grid-template-columns: minmax(0, 1fr);
    }
    .explorer {
      height: 600px;
    }
    .recs {
      height: 620px;
    }
    .find {
      flex-basis: 260px;
    }
  }
</style>
