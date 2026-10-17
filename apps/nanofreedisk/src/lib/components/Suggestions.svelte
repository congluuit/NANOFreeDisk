<script lang="ts">
  // Analyze → Suggestions: right after a scan, what can be freed and what must stay,
  // grouped the way a person thinks about it ("82 GB of AI agent worktrees in
  // NANOFreeDisk\.claude", "35 GB of Rust build output in 12 folders", "Windows —
  // keep"), each with what happens if it goes and what is not touched. Ticked groups
  // are cleaned through `advice_clean` (rule walk / quarantine / git / Recycle Bin);
  // 🟢 groups are ticked for the user, 🟡 need an explicit tick and an acknowledgement.
  import { api } from "../api";
  import { SECTIONS, actionTarget, bi, blockedKey, sections, segments, selItems, titleOf, VERDICT_TOKEN, type Section } from "../advice";
  import { advice } from "../advice/state.svelte";
  import { dev } from "../devcache/state.svelte";
  import { formatCount, formatSize } from "../format";
  import { i18n, intlLocale, t } from "../i18n.svelte";
  import { toggle } from "../selection";
  import { app, expert, fail, navigate, select } from "../state.svelte";
  import type { AdviceGroup, AdviceGroupItem } from "../types";
  import Icon from "./Icon.svelte";
  import SelectionBar from "./SelectionBar.svelte";
  import VerdictBadge from "./VerdictBadge.svelte";

  let el = $state<HTMLElement | null>(null);
  let open = $state<Record<string, boolean>>({});
  const loc = $derived(intlLocale());
  const lang = $derived(i18n.locale);
  const sum = $derived(advice.summary);
  const sugg = $derived(sum?.suggestions ?? null);
  const parts = $derived(sections(sugg?.groups ?? [], sum?.elsewhere ?? []));
  const items = $derived(selItems([...(sugg?.groups ?? []), ...(sum?.elsewhere ?? [])]));
  const bar = $derived(segments(sugg?.totals));
  const total = $derived(bar.reduce((s, b) => s + b.bytes, 0));
  const elsewhereBytes = $derived(parts.elsewhere.reduce((s, g) => s + g.bytes, 0));
  const waitingGit = $derived(!!sugg && !sugg.enriched && (sugg.groups ?? []).some((g) => g.blocked === "git-check"));
  const selectableKeys = $derived(new Set(items.filter((i) => !i.locked).map((i) => i.id)));

  function sectionBytes(gs: AdviceGroup[]): number {
    return gs.reduce((s, g) => s + g.bytes, 0);
  }

  async function show(it: AdviceGroupItem) {
    if (it.id == null) return;
    try {
      const d = await api.detail(it.id);
      const parent = d.ancestors.length > 1 ? d.ancestors[d.ancestors.length - 2].id : (app.summary?.rootId ?? 0);
      navigate(parent);
      select(it.id);
    } catch (e) {
      fail(e);
    }
  }

  function follow(g: AdviceGroup) {
    const target = actionTarget(g.action);
    if (target.view === "devcache") {
      dev.tab = "stores";
      app.view = "devcache";
    } else if (target.view === "duplicates") {
      app.view = "duplicates";
    } else if (g.items[0]) {
      // guide: the details panel explains it (Folder Encyclopedia)
      void show(g.items[0]);
    }
  }

  function pick(g: AdviceGroup) {
    advice.picks = toggle(items, advice.picks, g.key, expert());
  }

  /** "Delete anyway" on a kept group: its listed folders. */
  function askOverride(g: AdviceGroup) {
    const title = titleOf(g, lang);
    const items = g.items.filter((i) => i.id != null);
    advice.override = {
      ids: items.map((i) => i.id as number),
      rows: items.map((i) => ({ key: String(i.id), verdict: g.verdict, label: t(title.key, title.args), location: i.path, bytes: i.bytes, impact: bi(g.impact, lang) })),
      why: bi(g.impact, lang),
      blocked: g.blocked,
      bytes: items.reduce((s, i) => s + i.bytes, 0),
    };
  }

  function confirmPicks() {
    const all = [...(sugg?.groups ?? []), ...(sum?.elsewhere ?? [])];
    const picked = all.filter((g) => advice.picks.includes(g.key) && selectableKeys.has(g.key));
    advice.confirm = {
      keys: picked.map((g) => g.key),
      rows: picked.map((g) => {
        const title = titleOf(g, lang);
        return { key: g.key, verdict: g.verdict, label: t(title.key, title.args), location: g.location, bytes: g.bytes, impact: bi(g.impact, lang) };
      }),
      items: items.filter((i) => picked.some((g) => g.key === i.id)),
      touched: picked.flatMap((g) => g.items.map((i) => i.id)).filter((id): id is number => id != null),
    };
  }
