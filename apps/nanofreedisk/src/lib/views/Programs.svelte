<script lang="ts">
  // Programs (PLAN §13.2 E1, docs/UNINSTALLER.md): installed programs and their own
  // uninstallers, what they leave behind, and the junk installed programs pile up.
  // The vendor's uninstaller always runs in its own window; leftovers and junk go
  // Planner → SafetyGate → Executor into the quarantine (registry keys to .reg
  // backups) and every removal can be undone from the Activity log.
  import { formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app } from "../state.svelte";
  import { forcedLeftovers, pg, programOf, stopWaiting, type ProgramsTab } from "../programs/state.svelte";
  import { expert } from "../state.svelte";
  import Icon from "../components/Icon.svelte";
  import InstalledTab from "../components/programs/InstalledTab.svelte";
  import JunkTab from "../components/programs/JunkTab.svelte";
  import LeftoversCard from "../components/programs/LeftoversCard.svelte";
  import UninstallDialog from "../components/programs/UninstallDialog.svelte";

  const loc = $derived(intlLocale());
  const TABS: { id: ProgramsTab; icon: string }[] = [
    { id: "installed", icon: "apps" },
    { id: "junk", icon: "clean" },
  ];
</script>

<section class="page programs">
  <header class="page-head">
    <div>
      <h1>{t("pg-title")}</h1>
      <p>{t("pg-intro")}</p>
    </div>
    <div class="page-actions">
      <span class="tag safe note"><Icon name="shield" size={13} />{t("pg-safety-note")}</span>
      <button class="btn ghost" onclick={() => (app.view = "activity")} title={t("clean-activity-hint")}>
        <Icon name="undo" />{t("nav-activity")}
      </button>
    </div>
  </header>

  <div class="tabrow" role="tablist">
    {#each TABS as tab (tab.id)}
      <button role="tab" class="tab" class:on={pg.tab === tab.id} aria-selected={pg.tab === tab.id} data-pgtab={tab.id} onclick={() => (pg.tab = tab.id)}>
        <Icon name={tab.icon} size={16} />{t(`pg-tab-${tab.id}`)}
      </button>
    {/each}
  </div>

  {#if pg.current}
    {@const c = pg.current}
    <div class="card running" aria-live="polite" data-pg-running>
      <div class="row">
        <span class="spin"><Icon name="refresh" size={18} /></span>
        <div class="grow">
          <strong>{c.phase === "scanning" ? t("pg-scanning", { name: c.name }) : t("pg-running", { name: c.name })}</strong>
          <p class="muted small">
            {c.phase === "scanning" ? t("pg-scanning-hint") : t("pg-running-hint")}
            {#if pg.queue.length}· {t("pg-queue", { count: pg.queue.length })}{/if}
          </p>
        </div>
        {#if c.phase !== "scanning"}
          <button class="btn" onclick={stopWaiting} data-pg-finished title={t("pg-finished-hint")}>
            <Icon name="check" size={16} />{t("pg-finished")}
          </button>
        {/if}
      </div>
    </div>
  {/if}

  {#if pg.running && !pg.current}
    <div class="card running" aria-live="polite">
      <div class="row">
        <strong>{t("clean-running", { done: pg.running.done, total: pg.running.total })}</strong>
        <span class="muted">{t("clean-freed-so-far", { size: formatSize(pg.running.freed, loc) })}</span>
      </div>
      <div class="bar"><span style="width: {pg.running.total ? (pg.running.done / pg.running.total) * 100 : 0}%"></span></div>
      <p class="muted small ellipsis mono">{pg.running.path}</p>
    </div>
  {/if}

  {#each pg.still as s (s.id)}
    {@const p = programOf(s.id)}
    <div class="card still" data-pg-still={s.id}>
      <Icon name="alert" size={20} />
      <div class="grow">
        <strong>{t("pg-still-title", { name: p?.name ?? s.id })}</strong>
        <p class="muted small">{t("pg-still-body")}</p>
      </div>
      {#if expert()}
        <button class="btn" onclick={() => forcedLeftovers(s.id)} title={t("pg-forced-hint")}>{t("pg-forced")}</button>
      {/if}
      <button class="btn ghost icon" onclick={() => (pg.still = pg.still.filter((x) => x.id !== s.id))} aria-label={t("clean-dismiss")}><Icon name="x" size={16} /></button>
    </div>
  {/each}

  {#each pg.leftovers as r (r.program.id)}
    <LeftoversCard report={r} />
  {/each}

  {#if pg.tab === "installed"}
    <InstalledTab />
  {:else}
    <JunkTab />
  {/if}
</section>

{#if pg.confirm}
  <UninstallDialog ids={pg.confirm.ids} />
{/if}

<style>
  .programs {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .programs > .page-head {
    margin-bottom: 0;
  }
  .note {
    flex: none;
    font-weight: 500;
    white-space: normal;
  }
  .running {
    padding: 14px 16px;
    border-left: 4px solid var(--accent);
  }
  .running .bar {
    margin: 10px 0 6px;
    height: 8px;
  }
  .running p {
    margin: 2px 0 0;
  }
  .still {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 12px 16px;
    border-left: 4px solid var(--risk-caution);
    color: var(--text);
  }
  .still p {
    margin: 2px 0 0;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .small {
    font-size: 0.86em;
  }
  .spin {
    display: inline-flex;
    color: var(--accent);
    animation: spin 1.4s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* shared by the tab components */
  .programs :global(.pg-card) {
    padding: 14px 16px;
  }
  .programs :global(.pg-loading) {
    display: flex;
    gap: 10px;
    align-items: center;
    color: var(--text-muted);
    padding: 14px 16px;
  }
  .programs :global(.pg-row) {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  .programs :global(.pg-grow) {
    flex: 1;
    min-width: 0;
  }
  .programs :global(.pg-small) {
    font-size: 0.86em;
  }
  .programs :global(.pg-badge) {
    font-size: 0.74em;
    padding: 1px 7px;
    border-radius: 999px;
    border: 1px solid currentColor;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .programs :global(.pg-badge.safe) {
    color: var(--risk-safe);
  }
  .programs :global(.pg-badge.caution) {
    color: var(--risk-caution);
  }
  .programs :global(.pg-badge.advanced) {
    color: var(--risk-advanced);
  }
  .programs :global(.pg-badge.protected),
  .programs :global(.pg-badge.blocked) {
    color: var(--risk-protected);
  }
  .programs :global(.pg-badge.accent) {
    color: var(--accent);
  }
  .programs :global(.pg-num) {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    text-align: right;
  }
</style>
