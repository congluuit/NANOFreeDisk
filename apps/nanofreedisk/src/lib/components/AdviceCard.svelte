<script lang="ts">
  // Details panel: "Can I delete this?" for the focused entry — its verdict, what
  // happens if it goes, what is not touched, what it holds by verdict and the right
  // action (clean by rule / quarantine / git, the official command, the guide, or the
  // Recycle Bin for what nothing knows about). Kept items get no delete button.
  import { api } from "../api";
  import { actionTarget, bi, blockedKey, segments, titleOf, VERDICT_TOKEN } from "../advice";
  import { advice } from "../advice/state.svelte";
  import { dev } from "../devcache/state.svelte";
  import { formatSize } from "../format";
  import { i18n, intlLocale, t } from "../i18n.svelte";
  import { app, askDelete, expert, navigate, select } from "../state.svelte";
  import type { Detail, EntryAdvice } from "../types";
  import Icon from "./Icon.svelte";
  import VerdictBadge from "./VerdictBadge.svelte";

  let { detail }: { detail: Detail } = $props();
  let info = $state<EntryAdvice | null>(null);
  let loaded = $state(false);
  const loc = $derived(intlLocale());
  const lang = $derived(i18n.locale);

  $effect(() => {
    const id = detail.id;
    void app.version;
    let cancelled = false;
    loaded = false;
    api
      .adviceDetail(id)
      .then((a) => {
        if (cancelled) return;
        info = a;
        loaded = true;
      })
      .catch(() => {
        if (cancelled) return;
        info = null;
        loaded = true;
      });
    return () => {
      cancelled = true;
    };
  });

  const unit = $derived(info?.unit ?? null);
  const isRoot = $derived(!!unit && !info?.unitPath);
  const verdict = $derived(unit?.verdict ?? null);
  const bar = $derived(segments(info?.node.inside));
  const reclaim = $derived(info?.node.reclaim ?? 0);
  const rootId = $derived(app.summary?.rootId ?? 0);
  const title = $derived(unit ? titleOf(unit, lang) : null);
  const runnable = $derived(!!unit && (unit.action === "clean" || unit.action === "quarantine" || unit.action === "ai-leftovers"));
  const actionable = $derived(!!unit && !unit.blocked && (unit.verdict === "safe" || unit.verdict === "caution") && (unit.level !== "advanced" || expert()));

  /** Ask to clean this unit (`entry|id`) or everything inside this folder (`within|id`). */
  function clean(scope: "entry" | "within") {
    if (!info) return;
    const key = `${scope}|${detail.id}`;
    const bytes = scope === "entry" ? (info.node.inside.safe + info.node.inside.caution) : reclaim;
    const level = scope === "entry" && unit ? (unit.level === "advanced" ? "advanced" : unit.verdict === "safe" ? "safe" : "caution") : info.node.inside.caution > 0 ? "caution" : "safe";
    advice.confirm = {
      keys: [key],
      rows: [
        {
          key,
          verdict: scope === "entry" && unit ? unit.verdict : info.node.inside.caution > 0 ? "caution" : "safe",
          label: scope === "entry" && title ? t(title.key, title.args) : detail.name,
          location: detail.path,
          bytes,
          impact: unit ? bi(unit.impact, lang) : "",
        },
      ],
      items: [{ id: key, bytes, files: detail.files, level }],
      touched: [detail.id],
    };
  }

  function follow() {
    if (!unit) return;
    const target = actionTarget(unit.action);
    if (target.view === "devcache") {
      dev.tab = "stores";
      app.view = "devcache";
    } else if (target.view === "duplicates") {
      app.view = "duplicates";
    }
  }

  async function openUnit() {
    if (!unit) return;
    try {
      const d = await api.detail(unit.entry);
      const parent = d.ancestors.length > 1 ? d.ancestors[d.ancestors.length - 2].id : rootId;
      navigate(parent);
      select(unit.entry);
    } catch {
      /* gone meanwhile */
    }
  }
</script>

