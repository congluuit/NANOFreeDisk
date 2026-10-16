<script lang="ts">
  // DevCache → Package stores (C4): the package managers' own cleanup commands.
  // A command that can be undone by downloading again runs only after the dialog has
  // shown its exact command line (absolute program path and arguments) and the user
  // confirmed; the others are guide-only (copy and run yourself).
  import { formatDate, formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { app, fail } from "../../state.svelte";
  import { devApi } from "../../devcache/api";
  import { copyText, dev, loadStores, runStore } from "../../devcache/state.svelte";
  import type { StoreInfo } from "../../devcache/types";
  import Icon from "../Icon.svelte";

  const loc = $derived(intlLocale());
  $effect(() => {
    if (!dev.stores && !dev.storesLoading) void loadStores();
  });
  const list = $derived([...(dev.stores ?? [])].sort((a, b) => b.bytes - a.bytes));

  async function go(info: StoreInfo) {
    dev.storeConfirm = null;
    await runStore(info);
  }
  const ok = $derived(
    dev.storeLast ? dev.storeLast.output.code === 0 && !dev.storeLast.output.timedOut && !dev.storeLast.output.cancelled : false,
  );
</script>

<div class="card dc-note">
  <Icon name="shield" size={18} />
  <span class="dc-small">{t("dev-stores-note")}</span>
  <span class="dc-grow"></span>
  <button class="btn ghost" onclick={() => loadStores()} disabled={dev.storesLoading || !!dev.storeRunning} title={t("clean-rescan")}><Icon name="refresh" /></button>
</div>

{#if dev.storeRunning}
  <div class="card dc-loading" aria-live="polite">
    <Icon name="clock" size={22} /><span>{t("dev-store-running", { id: dev.storeRunning })}</span>
    <span class="dc-grow"></span>
    <button class="btn" onclick={() => devApi.cancel().catch(fail)}>{t("clean-cancel")}</button>
  </div>
{:else if dev.storeLast}
  {@const r = dev.storeLast}
  <div class="card dc-card last" class:bad={!ok}>
    <div class="dc-row">
      <Icon name={ok ? "check" : "alert"} size={20} />
      <strong>{ok ? t("dev-store-done", { size: formatSize(r.freed, loc) }) : t("dev-store-failed")}</strong>
      <span class="muted dc-small">{formatSize(r.before, loc)} → {formatSize(r.after, loc)}</span>
      <span class="dc-grow"></span>
      <button class="btn ghost icon" onclick={() => (dev.storeLast = null)} aria-label={t("clean-dismiss")}><Icon name="x" size={16} /></button>
    </div>
    <p class="muted dc-small">{t("dev-store-recorded")}</p>
    {#if r.output.stdout || r.output.stderr}
      <details>
        <summary class="dc-small">{t("dev-store-output")}</summary>
        <pre class="mono dc-small">{(r.output.stdout + "\n" + r.output.stderr).trim()}</pre>
      </details>
    {/if}
  </div>
{/if}

{#if !dev.stores}
  <div class="card dc-loading"><Icon name="search" size={22} /><span>{t("dev-stores-loading")}</span></div>
{:else if !list.length}
  <div class="card dc-loading"><Icon name="check" size={22} /><span>{t("dev-stores-none")}</span></div>
{:else}
  <div class="card dc-card">
    <ul class="dc-list">
      {#each list as s (s.id)}
        <li class="store" data-store={s.id}>
          <div class="what">
            <div class="dc-row">
              <span class="dc-badge {s.level}">{t(`clean-level-${s.level}`)}</span>
              <strong>{s.tool}</strong>
              <span class="muted dc-small">{t(s.undo === "redownload" ? "dev-undo-redownload" : "dev-undo-none")}</span>
            </div>
            <span class="dc-code dc-small"><code class="mono" title={s.commandLine}>{s.commandLine}</code>
              <button class="btn ghost icon" onclick={() => copyText(s.commandLine)} title={t("action-copy-command")} aria-label={t("action-copy-command")}><Icon name="copy" size={14} /></button>
            </span>
            {#if !s.program}<p class="muted dc-small">{t("dev-store-missing")}</p>{/if}
            {#if s.stores.length}<p class="muted dc-small">{t("dev-last-write", { date: s.newest ? formatDate(s.newest, loc) : "—" })}</p>{/if}
          </div>
          <div class="dc-size">
            <b class="num">{formatSize(s.bytes, loc)}{s.truncated ? "+" : ""}</b>
            <div class="dc-row">
              {#if s.stores[0]}<button class="btn ghost icon" onclick={() => devApi.reveal(s.stores[0]).catch(fail)} title={t("dev-reveal")} aria-label={t("dev-reveal")}><Icon name="reveal" size={14} /></button>{/if}
              {#if s.runnable}
                <button class="btn" disabled={!!dev.storeRunning} onclick={() => (dev.storeConfirm = s)} data-effect title={t("dev-store-run-hint")}><Icon name="clean" size={16} />{t("dev-store-run")}</button>
              {:else}
                <span class="dc-badge muted" title={t("dev-store-guide-hint")}>{t("dev-store-guide")}</span>
              {/if}
            </div>
          </div>
        </li>
      {/each}
    </ul>
  </div>
{/if}

{#if dev.storeConfirm}
  {@const s = dev.storeConfirm}
  <div class="dc-backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (dev.storeConfirm = null)} onkeydown={(e) => e.key === "Escape" && (dev.storeConfirm = null)}>
    <div class="dc-dialog card" role="alertdialog" aria-modal="true" aria-labelledby="store-confirm-title">
      <h2 id="store-confirm-title"><Icon name="clean" size={20} />{t("dev-store-confirm-title", { tool: s.tool })}</h2>
      <p>{t("dev-store-confirm-body", { size: formatSize(s.bytes, loc) })}</p>
      <pre class="mono cmd" data-command>{s.commandLine}</pre>
      {#if app.settings.mode === "expert"}
        <p class="muted dc-small mono">{s.stores.join("\n")}</p>
      {/if}
      <p class="muted dc-small">{t("dev-store-confirm-note")}</p>
      <footer>
        <button class="btn" onclick={() => (dev.storeConfirm = null)}>{t("delete-cancel")}</button>
        <button class="btn danger" onclick={() => go(s)} data-effect title={t("dev-store-confirm-note")}><Icon name="clean" size={16} />{t("dev-store-confirm-go")}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .store {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }
  .store:first-child {
    border-top: none;
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .what p {
    margin: 0;
  }
  .last {
    border-left: 4px solid var(--risk-safe);
  }
  .last.bad {
    border-left-color: var(--danger);
  }
  .last p {
    margin: 6px 0 0;
  }
  pre {
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 200px;
    overflow: auto;
    background: var(--surface-2);
    padding: 8px;
    border-radius: var(--radius-sm);
    margin: 6px 0 0;
  }
  .cmd {
    font-size: 0.95em;
    border: 1px solid var(--border);
  }
</style>
