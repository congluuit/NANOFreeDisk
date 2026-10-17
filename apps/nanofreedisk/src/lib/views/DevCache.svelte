<script lang="ts">
  // DevCache Manager (PHASE4_DEVCACHE §2.5): developer caches by vendor → app →
  // folder, toolchains, repositories and their build output, package-store commands
  // and caches no rule knows yet. Every delete goes through the Planner → SafetyGate
  // → Executor → quarantine and shows up in the Activity log.
  import { formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, fail } from "../state.svelte";
  import { reasonText } from "../clean/state.svelte";
  import { devApi } from "../devcache/api";
  import { dev } from "../devcache/state.svelte";
  import type { DevTab } from "../devcache/types";
  import Icon from "../components/Icon.svelte";
  import AiTab from "../components/devcache/AiTab.svelte";
  import CacheTab from "../components/devcache/CacheTab.svelte";
  import ToolchainsTab from "../components/devcache/ToolchainsTab.svelte";
  import ReposTab from "../components/devcache/ReposTab.svelte";
  import StoresTab from "../components/devcache/StoresTab.svelte";
  import UnknownTab from "../components/devcache/UnknownTab.svelte";

  const loc = $derived(intlLocale());
  const TABS: { id: DevTab; icon: string }[] = [
    { id: "cache", icon: "devcache" },
    { id: "ai", icon: "agent" },
    { id: "toolchains", icon: "cpu" },
    { id: "repos", icon: "folder" },
    { id: "stores", icon: "clean" },
    { id: "unknown", icon: "search" },
  ];
</script>

