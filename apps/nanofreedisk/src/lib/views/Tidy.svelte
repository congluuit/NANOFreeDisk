<script lang="ts">
  // Tidy up (PLAN §13 A3): empty folder trees and shortcuts whose target is gone.
  // Picked items go to the quarantine through the SafetyGate (restorable from the
  // Activity log for 7 days); broken junctions are listed only — links are removed
  // in Explorer.
  import { api } from "../api";
  import { itemMenu } from "../actions";
  import { formatCount } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, askDelete, fail } from "../state.svelte";
  import ContextMenu, { type MenuItem } from "../components/ContextMenu.svelte";
  import GroupCheck from "../components/GroupCheck.svelte";
  import Icon from "../components/Icon.svelte";
  import ItemTable from "../components/ItemTable.svelte";
  import SelectionBar from "../components/SelectionBar.svelte";
  import type { SelItem } from "../selection";
  import { expert } from "../state.svelte";
  import type { EmptyRow, EmptyRows, Located, ShortcutRow, ShortcutRows } from "../types";

  let empty = $state<EmptyRows | null>(null);
  let links = $state<ShortcutRows | null>(null);
  let junctions = $state<ShortcutRows | null>(null);
  let checking = $state(false);
  let selected = $state<number[]>([]);
  let menu: { x: number; y: number; items: MenuItem[] } | null = $state(null);
  const loc = $derived(intlLocale());
  const rootId = $derived(app.summary?.rootId ?? 0);
  let page = $state<HTMLElement | null>(null);
  // empty folders and broken shortcuts; broken junctions are listed only (removed in
  // Explorer), so Select all never picks them
  const selItems = $derived<SelItem[]>([
    ...(empty?.rows ?? []).map((r) => ({ id: String(r.id), bytes: r.onDisk, files: r.files, group: "empty" })),
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

  function toggle(id: number, additive: boolean) {
    if (additive) selected = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
    else selected = [id];
  }

  function context(id: number, x: number, y: number) {
    selected = [id];
    menu = { x, y, items: itemMenu(id, { recycle: false }) };
  }

  const reveal = (id: number) => api.reveal(id).catch(fail);
  const nested = (r: Located) => {
    const n = (r as EmptyRow).nested;
    return n ? t("tidy-nested", { count: n }) : t("tidy-empty-self");
  };
  const target = (r: Located) => {
    const s = r as ShortcutRow;
    return `${t(`tidy-status-${s.status}`)} · ${s.target}`;
  };
</script>

{#if !app.summary}
  <div class="empty">
    <Icon name="folder" size={48} />
    <p class="muted">{t("analyze-empty")}</p>
    <button class="btn primary" onclick={() => (app.view = "home")}>{t("nav-home")}</button>
  </div>
{:else}
  <section class="page" bind:this={page}>
    <header>
      <h1>{t("tidy-title")}</h1>
      <p class="muted">{t("tidy-intro")}</p>
      <p class="note"><Icon name="shield" size={16} />{t("tidy-quarantine-note")}</p>
    </header>

    <p class="muted small hint">{t("action-select-hint")}</p>

    <div class="card block">
      <h2>
        <GroupCheck items={selItems} group="empty" {picked} onchange={setPicked} expert={expert()} name={t("tidy-empty-title")} />
        <Icon name="folder" size={18} />{t("tidy-empty-title")}
        {#if empty}<span class="count">{formatCount(empty.matched, loc)}</span>{/if}
      </h2>
      {#if empty}
        <p class="muted small">{t("tidy-empty-summary", { count: empty.matched, total: empty.totalDirs })}</p>
      {/if}
      <div class="body">
        {#if empty?.rows.length}
          <ItemTable rows={empty.rows} {selected} onselect={toggle} oncontext={context} onreveal={reveal} aux={{ label: t("details-contains"), value: nested }} />
        {:else if empty}
          <p class="muted none"><Icon name="check" size={18} />{t("tidy-empty-none")}</p>
        {/if}
      </div>
    </div>

    <div class="card block">
      <h2>
        <GroupCheck items={selItems} group="links" {picked} onchange={setPicked} expert={expert()} name={t("tidy-links-title")} />
        <Icon name="link" size={18} />{t("tidy-links-title")}
        {#if links}<span class="count">{formatCount(links.rows.length, loc)}</span>{/if}
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
          <ItemTable rows={links.rows} {selected} onselect={toggle} oncontext={context} onreveal={reveal} aux={{ label: t("tidy-target"), value: target, wide: true }} />
        {:else if links && !checking}
          <p class="muted none"><Icon name="check" size={18} />{t("tidy-links-none")}</p>
        {/if}
      </div>
    </div>

    <div class="card block">
      <h2>
        <Icon name="link" size={18} />{t("tidy-junctions-title")}
        {#if junctions}<span class="count">{formatCount(junctions.rows.length, loc)}</span>{/if}
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
    <SelectionBar
      items={selItems}
      {picked}
      onchange={setPicked}
      expert={expert()}
      scope={page}
      primary
      noLevels
      sticky={false}
      action={{ label: (count, size) => t("sel-quarantine-n", { count, size }), hint: t("tidy-quarantine-hint"), testid: "tidy", run: () => askDelete(selected, "quarantine", "tidy") }}
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
  .page {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 20px 24px;
    min-height: 0;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 1.45em;
  }
  header p {
    margin: 0 0 4px;
    max-width: 80ch;
  }
  .note {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    color: var(--risk-safe);
    font-size: 0.9em;
  }
  .hint {
    margin: 0;
    font-size: 0.86em;
  }
  .block {
    flex: 1;
    min-height: 140px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding-top: 10px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 14px 2px;
    font-size: 1.05em;
  }
  .count {
    font-size: 0.8em;
    font-weight: normal;
    padding: 0 8px;
    border-radius: 8px;
    background: var(--accent-soft);
  }
  .small {
    font-size: 0.86em;
    margin: 0 14px 6px;
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
