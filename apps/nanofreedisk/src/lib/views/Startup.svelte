<script lang="ts">
  // Startup (PLAN §13.2 E2, docs/STARTUP.md): programs that start with Windows,
  // from the Run keys and the Startup folders, turned on or off the way Task
  // Manager does. Nothing is deleted: turning one off only sets Task Manager's
  // flag, and Undo (or Task Manager) turns it back on. All-users entries need
  // administrator rights and are left to Task Manager.
  import { formatCount, formatDate } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { canChange, needsConfirm, publisherOf, visibleEntries, type StartupFilter, type StartupSort } from "../startup/model";
  import { loadStartup, openTaskManager, setEnabled, su, undoLast } from "../startup/state.svelte";
  import type { StartupEntry } from "../startup/types";
  import AppIcon from "../components/programs/AppIcon.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Icon from "../components/Icon.svelte";

  const loc = $derived(intlLocale());
  const FILTERS: StartupFilter[] = ["all", "enabled", "disabled", "user", "machine"];
  const SORTS: StartupSort[] = ["name", "status"];

  $effect(() => {
    if (!su.list && !su.loading) void loadStartup();
  });

  const shown = $derived(visibleEntries(su.list ?? [], su.query, su.filter, su.sort));
  const on = $derived((su.list ?? []).filter((e) => e.enabled).length);

  function toggle(e: StartupEntry) {
    if (e.enabled && needsConfirm(e)) su.confirm = e;
    else void setEnabled(e, !e.enabled);
  }
</script>