<section class="page devcache">
  <header>
    <div>
      <h1>{t("dev-title")}</h1>
      <p class="muted">{t("dev-intro")}</p>
    </div>
    <button class="btn ghost" onclick={() => (app.view = "activity")} title={t("clean-activity-hint")}>
      <Icon name="undo" />{t("nav-activity")}
    </button>
  </header>

  <div class="tabs" role="tablist">
    {#each TABS as tab (tab.id)}
      <button role="tab" class="tab" class:on={dev.tab === tab.id} aria-selected={dev.tab === tab.id} data-devtab={tab.id} onclick={() => (dev.tab = tab.id)}>
        <Icon name={tab.icon} size={16} />{t(`dev-tab-${tab.id}`)}
      </button>
    {/each}
  </div>

  {#if dev.running}
    <div class="card running" aria-live="polite">
      <div class="row">
        <strong>{t("clean-running", { done: dev.running.done, total: dev.running.total })}</strong>
        <span class="muted">{t("clean-freed-so-far", { size: formatSize(dev.running.freed, loc) })}</span>
        <span class="grow"></span>
        <button class="btn" onclick={() => devApi.cancel().catch(fail)}>{t("clean-cancel")}</button>
      </div>
      <div class="bar"><span style="width: {dev.running.total ? (dev.running.done / dev.running.total) * 100 : 0}%"></span></div>
      <p class="muted small ellipsis mono">{dev.running.path}</p>
    </div>
  {:else if dev.last}
    {@const r = dev.last}
    <div class="card result" class:bad={r.failed > 0}>
      <Icon name={r.failed ? "alert" : "check"} size={22} />
      <div class="grow">
        <strong>{t("clean-done-title", { size: formatSize(r.freed, loc) })}</strong>
        <p class="muted small">
          {t("clean-done-detail", { done: r.done, partial: r.partial, skipped: r.skipped, failed: r.failed })}
          {#if r.cancelled}· {t("clean-was-cancelled")}{/if}
        </p>
        {#if r.problems.length}
          <details>
            <summary class="small">{t("clean-problems", { count: r.problems.length })}</summary>
            <ul class="problems">
              {#each r.problems.slice(0, 20) as p (p.path)}
                <li class="small"><span class="mono ellipsis" title={p.path}>{p.path}</span><span class="muted">{t(`clean-status-${p.status}`)}{p.reason ? ` · ${reasonText(p.reason)}` : ""}</span></li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
      <button class="btn" onclick={() => (app.view = "activity")}><Icon name="undo" size={16} />{t("clean-undo")}</button>
      <button class="btn ghost icon" onclick={() => (dev.last = null)} aria-label={t("clean-dismiss")}><Icon name="x" size={16} /></button>
    </div>
  {/if}

  {#if dev.tab === "cache"}
    <CacheTab />
  {:else if dev.tab === "ai"}
    <AiTab />
  {:else if dev.tab === "toolchains"}
    <ToolchainsTab />
  {:else if dev.tab === "repos"}
    <ReposTab />
  {:else if dev.tab === "stores"}
    <StoresTab />
  {:else}
    <UnknownTab />
  {/if}
</section>

<style>
  .page {
    height: 100%;
    overflow: auto;
    padding: 28px 32px 40px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 1.6em;
  }
  header p {
    margin: 0;
    max-width: 75ch;
  }
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--border);
    flex-wrap: wrap;
  }
  .tab {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 8px 12px;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-muted);
    cursor: pointer;
    font: inherit;
  }
  .tab.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .running,
  .result {
    padding: 14px 16px;
  }
  .running .bar {
    margin: 10px 0 6px;
    height: 8px;
  }
  .running p,
  .result p {
    margin: 2px 0 0;
  }
  .result {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    border-left: 4px solid var(--risk-safe);
  }
  .result.bad {
    border-left-color: var(--danger);
  }
  .problems {
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
    max-height: 180px;
    overflow: auto;
  }
  .problems li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
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

  /* shared by the tab components */
  .devcache :global(.dc-card) {
    padding: 14px 16px;
  }
  .devcache :global(.dc-loading) {
    display: flex;
    gap: 10px;
    align-items: center;
    color: var(--text-muted);
    padding: 14px 16px;
  }
  .devcache :global(.dc-row) {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  .devcache :global(.dc-grow) {
    flex: 1;
  }
  .devcache :global(.dc-small) {
    font-size: 0.86em;
  }
  .devcache :global(.dc-badge) {
    font-size: 0.75em;
    padding: 1px 7px;
    border-radius: 999px;
    border: 1px solid currentColor;
    white-space: nowrap;
  }
  .devcache :global(.dc-badge.safe) {
    color: var(--risk-safe);
  }
  .devcache :global(.dc-badge.caution) {
    color: var(--risk-caution);
  }
  .devcache :global(.dc-badge.advanced) {
    color: var(--risk-advanced);
  }
  .devcache :global(.dc-badge.protected) {
    color: var(--risk-protected);
  }
  .devcache :global(.dc-badge.muted) {
    color: var(--text-muted);
  }
  .devcache :global(.dc-warn) {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 4px 0 0;
    color: var(--risk-caution);
  }
  .devcache :global(.dc-note) {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 10px 14px;
    color: var(--text-muted);
  }
  .devcache :global(.dc-list) {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .devcache :global(.dc-item) {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    gap: 10px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }
  .devcache :global(.dc-item:first-child) {
    border-top: none;
  }
  .devcache :global(.dc-size) {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    min-width: 110px;
  }
  .devcache :global(.dc-code) {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 4px 8px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-width: 100%;
  }
  .devcache :global(.dc-code code) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .devcache :global(.dc-footer) {
    display: flex;
    gap: 12px;
    align-items: center;
    position: sticky;
    bottom: 0;
    box-shadow: var(--shadow);
    padding: 14px 16px;
  }
  .devcache :global(.dc-backdrop) {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--bg) 65%, transparent);
    backdrop-filter: blur(2px);
  }
  .devcache :global(.dc-dialog) {
    width: min(640px, 94vw);
    max-height: 86vh;
    overflow: auto;
    padding: 22px 24px;
    box-shadow: var(--shadow);
  }
  .devcache :global(.dc-dialog h2) {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0 0 8px;
    font-size: 1.2em;
  }
  .devcache :global(.dc-dialog footer) {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
  .devcache :global(.dc-confirm-list) {
    list-style: none;
    margin: 8px 0;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-height: 220px;
    overflow: auto;
  }
  .devcache :global(.dc-confirm-list li) {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 3px 4px;
  }
</style>
