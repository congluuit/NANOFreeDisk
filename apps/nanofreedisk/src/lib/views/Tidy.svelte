<script lang="ts">
  // Tidy up (PLAN §13 A3): empty folder trees (folders holding only what Windows or
  // macOS made on their own count as empty), empty files outside program folders and
  // shortcuts whose target is gone. Picked items go to the quarantine through the
  // SafetyGate (restorable from the Activity log for 7 days); broken junctions are
  // listed only, for Expert mode (links are removed in Explorer).
  import { api } from "../api";
  import { itemMenu } from "../actions";
  import { formatCount } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, askDelete, expert, fail, refreshVolumes, startScan } from "../state.svelte";
  import ContextMenu, { type MenuItem } from "../components/ContextMenu.svelte";
  import GroupCheck from "../components/GroupCheck.svelte";
  import Icon from "../components/Icon.svelte";
  import ItemTable from "../components/ItemTable.svelte";
  import SelectionBar from "../components/SelectionBar.svelte";
  import StatCard from "../components/StatCard.svelte";
  import type { SelItem } from "../selection";
  import type { EmptyRow, EmptyRows, Located, ShortcutRow, ShortcutRows, ZeroRows } from "../types";

  let empty = $state<EmptyRows | null>(null);
  let zero = $state<ZeroRows | null>(null);
  let links = $state<ShortcutRows | null>(null);
  let junctions = $state<ShortcutRows | null>(null);
  let checking = $state(false);
  let selected = $state<number[]>([]);
  let menu: { x: number; y: number; items: MenuItem[] } | null = $state(null);
  const loc = $derived(intlLocale());
  const rootId = $derived(app.summary?.rootId ?? 0);
  const system = $derived(app.volumes.find((v) => v.system && v.ready));
  let page = $state<HTMLElement | null>(null);
  // empty folders, empty files and broken shortcuts; broken junctions are listed only
  // (removed in Explorer), so Select all never picks them
  const selItems = $derived<SelItem[]>([
    ...(empty?.rows ?? []).map((r) => ({ id: String(r.id), bytes: r.onDisk, files: r.files, group: "empty" })),
    ...(zero?.rows ?? []).map((r) => ({ id: String(r.id), bytes: r.onDisk, files: 1, group: "zero" })),
    ...(links?.rows ?? []).map((r) => ({ id: String(r.id), bytes: r.onDisk, files: 1, group: "links" })),
  ]);
  const picked = $derived(selected.map(String));
  const setPicked = (ids: string[]) => (selected = ids.map(Number));

  $effect(() => {
    void app.version;
    const id = rootId;
    if (!app.summary) return;
    let cancelled = false;
    api.emptyFolders(id, 2000).then((r) => !cancelled && (empty = r)).catch(fail);
    api.zeroByteFiles(id, 2000).then((r) => !cancelled && (zero = r)).catch(fail);
    api.brokenLinks(id, 2000).then((r) => !cancelled && (junctions = r)).catch(fail);
    checking = true;
    api
      .brokenShortcuts(id, 2000)
      .then((r) => !cancelled && (links = r))
      .catch(fail)
      .finally(() => !cancelled && (checking = false));
    return () => {
      cancelled = true;
    };
  });

  // the empty state offers to scan the system drive: make sure the drives are known
  $effect(() => {
    if (!app.summary && !app.volumes.length) void refreshVolumes();
  });

  function toggle(id: number, additive: boolean) {
    if (additive) selected = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
    else selected = [id];
  }

  function context(id: number, x: number, y: number) {
    selected = [id];
    menu = { x, y, items: itemMenu(id, { recycle: false }) };
  }

  /** Only what the lists offer goes (a junction clicked in Expert mode never does). */
  function quarantine() {
    const ok = new Set(selItems.map((i) => i.id));
    askDelete(
      selected.filter((id) => ok.has(String(id))),
      "quarantine",
      "tidy",
    );
  }

  const drive = (root: string) => root.replace(/\\$/, "");
  const reveal = (id: number) => api.reveal(id).catch(fail);
  const contents = (r: Located) => {
    const { nested, clutter } = r as EmptyRow;
    const parts = [nested ? t("tidy-nested", { count: nested }) : t("tidy-empty-self")];
    if (clutter) parts.push(t("tidy-clutter", { count: clutter }));
    return parts.join(", ");
  };
  const target = (r: Located) => {
    const s = r as ShortcutRow;
    return `${t(`tidy-status-${s.status}`)} · ${s.target}`;
  };
</script>