<section class="page startup">
  <header class="page-head">
    <div>
      <h1>{t("su-title")}</h1>
      <p>{t("su-intro")}</p>
    </div>
    <div class="page-actions">
      <span class="tag safe note"><Icon name="shield" size={13} />{t("su-safety-note")}</span>
      <button class="btn ghost" onclick={openTaskManager} title={t("su-task-manager-hint")} data-su-taskmgr>
        <Icon name="tasks" />{t("su-task-manager")}
      </button>
    </div>
  </header>

  {#if su.undo}
    {@const u = su.undo}
    <div class="card undo" role="status" data-su-undo>
      <Icon name={u.enabled ? "check" : "power"} size={18} />
      <span class="grow">{t(u.enabled ? "su-undo-enabled" : "su-undo-disabled", { name: u.name })}</span>
      <button class="btn" disabled={!!su.busy} onclick={undoLast} data-su-undo-run><Icon name="undo" size={16} />{t("su-undo")}</button>
      <button class="btn ghost icon" onclick={() => (su.undo = null)} aria-label={t("clean-dismiss")}><Icon name="x" size={16} /></button>
    </div>
  {/if}

  <div class="card toolbar">
    <div class="row">
      <label class="search">
        <Icon name="search" size={16} />
        <input type="search" placeholder={t("su-search")} bind:value={su.query} aria-label={t("su-search")} data-su-search />
      </label>
      <select bind:value={su.filter} aria-label={t("su-filter")} data-su-filter>
        {#each FILTERS as f (f)}<option value={f}>{t(`su-filter-${f}`)}</option>{/each}
      </select>
      <select bind:value={su.sort} aria-label={t("su-sort")}>
        {#each SORTS as s (s)}<option value={s}>{t(`su-sort-${s}`)}</option>{/each}
      </select>
      <span class="grow"></span>
      <button class="btn" onclick={() => loadStartup()} disabled={su.loading} title={t("su-refresh-hint")}>
        <Icon name="refresh" size={16} />{t("su-refresh")}
      </button>
    </div>
    {#if su.list}
      <p class="muted small summary" data-su-summary>
        {t("su-summary", { count: formatCount(su.list.length, loc), on: formatCount(on, loc), shown: formatCount(shown.length, loc) })}
      </p>
    {/if}
  </div>

  {#if su.loading && !su.list}
    <div class="card loading"><Icon name="search" size={22} /><span>{t("su-loading")}</span></div>
  {:else if su.list}
    <div class="card list" data-su-list>
      {#each shown as e (e.id)}
        {@const pub = publisherOf(e)}
        <div class="entry" class:off={!e.enabled} data-su-entry={e.id}>
          <AppIcon id={e.id} name={e.name} />
          <div class="who">
            <div class="name ellipsis" title={e.name}>{e.name}</div>
            <div class="muted small ellipsis" title={e.command}>
              {#if pub}<span title={pub.verified ? t("su-publisher-signed") : t("su-publisher-unsigned")}>{pub.name}{pub.verified ? "" : " *"}</span> · {/if}<span class="mono">{e.command}</span>
            </div>
            <div class="badges">
              <span class="badge" title={e.location}>{t(`su-origin-${e.origin}`)}</span>
              {#if e.scope === "machine"}<span class="badge" title={t("su-machine-hint")}><Icon name="shield" size={11} />{t("su-machine")}</span>{/if}
              {#if e.caution}<span class="badge caution" title={t(`su-caution-${e.caution}-hint`)}><Icon name="alert" size={11} />{t(`su-caution-${e.caution}`)}</span>{/if}
              {#if e.missing}<span class="badge caution" title={t("su-missing-hint")}>{t("su-missing")}</span>{/if}
            </div>
          </div>
          <span class="state small" class:on={e.enabled}>
            {e.enabled ? t("su-state-on") : t("su-state-off")}
            {#if !e.enabled && e.disabledAt}<span class="muted">· {formatDate(e.disabledAt, loc)}</span>{/if}
          </span>
          <div class="acts">
            {#if canChange(e)}
              <button class="btn" class:ghost={!e.enabled} disabled={!!su.busy} onclick={() => toggle(e)} data-su-toggle={e.id} title={e.enabled ? t("su-disable-hint") : t("su-enable-hint")}>
                <Icon name={e.enabled ? "power" : "check"} size={16} />{e.enabled ? t("su-disable") : t("su-enable")}
              </button>
            {:else}
              <span class="muted small why">{t("su-needs-admin")}</span>
              <button class="btn ghost" onclick={openTaskManager} title={t("su-task-manager-hint")}><Icon name="tasks" size={16} />{t("su-task-manager")}</button>
            {/if}
          </div>
        </div>
      {:else}
        <p class="muted none"><Icon name="search" size={18} />{t("su-none")}</p>
      {/each}
    </div>
    <p class="muted small scope-note"><Icon name="info" size={14} />{t("su-out-of-scope")}</p>
  {/if}
</section>

{#if su.confirm}
  {@const c = su.confirm}
  <ConfirmDialog
    title={t("su-confirm-title", { name: c.name })}
    body={t(`su-confirm-${c.caution ?? "windows"}`)}
    detail={c.command}
    confirm={t("su-disable")}
    icon="alert"
    onconfirm={() => setEnabled(c, false)}
    oncancel={() => (su.confirm = null)}
  />
{/if}

<style>
  .startup {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .startup > :global(*) {
    flex-shrink: 0;
  }
  .startup > .page-head {
    margin-bottom: 0;
  }
  .note {
    flex: none;
    font-weight: 500;
    white-space: normal;
  }
  .undo {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 10px 16px;
    border-left: 4px solid var(--accent);
  }
  .toolbar {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 16px;
  }
  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
    min-width: 0;
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
  .summary {
    margin: 0;
  }
  .loading {
    display: flex;
    gap: 10px;
    align-items: center;
    color: var(--text-muted);
    padding: 14px 16px;
  }
  .list {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .entry {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) 150px auto;
    gap: 12px;
    align-items: center;
    padding: 9px 14px;
    border-bottom: 1px solid var(--border);
  }
  .entry:last-child {
    border-bottom: none;
  }
  .entry:hover {
    background: var(--hover);
  }
  .entry.off .name {
    color: var(--text-muted);
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
  .badge {
    display: inline-flex;
    gap: 3px;
    align-items: center;
    font-size: 0.74em;
    padding: 1px 7px;
    border-radius: 999px;
    border: 1px solid currentColor;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .badge.caution {
    color: var(--risk-caution);
  }
  .state {
    color: var(--text-muted);
  }
  .state.on {
    color: var(--risk-safe-ink);
    font-weight: 600;
  }
  .acts {
    display: flex;
    gap: 6px;
    align-items: center;
    justify-content: flex-end;
  }
  .why {
    max-width: 200px;
    text-align: right;
  }
  .none {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 16px;
  }
  .scope-note {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 0;
  }
  .small {
    font-size: 0.86em;
  }
</style>
