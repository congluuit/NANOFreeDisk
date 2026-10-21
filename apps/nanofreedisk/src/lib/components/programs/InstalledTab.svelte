<script lang="ts">
  // Programs → Installed: what "Apps & features" lists, with sizes, install dates,
  // who it is for and what deserves care (runtimes, drivers). Uninstalling runs the
  // vendor's own uninstaller; programs whose uninstaller is gone offer their
  // leftovers instead. Several can be ticked: they are uninstalled one after the
  // other.
  import { formatCount, formatDate, formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { expert, fail } from "../../state.svelte";
  import { programsApi } from "../../programs/api";
  import { hue, initial, removable, uninstallCommand, visiblePrograms, type ProgramFilter, type ProgramSort } from "../../programs/model";
  import { loadPrograms, pg } from "../../programs/state.svelte";
  import type { Program } from "../../programs/types";
  import type { SelItem } from "../../selection";
  import CliHint from "../CliHint.svelte";
  import Icon from "../Icon.svelte";
  import SelectionBar from "../SelectionBar.svelte";

  const loc = $derived(intlLocale());
  let scope = $state<HTMLElement | null>(null);

  $effect(() => {
    if (!pg.list && !pg.loading) void loadPrograms();
  });

  const shown = $derived(visiblePrograms(pg.list ?? [], pg.query, pg.filter, pg.sort));
  const total = $derived((pg.list ?? []).reduce((s, p) => s + p.size, 0));
  const selItems = $derived<SelItem[]>(shown.map((p) => ({ id: p.id, bytes: p.size, locked: !removable(p) || !!pg.current })));
  const FILTERS: ProgramFilter[] = ["all", "win32", "store", "user", "large", "broken"];
  const SORTS: ProgramSort[] = ["name", "size", "date"];

  function toggle(id: string) {
    pg.picked = pg.picked.includes(id) ? pg.picked.filter((x) => x !== id) : [...pg.picked, id];
  }
  function ask(ids: string[]) {
    if (ids.length) pg.confirm = { ids };
  }
  const reveal = (p: Program) => p.installLocation && programsApi.reveal(p.installLocation).catch(fail);
  function blockedText(p: Program): string {
    return t(`pg-blocked-${p.blocked}`);
  }
  const single = $derived(pg.picked.length === 1 ? (pg.list ?? []).find((p) => p.id === pg.picked[0]) : undefined);
</script>

<div class="card pg-card toolbar">
  <div class="pg-row">
    <label class="search">
      <Icon name="search" size={16} />
      <input type="search" placeholder={t("pg-search")} bind:value={pg.query} data-pg-search aria-label={t("pg-search")} />
    </label>
    <select bind:value={pg.filter} aria-label={t("pg-filter")} data-pg-filter>
      {#each FILTERS as f (f)}<option value={f}>{t(`pg-filter-${f}`)}</option>{/each}
    </select>
    <select bind:value={pg.sort} aria-label={t("pg-sort")}>
      {#each SORTS as s (s)}<option value={s}>{t(`pg-sort-${s}`)}</option>{/each}
    </select>
    <label class="pg-small store"><input type="checkbox" bind:checked={pg.store} onchange={() => loadPrograms()} />{t("pg-store")}</label>
    <span class="pg-grow"></span>
    <button class="btn" onclick={() => loadPrograms()} disabled={pg.loading || !!pg.current} data-effect title={t("pg-refresh-hint")}>
      <Icon name="refresh" size={16} />{t("pg-refresh")}
    </button>
  </div>
  {#if pg.list}
    <p class="muted pg-small summary" data-pg-summary>
      {t("pg-summary", { count: formatCount(pg.list.length, loc), size: formatSize(total, loc), shown: formatCount(shown.length, loc) })}
    </p>
  {/if}
</div>

{#if pg.loading && !pg.list}
  <div class="card pg-loading"><Icon name="search" size={22} /><span>{t("pg-loading")}</span></div>
{:else if pg.list}
  <div class="card list" bind:this={scope} data-pg-list>
    {#each shown as p (p.id)}
      <div class="prog" class:picked={pg.picked.includes(p.id)} data-pg-program={p.id}>
        <input
          type="checkbox"
          checked={pg.picked.includes(p.id)}
          disabled={!removable(p) || !!pg.current}
          onchange={() => toggle(p.id)}
          aria-label={t("pg-pick", { name: p.name })}
        />
        <span class="avatar" style="--h: {hue(p.name)}" aria-hidden="true">{initial(p.name)}</span>
        <div class="who">
          <div class="name ellipsis" title={p.name}>{p.name}</div>
          <div class="muted pg-small ellipsis">
            {[p.publisher, p.version].filter(Boolean).join(" · ") || t("pg-unknown-publisher")}
          </div>
          <div class="badges">
            {#if p.source === "store"}<span class="pg-badge accent">{t("pg-badge-store")}</span>{/if}
            {#if p.scope === "machine"}<span class="pg-badge" title={t("pg-badge-machine-hint")}><Icon name="shield" size={11} />{t("pg-badge-machine")}</span>{/if}
            {#if p.caution}<span class="pg-badge caution" title={t(`pg-caution-${p.caution}-hint`)}>{t(`pg-caution-${p.caution}`)}</span>{/if}
            {#if p.broken}<span class="pg-badge caution" title={t("pg-broken-hint")}>{t("pg-broken")}</span>{/if}
            {#if p.blocked}<span class="pg-badge blocked" title={blockedText(p)}><Icon name="lock" size={11} />{t("pg-badge-blocked")}</span>{/if}
          </div>
        </div>
        <span class="pg-num size" title={p.sizeMeasured ? t("pg-size-measured") : t("pg-size-registered")}>{p.size ? formatSize(p.size, loc) : "—"}</span>
        <span class="muted pg-small date">{p.installedAt ? formatDate(p.installedAt, loc) : ""}</span>
        <div class="acts">
          {#if p.installLocation}
            <button class="btn ghost icon" onclick={() => reveal(p)} title={t("pg-reveal")} aria-label={t("pg-reveal")}><Icon name="reveal" size={16} /></button>
          {/if}
          {#if !p.blocked}
            <button class="btn" disabled={!!pg.current} onclick={() => ask([p.id])} data-pg-uninstall={p.id} title={p.broken ? t("pg-leftovers-only-hint") : t("pg-uninstall-hint")}>
              <Icon name={p.broken ? "clean" : "trash"} size={16} />{p.broken ? t("pg-leftovers-only") : t("pg-uninstall")}
            </button>
          {:else}
            <span class="muted pg-small why" title={blockedText(p)}>{blockedText(p)}</span>
          {/if}
        </div>
      </div>
    {:else}
      <p class="muted none"><Icon name="search" size={18} />{t("pg-none")}</p>
    {/each}
  </div>
  {#if single}<CliHint command={uninstallCommand(single)} />{/if}
  <SelectionBar
    items={selItems}
    picked={pg.picked}
    onchange={(ids) => (pg.picked = ids)}
    expert={expert()}
    {scope}
    noLevels
    files={false}
    sticky={!pg.leftovers.length}
    busy={!!pg.current}
    action={{ label: (count) => t("pg-uninstall-n", { count }), icon: "trash", hint: t("pg-uninstall-n-hint"), testid: "programs", run: () => ask(pg.picked) }}
  />
{/if}

<style>
  .toolbar {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .search {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    min-width: 260px;
    flex: 1;
    max-width: 420px;
  }
  .search input {
    border: none;
    background: none;
    outline: none;
    padding: 7px 0;
    flex: 1;
    min-width: 0;
  }
  select {
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .store {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .summary {
    margin: 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .prog {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr) 90px 110px auto;
    gap: 12px;
    align-items: center;
    padding: 9px 14px;
    border-bottom: 1px solid var(--border);
  }
  .prog:last-child {
    border-bottom: none;
  }
  .prog:hover {
    background: var(--hover);
  }
  .prog.picked {
    background: var(--accent-soft);
  }
  .avatar {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 9px;
    font-weight: 700;
    color: var(--text);
    background: color-mix(in srgb, hsl(var(--h) 60% 50%) 35%, var(--surface-2));
  }
  .who {
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .badges {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    margin-top: 3px;
  }
  .badges :global(.pg-badge) {
    display: inline-flex;
    gap: 3px;
    align-items: center;
  }
  .acts {
    display: flex;
    gap: 6px;
    align-items: center;
    justify-content: flex-end;
  }
  .why {
    max-width: 220px;
    text-align: right;
  }
  .none {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 16px;
  }
</style>
