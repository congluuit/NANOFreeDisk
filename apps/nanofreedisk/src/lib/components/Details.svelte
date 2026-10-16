<script lang="ts">
  import { api } from "../api";
  import { formatCount, formatDate, formatPercent, formatSize } from "../format";
  import { i18n, intlLocale, t } from "../i18n.svelte";
  import { renderArticle } from "../md";
  import { app, askDelete, expert, fail, navigate, toast } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import type { Article, Detail } from "../types";

  let detail = $state<Detail | null>(null);
  let article = $state<Article | null>(null);
  let showArticle = $state(true);
  let owner = $state<string | null>(null);

  // A9: owner account of the focused entry (read from its security descriptor)
  $effect(() => {
    const id = app.focus;
    owner = null;
    // technical detail: Expert mode only
    if (id == null || !expert()) return;
    let cancelled = false;
    api
      .owner(id)
      .then((o) => !cancelled && (owner = o))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // Folder Encyclopedia (A5): "What is this?" for the focused entry or its parent
  $effect(() => {
    const id = app.focus;
    const lang = i18n.locale;
    void app.version;
    if (id == null) {
      article = null;
      return;
    }
    let cancelled = false;
    api
      .explain(id, lang)
      .then((a) => !cancelled && (article = a))
      .catch(() => !cancelled && (article = null));
    return () => {
      cancelled = true;
    };
  });
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
    {#if owner}
      <p class="owner muted small" title={t("details-owner-hint")}>{t("details-owner")}: <span class="mono">{owner}</span></p>
    {/if}
    {#if article}
      <div class="article risk-{article.risk}">
        <button class="ahead" onclick={() => (showArticle = !showArticle)} aria-expanded={showArticle}>
          <Icon name="shield" size={16} />
          <span class="ellipsis">
            <span class="muted">{article.inside ? t("encyclopedia-inside") : t("encyclopedia-title")}</span>
            <strong>{article.title}</strong>
          </span>
          <span class="risk">{t(`risk-${article.risk}`)}</span>
        </button>
        {#if showArticle}
          <!-- escaped by renderArticle (md.ts) -->
          <div class="abody">{@html renderArticle(article.body)}</div>
          {#if article.inside}<p class="muted small mono ellipsis" title={article.path}>{article.path}</p>{/if}
          <p class="muted small">{t("encyclopedia-note")}</p>
        {/if}
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
        data-effect
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
  .article {
    margin: 8px 0;
    border: 1px solid var(--border);
    border-left: 4px solid var(--risk-safe);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    padding: 6px 10px;
    max-height: 40vh;
    overflow: auto;
  }
  .article.risk-caution {
    border-left-color: var(--risk-caution);
  }
  .article.risk-advanced {
    border-left-color: var(--risk-advanced);
  }
  .article.risk-protected {
    border-left-color: var(--risk-protected);
  }
  .ahead {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: none;
    background: none;
    padding: 2px 0;
    text-align: left;
  }
  .ahead > span.ellipsis {
    flex: 1;
    display: flex;
    gap: 6px;
  }
  .risk {
    font-size: 0.78em;
    padding: 0 6px;
    border-radius: 4px;
    border: 1px solid var(--border);
    white-space: nowrap;
  }
  .abody :global(p) {
    margin: 6px 0;
    font-size: 0.92em;
  }
  .abody :global(code) {
    font-family: "Cascadia Mono", Consolas, ui-monospace, monospace;
    font-size: 0.9em;
    padding: 0 3px;
    border-radius: 3px;
    background: var(--surface);
  }
  .owner {
    margin: 4px 0 0;
  }
</style>