{#if !loaded}
  <p class="muted small">{t("advice-details-loading")}</p>
{:else if info}
  <section class="advice" data-advice-card data-verdict={verdict ?? "mixed"} style="--vc: var({VERDICT_TOKEN[verdict ?? 'unknown']})">
    <div class="head">
      <span class="label muted small">{t("advice-details-title")}</span>
      {#if verdict}<VerdictBadge {verdict} long />{/if}
      {#if title}<strong class="ellipsis">{t(title.key, title.args)}</strong>{/if}
    </div>
    {#if unit}
      {#if info.unitPath}
        <p class="small muted">
          {t("advice-details-part-of", { path: info.unitPath })}
          <button class="link" onclick={openUnit}>{t("advice-action-show")}</button>
        </p>
      {/if}
      <p class="impact"><b>{unit.verdict === "keep" ? t("advice-why-keep") : t("advice-if-delete")}:</b> {bi(unit.impact, lang)}</p>
      {#if unit.untouched}<p class="small"><b>{t("advice-untouched")}:</b> {bi(unit.untouched, lang)}</p>{/if}
      {#if unit.blocked}<p class="blocked small"><Icon name="lock" size={13} />{t(blockedKey(unit.blocked), { code: unit.blocked })}</p>{/if}
      {#if unit.requiresClosed.length}<p class="muted small">{t("advice-close-first", { apps: unit.requiresClosed.join(", ") })}</p>{/if}
      {#if unit.store}<p class="muted small">{t("advice-store-better", { id: unit.store })}</p>{/if}
      {#if expert() && unit.source}<p class="muted small mono">{unit.reason} · {unit.source}</p>{/if}
    {:else if reclaim > 0}
      <p class="small">{t("advice-details-mixed", { free: formatSize(reclaim, loc) })}</p>
    {:else if detail.kind === "dir" && info.parts.length}
      <p class="small muted">{t("advice-details-nothing")}</p>
    {:else}
      <p class="small muted">{t("advice-details-unknown")}</p>
    {/if}
    {#if detail.kind === "dir" && bar.length > 1}
      <div class="inside">
        <span class="muted small">{t("advice-details-inside")}</span>
        <div class="bar">
          {#each bar as b (b.verdict)}
            <span style="width: {Math.max(b.share * 100, 0.8)}%; background: var({VERDICT_TOKEN[b.verdict]})" title={`${t(`advice-verdict-${b.verdict}`)}: ${formatSize(b.bytes, loc)}`}></span>
          {/each}
        </div>
        <div class="legend small">
          {#each bar as b (b.verdict)}
            <span><i style="background: var({VERDICT_TOKEN[b.verdict]})"></i>{t(`advice-badge-${b.verdict}`)} {formatSize(b.bytes, loc)}</span>
          {/each}
        </div>
      </div>
    {/if}
    {#if info.parts.length && detail.kind === "dir"}
      <div class="parts">
        <span class="muted small">{t("advice-details-parts")}</span>
        <ul>
          {#each info.parts as p (p.id)}
            {@const pt = titleOf(p, lang)}
            <li>
              <VerdictBadge verdict={p.verdict} compact />
              <button class="link ellipsis" title={p.path} onclick={() => { select(p.id); }}>{t(pt.key, pt.args)} <span class="muted mono">{p.path}</span></button>
              <span class="num small">{formatSize(p.bytes, loc)}</span>
            </li>
          {/each}
        </ul>
      </div>
    {/if}
    <div class="acts">
      {#if unit && isRoot && runnable && actionable}
        <button class="btn primary" data-advice-action={unit.action} onclick={() => clean("entry")} title={t(`advice-action-hint-${unit.action}`)}>
          <Icon name="clean" size={16} />{t(`advice-action-${unit.action}`)} ({formatSize(info.node.inside.safe + info.node.inside.caution, loc)})
        </button>
      {:else if !unit && reclaim > 0 && detail.kind === "dir"}
        <button class="btn primary" data-advice-action="within" onclick={() => clean("within")} title={t("advice-clean-hint")}>
          <Icon name="clean" size={16} />{t("advice-free-here", { size: formatSize(reclaim, loc) })}
        </button>
      {/if}
      {#if unit && (unit.action === "store" || unit.action === "duplicates")}
        <button class="btn" onclick={follow} title={t(`advice-action-hint-${unit.action}`)}><Icon name={unit.action === "store" ? "devcache" : "duplicates"} size={16} />{t(`advice-action-${unit.action}`)}</button>
      {/if}
      {#if unit && unit.action === "recycle-bin" && isRoot && actionable}
        <button class="btn danger-outline" data-advice-action="recycle-bin" onclick={() => askDelete([detail.id])} title={t("advice-action-hint-recycle-bin")}><Icon name="trash" size={16} />{t("advice-action-recycle-bin")}</button>
      {/if}
    </div>
  </section>
{/if}

<style>
  .advice {
    border: 1px solid var(--border);
    border-left: 4px solid var(--vc);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 32vh;
    overflow: auto;
  }
  .head {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 0;
  }
  .advice p {
    margin: 0;
  }
  .impact {
    font-size: 0.92em;
  }
  .small {
    font-size: 0.85em;
  }
  .blocked {
    display: flex;
    gap: 5px;
    align-items: center;
    color: var(--risk-caution);
  }
  .link {
    border: none;
    background: none;
    padding: 0 2px;
    color: var(--accent);
    text-align: left;
  }
  .inside .bar {
    display: flex;
    height: 8px;
    border-radius: 4px;
    overflow: hidden;
    gap: 1px;
    margin: 3px 0;
    background: var(--surface);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
    color: var(--text-muted);
  }
  .legend span {
    display: inline-flex;
    gap: 4px;
    align-items: center;
  }
  .legend i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    display: inline-block;
  }
  .parts ul {
    list-style: none;
    margin: 3px 0 0;
    padding: 0;
  }
  .parts li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 1px 0;
    min-width: 0;
  }
  .parts .link {
    flex: 1;
    min-width: 0;
    color: var(--text);
    font-size: 0.88em;
  }
  .parts .link:hover {
    color: var(--accent);
  }
  .num {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .danger-outline {
    border-color: var(--danger);
    color: var(--danger);
    background: transparent;
  }
</style>