{#if !app.summary}
  <div class="empty">
    <Icon name="folder" size={48} />
    <h2>{t("tidy-scan-title")}</h2>
    <p class="muted">{t("tidy-scan-text")}</p>
    <div class="actions">
      {#if system}
        <button class="btn primary" data-effect data-tidy-scan onclick={() => startScan(system.root, "tidy")}>
          <Icon name="drive" size={16} />{t("tidy-scan-system", { drive: drive(system.root) })}
        </button>
      {/if}
      <button class="btn" class:primary={!system} onclick={() => (app.view = "home")}>{t("tidy-scan-other")}</button>
    </div>
  </div>
{:else}
  <section class="page tidy" bind:this={page}>
    <header class="page-head">
      <div>
        <h1>{t("tidy-title")}</h1>
        <p>{t("tidy-intro")}</p>
      </div>
      <span class="tag safe note"><Icon name="shield" size={13} />{t("tidy-quarantine-note")}</span>
    </header>

    <div class="stats">
      <StatCard label={t("tidy-empty-title")} value={empty ? formatCount(empty.matched, loc) : null} unit={empty ? t("stat-of-folders", { total: formatCount(empty.totalDirs, loc) }) : t("stat-checking")} />
      <StatCard label={t("tidy-zero-title")} value={zero ? formatCount(zero.matched, loc) : null} unit={zero ? "" : t("stat-checking")} />
      <StatCard label={t("tidy-links-title")} value={links ? formatCount(links.rows.length, loc) : null} unit={checking ? t("stat-checking") : links ? t("stat-checked-n", { count: links.checked }) : ""} />
      {#if expert()}
        <StatCard label={t("tidy-junctions-title")} value={junctions ? formatCount(junctions.rows.length, loc) : null} unit={t("stat-listed-only")} />
      {/if}
    </div>
    <p class="muted small hint">{expert() ? t("action-select-hint") : t("sel-tick-hint")}</p>

    <div class="panel block">
      <h2>
        <GroupCheck items={selItems} group="empty" {picked} onchange={setPicked} expert={expert()} name={t("tidy-empty-title")} />
        <Icon name="folder" size={18} />{t("tidy-empty-title")}
      </h2>
      {#if empty}
        <p class="muted small">{t("tidy-empty-summary", { count: empty.matched, total: empty.totalDirs })} {t("tidy-empty-clutter-hint")}</p>
      {/if}
      <div class="body">
        {#if empty?.rows.length}
          <ItemTable rows={empty.rows} {selected} onselect={toggle} oncontext={context} onreveal={reveal} aux={{ label: t("details-contains"), value: contents }} checks />
        {:else if empty}
          <p class="muted none"><Icon name="check" size={18} />{t("tidy-empty-none")}</p>
        {/if}
      </div>
    </div>

    <div class="panel block">
      <h2>
        <GroupCheck items={selItems} group="zero" {picked} onchange={setPicked} expert={expert()} name={t("tidy-zero-title")} />
        <Icon name="file" size={18} />{t("tidy-zero-title")}
      </h2>
      {#if zero}
        <p class="muted small">{t("tidy-zero-summary", { count: zero.matched })} {t("tidy-zero-hint")}</p>
      {/if}
      <div class="body">
        {#if zero?.rows.length}
          <ItemTable rows={zero.rows} {selected} onselect={toggle} oncontext={context} onreveal={reveal} checks />
        {:else if zero}
          <p class="muted none"><Icon name="check" size={18} />{t("tidy-zero-none")}</p>
        {/if}
      </div>
    </div>

    <div class="panel block">
      <h2>
        <GroupCheck items={selItems} group="links" {picked} onchange={setPicked} expert={expert()} name={t("tidy-links-title")} />
        <Icon name="link" size={18} />{t("tidy-links-title")}
      </h2>
      {#if checking}
        <p class="muted small">{t("tidy-links-checking")}</p>
      {:else if links}
        <p class="muted small">
          {t("tidy-links-summary", { checked: links.checked, unchecked: links.unchecked + links.unreadable })}
        </p>
      {/if}
      <div class="body">
        {#if links?.rows.length}
          <ItemTable rows={links.rows} {selected} onselect={toggle} oncontext={context} onreveal={reveal} aux={{ label: t("tidy-target"), value: target, wide: true }} checks />
        {:else if links && !checking}
          <p class="muted none"><Icon name="check" size={18} />{t("tidy-links-none")}</p>
        {/if}
      </div>
    </div>

    {#if expert()}
      <div class="panel block">
        <h2>
          <Icon name="link" size={18} />{t("tidy-junctions-title")}
        </h2>
        {#if junctions}
          <p class="muted small">{t("tidy-junctions-summary", { checked: junctions.checked })}</p>
        {/if}
        <div class="body">
          {#if junctions?.rows.length}
            <ItemTable rows={junctions.rows} {selected} onselect={toggle} oncontext={context} onreveal={reveal} aux={{ label: t("tidy-target"), value: target, wide: true }} />
          {:else if junctions}
            <p class="muted none"><Icon name="check" size={18} />{t("tidy-junctions-none")}</p>
          {/if}
        </div>
      </div>
    {/if}
    <SelectionBar
      items={selItems}
      {picked}
      onchange={setPicked}
      expert={expert()}
      scope={page}
      primary
      noLevels
      sticky={false}
      action={{ label: (count, size) => t("sel-quarantine-n", { count, size }), hint: t("tidy-quarantine-hint"), testid: "tidy", run: quarantine }}
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
    max-width: 460px;
    margin: 0 auto;
    text-align: center;
    color: var(--text-muted);
  }
  .empty h2 {
    margin: 0;
    color: var(--text);
    font-size: 1.2em;
  }
  .empty p {
    margin: 0;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    margin-top: 4px;
  }
  .actions .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .tidy {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 18px;
  }
  .tidy > .page-head,
  .tidy > .stats {
    margin-bottom: 0;
  }
  .note {
    flex: none;
    font-weight: 500;
  }
  .tidy > .hint {
    margin: -4px 0 0;
  }
  .block {
    flex: 1 0 200px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 14px 0 0;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 18px 2px;
    font-size: 1.06em;
  }
  .small {
    margin: 0 18px 8px;
  }
  .body {
    flex: 1;
    overflow: auto;
    min-height: 0;
    border-top: 1px solid var(--border);
  }
  .none {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 16px;
  }
</style>
