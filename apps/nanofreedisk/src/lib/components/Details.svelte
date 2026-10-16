<script lang="ts">
  import { api } from "../api";
  import { formatCount, formatDate, formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, askDelete, expert, fail, navigate, toast } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import type { Detail } from "../types";

  let detail = $state<Detail | null>(null);
  const loc = $derived(intlLocale());
  const rootId = $derived(app.summary?.rootId ?? 0);
  const targets = $derived(app.selected.length ? app.selected : detail && detail.id !== rootId ? [detail.id] : []);

  $effect(() => {
    const id = app.focus;
    void app.version;
    if (id == null) {
      detail = null;
      return;
    }
    let cancelled = false;
    api
      .detail(id)
      .then((d) => !cancelled && (detail = d))
      .catch(() => !cancelled && (detail = null));
    return () => {
      cancelled = true;
    };
  });

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast(t("copied"), "ok");
    } catch (e) {
      fail(e);
    }
  }
</script>

<section class="details">
  {#if !detail}
    <p class="muted">{t("details-none")}</p>
  {:else}
    <div class="title">
      <span class="ic {detail.kind}"><Icon name={detail.kind === "dir" ? "folder" : detail.kind === "file" ? "file" : "link"} size={22} /></span>
      <div class="ellipsis">
        <strong class="ellipsis" title={detail.name}>{detail.name}</strong>
        <div class="muted small">
          {t(`kind-${detail.kind}`)} · {formatSize(detail.onDisk, loc)}
          {#if detail.logical !== detail.onDisk}<span title={t("col-logical")}> ({formatSize(detail.logical, loc)})</span>{/if}
          {#if detail.kind === "dir"} · {t("details-contains")} {formatCount(detail.files, loc)} {t("stat-files").toLowerCase()}{/if}
          · {formatPercent(detail.share, loc)} · {formatDate(detail.mtime, loc)}
        </div>
      </div>
    </div>
    <div class="path">
      <span class="mono ellipsis" title={detail.path}>{detail.path}</span>
      <button class="btn ghost icon" title={t("action-copy-path")} onclick={() => copy(detail!.path)}>
        <Icon name="copy" size={15} />
      </button>
    </div>
    {#if expert() && (detail.attrs.length || detail.hardlinkDup)}
      <div class="chips">
        {#each detail.attrs as a (a)}<span class="chip">{t(`attr-${a}`)}</span>{/each}
        {#if detail.hardlinkDup}<span class="chip">{t("attr-hardlink")}</span>{/if}
      </div>
    {/if}
    <div class="actions">
      {#if detail.kind === "dir" && detail.id !== app.current}
        <button class="btn" onclick={() => navigate(detail!.id)}><Icon name="zoom" size={16} />{t("action-zoom")}</button>
      {/if}
      <button class="btn" onclick={() => api.reveal(detail!.id).catch(fail)}><Icon name="reveal" size={16} />{t("action-reveal")}</button>
      {#if detail.kind === "dir" || detail.kind === "file"}
        <button class="btn" onclick={() => api.openItem(detail!.id).catch(fail)}><Icon name="open" size={16} />{t("action-open")}</button>
      {/if}
      <span class="grow"></span>
      <button
        class="btn danger-outline"
        disabled={!targets.length}
        title={t("action-recycle-hint")}
        onclick={() => askDelete(targets)}
      >
        <Icon name="trash" size={16} />
        {targets.length > 1 ? t("action-recycle-n", { count: targets.length }) : t("action-recycle")}
      </button>
    </div>
    {#if app.selected.length < 2}
      <p class="hint muted">{t("action-select-hint")}</p>
    {/if}
  {/if}
</section>

<style>
  .details {
    padding: 12px 14px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 150px;
  }
  .title {
    display: flex;
    gap: 10px;
    align-items: center;
    min-width: 0;
  }
  .ic {
    color: var(--text-muted);
  }
  .ic.dir {
    color: var(--risk-caution);
  }
  .small {
    font-size: 0.86em;
  }
  .path {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    padding: 2px 4px 2px 8px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .path .mono {
    flex: 1;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    font-size: 0.8em;
    padding: 1px 8px;
    border-radius: 10px;
    border: 1px solid var(--border);
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .grow {
    flex: 1;
  }
  .danger-outline {
    border-color: var(--danger);
    color: var(--danger);
    background: transparent;
  }
  .danger-outline:hover:not(:disabled) {
    background: var(--danger-soft);
    border-color: var(--danger);
  }
  .hint {
    margin: 0;
    font-size: 0.8em;
  }
</style>
