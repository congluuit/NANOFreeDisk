<script lang="ts">
  import { api } from "../api";
  import { formatCount, formatDuration, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, fail } from "../state.svelte";

  const p = $derived(app.scanning?.progress);
  const loc = $derived(intlLocale());
</script>

<div class="backdrop" role="dialog" aria-modal="true" aria-labelledby="scan-title">
  <div class="panel card">
    <div class="spinner" aria-hidden="true"></div>
    <h2 id="scan-title" class="ellipsis">{t("scan-title", { path: app.scanning?.path ?? "" })}</h2>
    <div class="grid">
      <div><strong class="num">{formatCount(p?.files ?? 0, loc)}</strong><span class="muted">{t("stat-files")}</span></div>
      <div><strong class="num">{formatCount(p?.dirs ?? 0, loc)}</strong><span class="muted">{t("stat-dirs")}</span></div>
      <div><strong class="num">{formatSize(p?.onDisk ?? 0, loc)}</strong><span class="muted">{t("stat-size")}</span></div>
      <div><strong class="num">{formatDuration(p?.elapsedMs ?? 0, loc)}</strong><span class="muted">{t("stat-time")}</span></div>
    </div>
    {#if p?.errors}
      <p class="muted">{t("scan-errors", { count: formatCount(p.errors, loc) })}</p>
    {/if}
    <p class="hint muted">{t("scan-hint")}</p>
    <button class="btn" onclick={() => api.cancelScan().catch(fail)}>{t("scan-cancel")}</button>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    z-index: 40;
  }
  .panel {
    width: min(560px, 92vw);
    padding: 28px;
    text-align: center;
    box-shadow: var(--shadow);
  }
  h2 {
    font-size: 1.15em;
    margin: 14px 0 18px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    margin-bottom: 12px;
  }
  .grid div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    padding: 10px 6px;
  }
  .grid strong {
    font-size: 1.15em;
    text-align: center;
  }
  .grid span {
    font-size: 0.82em;
  }
  .hint {
    font-size: 0.85em;
  }
  .spinner {
    width: 40px;
    height: 40px;
    margin: 0 auto;
    border-radius: 50%;
    border: 4px solid var(--surface-2);
    border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