</script>

<section class="sugg" bind:this={el} data-suggestions>
  {#if !sugg}
    <div class="wait muted">
      <div class="spinner" aria-hidden="true"></div>
      <p>{advice.summary?.computing || advice.loading ? t("advice-computing") : t("advice-not-ready")}</p>
    </div>
  {:else}
    <header>
      <div class="headline">
        <h2 data-advice-headline>
          {sugg.reclaim > 0 ? t("advice-headline", { size: formatSize(sugg.reclaim, loc) }) : t("advice-headline-none")}
        </h2>
        <p class="muted small">{t("advice-headline-sub", { total: formatSize(total, loc) })}</p>
      </div>
      {#if sum?.computing}
        <span class="status muted small"><span class="spinner sm" aria-hidden="true"></span>{waitingGit ? t("advice-checking-git") : t("advice-computing")}</span>
      {/if}
    </header>
    {#if bar.length}
      <div class="bar" role="img" aria-label={bar.map((b) => `${t(`advice-verdict-${b.verdict}`)} ${formatSize(b.bytes, loc)}`).join(", ")}>
        {#each bar as b (b.verdict)}
          <span style="width: {Math.max(b.share * 100, 0.6)}%; background: var({VERDICT_TOKEN[b.verdict]})" title={`${t(`advice-verdict-${b.verdict}`)}: ${formatSize(b.bytes, loc)}`}></span>
        {/each}
      </div>
      <div class="legend small">
        {#each bar as b (b.verdict)}
          <span><i style="background: var({VERDICT_TOKEN[b.verdict]})"></i>{t(`advice-verdict-${b.verdict}`)} <b>{formatSize(b.bytes, loc)}</b></span>
        {/each}
      </div>
    {/if}
    <div class="body">
      {#each SECTIONS as sec (sec)}
        {@const gs = parts[sec as Section]}
        {#if gs.length}
          <h3 data-section={sec}>
            {t(`advice-section-${sec}`)}
            <span class="muted">{formatSize(sec === "elsewhere" ? elsewhereBytes : sectionBytes(gs), loc)}</span>
          </h3>
          <p class="hint muted small">{t(`advice-section-${sec}-hint`)}</p>
          {#each gs as g (g.key)}
            {@const title = titleOf(g, lang)}
            {@const can = selectableKeys.has(g.key)}
            {@const isOpen = open[g.key] ?? false}
            <article class="group {g.verdict}" data-group={g.key} data-reason={g.reason} style="--vc: var({VERDICT_TOKEN[g.verdict]})">
              <div class="row">
                {#if items.some((i) => i.id === g.key)}
                  <input
                    type="checkbox"
                    data-pick={g.key}
                    checked={advice.picks.includes(g.key)}
                    disabled={!can || advice.running}
                    aria-label={t(title.key, title.args)}
                    onchange={() => pick(g)}
                  />
                {:else}
                  <span class="nocheck"></span>
                {/if}
                <button class="title" onclick={() => (open = { ...open, [g.key]: !isOpen })} aria-expanded={isOpen}>
                  <VerdictBadge verdict={g.verdict} hard={g.hard} />
                  <strong class="ellipsis">{t(title.key, title.args)}</strong>
                  {#if g.location}<span class="loc mono muted ellipsis" title={g.location}>{g.location}</span>{/if}
                  {#if g.count > 1}<span class="muted small nowrap">{t("advice-in-folders", { count: g.count })}</span>{/if}
                  {#if g.level === "advanced" && !expert()}<span class="tag small">{t("advice-expert-only")}</span>{/if}
                </button>
                <span class="size">{formatSize(g.bytes, loc)}</span>
              </div>
              <p class="impact">{bi(g.impact, lang)}</p>
              {#if g.blocked}
                <p class="blocked small"><Icon name="lock" size={13} />{t(blockedKey(g.blocked), { code: g.blocked })}</p>
              {/if}
              {#if isOpen}
                {#if g.untouched}
                  <p class="small untouched"><b>{t("advice-untouched")}:</b> {bi(g.untouched, lang)}</p>
                {/if}
                {#if g.recent}<p class="muted small">{t("advice-recent", { size: formatSize(g.recent, loc) })}</p>{/if}
                {#if g.requiresClosed.length}<p class="muted small">{t("advice-close-first", { apps: g.requiresClosed.join(", ") })}</p>{/if}
                {#if expert() && g.source}<p class="muted small mono">{g.source}{g.store ? ` · ${g.store}` : ""}</p>{/if}
                <ul class="items">
                  {#each g.items as it (it.path)}
                    <li>
                      <button class="mono ellipsis link" disabled={it.id == null} title={it.path} onclick={() => show(it)}>{it.path || "."}</button>
                      {#if it.blocked}<span class="muted small" title={t(blockedKey(it.blocked), { code: it.blocked })}><Icon name="lock" size={12} /></span>{/if}
                      <span class="num muted small">{formatSize(it.bytes, loc)}</span>
                    </li>
                  {/each}
                </ul>
                {#if g.count > g.items.length}<p class="muted small">{t("advice-items-more", { count: formatCount(g.count - g.items.length, loc) })}</p>{/if}
              {/if}
              {#if g.action === "store" || g.action === "duplicates" || g.action === "guide" || g.overridable}
                <div class="acts">
                  {#if g.action === "store" || g.action === "duplicates" || g.action === "guide"}
                  <button class="btn small" onclick={() => follow(g)} title={t(`advice-action-hint-${g.action}`)}>
                    <Icon name={g.action === "guide" ? "zoom" : g.action === "duplicates" ? "duplicates" : "devcache"} size={14} />{t(`advice-action-${g.action}`)}
                  </button>
                  {/if}
                  {#if g.overridable}
                    <button class="btn small ghost danger-text" data-override={g.key} onclick={() => askOverride(g)} title={t("advice-override-hint")}>
                      <Icon name="trash" size={14} />{t("advice-override")}
                    </button>
                  {/if}
                </div>
              {/if}
            </article>
          {/each}
        {/if}
      {/each}
    </div>
    <SelectionBar
      {items}
      picked={advice.picks}
      onchange={(ids) => (advice.picks = ids)}
      expert={expert()}
      busy={advice.running}
      scope={el}
      nested
      sticky={false}
      action={{ label: (count, size) => t("advice-clean-n", { count, size }), icon: "clean", hint: t("advice-clean-hint"), testid: "advice", run: confirmPicks }}
    />
  {/if}
</section>

<style>
  .sugg {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .wait {
    flex: 1;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    padding: 20px;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px 6px;
  }
  h2 {
    margin: 0;
    font-size: 1.25em;
  }
  .headline p {
    margin: 2px 0 0;
  }
  .status {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    white-space: nowrap;
  }
  .bar {
    display: flex;
    height: 10px;
    margin: 4px 14px;
    border-radius: 5px;
    overflow: hidden;
    background: var(--surface-2);
    gap: 1px;
  }
  .bar span {
    height: 100%;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 14px;
    padding: 2px 14px 6px;
    color: var(--text-muted);
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .legend b {
    color: var(--text);
    font-weight: 600;
  }
  .legend i {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    display: inline-block;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 10px 8px;
  }
  h3 {
    display: flex;
    gap: 8px;
    align-items: baseline;
    margin: 12px 4px 0;
    font-size: 0.98em;
  }
  .hint {
    margin: 2px 4px 6px;
  }
  .group {
    border: 1px solid var(--border);
    border-left: 3px solid var(--vc);
    border-radius: var(--radius-sm);
    padding: 6px 10px 6px 8px;
    margin: 0 0 6px;
    background: var(--surface);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .nocheck {
    width: 13px;
    flex: none;
  }
  .title {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    border: none;
    background: none;
    padding: 2px 0;
    text-align: left;
    color: var(--text);
  }
  .loc {
    font-size: 0.84em;
    min-width: 0;
  }
  .nowrap {
    white-space: nowrap;
  }
  .size {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    white-space: nowrap;
  }
  .impact {
    margin: 2px 0 0 21px;
    font-size: 0.9em;
    color: var(--text-muted);
  }
  .blocked {
    display: flex;
    gap: 5px;
    align-items: center;
    margin: 4px 0 0 21px;
    color: var(--risk-caution);
  }
  .untouched {
    margin: 6px 0 0 21px;
  }
  .items {
    list-style: none;
    margin: 6px 0 0 21px;
    padding: 4px 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-height: 180px;
    overflow: auto;
  }
  .items li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 1px 0;
  }
  .items .link {
    flex: 1;
    min-width: 0;
    text-align: left;
    border: none;
    background: none;
    padding: 0;
    color: var(--text);
    font-size: 0.86em;
  }
  .items .link:hover:not(:disabled) {
    color: var(--accent);
    text-decoration: underline;
  }
  .acts {
    display: flex;
    gap: 6px;
    margin: 6px 0 0 21px;
  }
  .danger-text {
    color: var(--danger);
  }
  .tag {
    padding: 0 6px;
    border-radius: 8px;
    border: 1px solid var(--border);
    color: var(--text-muted);
    white-space: nowrap;
  }
  .small {
    font-size: 0.84em;
  }
  .spinner {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 3px solid var(--border);
    border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }
  .spinner.sm {
    width: 12px;
    height: 12px;
    border-width: 2px;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
