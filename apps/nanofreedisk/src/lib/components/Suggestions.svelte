<script lang="ts">
  // Analyze → Recommendations: right after a scan, what can be freed and what must
  // stay, grouped the way a person thinks about it ("82 GB of AI agent worktrees in
  // NANOFreeDisk\.claude", "35 GB of Rust build output in 12 folders", "Windows —
  // keep"), each with what happens if it goes and what is not touched. Ticked groups
  // are cleaned through `advice_clean` (rule walk / quarantine / git / Recycle Bin);
  // 🟢 groups are ticked for the user, 🟡 need an explicit tick and an acknowledgement,
  // what must stay (conversation history, Windows…) never gets a checkbox.
  import { api } from "../api";
  import { SECTIONS, actionTarget, bi, blockedKey, sections, selItems, titleOf, type Section } from "../advice";
  import { advice } from "../advice/state.svelte";
  import { dev } from "../devcache/state.svelte";
  import { formatCount, formatSize } from "../format";
  import { i18n, intlLocale, t } from "../i18n.svelte";
  import { toggle } from "../selection";
  import { app, expert, fail, navigate, select, toast } from "../state.svelte";
  import type { AdviceGroup, AdviceGroupItem } from "../types";
  import Icon from "./Icon.svelte";
  import SelectionBar from "./SelectionBar.svelte";

  let el = $state<HTMLElement | null>(null);
  let open = $state<Record<string, boolean>>({});
  const loc = $derived(intlLocale());
  const lang = $derived(i18n.locale);
  const sum = $derived(advice.summary);
  const sugg = $derived(sum?.suggestions ?? null);
  const parts = $derived(sections(sugg?.groups ?? [], sum?.elsewhere ?? []));
  const items = $derived(selItems([...(sugg?.groups ?? []), ...(sum?.elsewhere ?? [])]));
  const waitingGit = $derived(!!sugg && !sugg.enriched && (sugg.groups ?? []).some((g) => g.blocked === "git-check"));
  const selectableKeys = $derived(new Set(items.filter((i) => !i.locked).map((i) => i.id)));
  const listed = $derived(new Set(items.map((i) => i.id)));

  /** Pill color of each section. */
  const TONE: Record<Section, string> = { safe: "safe", caution: "caution", guide: "guide", keep: "", elsewhere: "accent" };

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

  function pickedGroups(): AdviceGroup[] {
    const all = [...(sugg?.groups ?? []), ...(sum?.elsewhere ?? [])];
    return all.filter((g) => advice.picks.includes(g.key) && selectableKeys.has(g.key));
  }

  function confirmPicks() {
    const picked = pickedGroups();
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

  /** The ticked items as plain text (to paste in a note or a chat); deletes nothing. */
  async function copyList() {
    const lines = pickedGroups().map((g) => {
      const title = titleOf(g, lang);
      const where = g.location ?? t("advice-in-folders", { count: g.count });
      return `- ${t(title.key, title.args)} · ${formatSize(g.bytes, loc)} · ${where}`;
    });
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      toast(t("recs-copied", { count: lines.length }), "ok");
    } catch (e) {
      fail(e);
    }
  }
</script>

<section class="sugg panel" bind:this={el} data-suggestions>
  <header class="panel-head">
    <div class="headline">
      <h2 data-advice-headline>{t("recs-title")}</h2>
      <p>{sugg && sugg.reclaim <= 0 ? t("advice-headline-none") : t("recs-sub")}</p>
    </div>
    {#if sum?.computing && sugg}
      <span class="status muted small"><span class="spinner sm" aria-hidden="true"></span>{waitingGit ? t("advice-checking-git") : t("advice-computing")}</span>
    {/if}
  </header>
  {#if !sugg}
    <div class="wait muted">
      <div class="spinner" aria-hidden="true"></div>
      <p>{advice.summary?.computing || advice.loading ? t("advice-computing") : t("advice-not-ready")}</p>
    </div>
  {:else}
    <div class="body">
      {#each SECTIONS as sec (sec)}
        {@const gs = parts[sec as Section]}
        {#if gs.length}
          <div class="sec" data-section={sec}>
            <span class="tag {TONE[sec]}" title={t(`advice-section-${sec}-hint`)}>{t(`advice-section-${sec}`)}</span>
            <span class="sec-total">{formatSize(sectionBytes(gs), loc)}</span>
          </div>
          {#if sec === "caution" || sec === "guide" || sec === "keep"}
            <p class="hint">{t(`advice-section-${sec}-hint`)}</p>
          {/if}
          <ul class="groups">
            {#each gs as g (g.key)}
              {@const title = titleOf(g, lang)}
              {@const name = t(title.key, title.args)}
              {@const can = selectableKeys.has(g.key)}
              {@const isOpen = open[g.key] ?? false}
              <li class="group" class:open={isOpen} data-group={g.key} data-reason={g.reason}>
                <div class="line">
                  {#if listed.has(g.key)}
                    <input
                      type="checkbox"
                      data-pick={g.key}
                      checked={advice.picks.includes(g.key)}
                      disabled={!can || advice.running}
                      aria-label={name}
                      onchange={() => pick(g)}
                    />
                  {:else}
                    <span class="nocheck" title={g.hard ? t("advice-verdict-hint-protected") : t(`advice-verdict-hint-${g.verdict}`)}>
                      <Icon name={g.hard ? "lock" : g.verdict === "guide" ? "zoom" : "shield"} size={14} />
                    </span>
                  {/if}
                  <button class="what" onclick={() => (open = { ...open, [g.key]: !isOpen })} aria-expanded={isOpen} title={bi(g.impact, lang)}>
                    <span class="name">
                      <strong class="ellipsis">{name}</strong>
                      {#if g.hard}<span class="tag protected">{t("advice-verdict-protected")}</span>{/if}
                      {#if g.level === "advanced" && !expert()}<span class="tag">{t("advice-expert-only")}</span>{/if}
                    </span>
                    {#if g.location}
                      <span class="where mono ellipsis" title={g.location}>{g.location}</span>
                    {:else if g.count > 1}
                      <span class="where ellipsis">{t("advice-in-folders", { count: g.count })}</span>
                    {/if}
                  </button>
                  <span class="size">{formatSize(g.bytes, loc)}</span>
                  <button class="chev" class:on={isOpen} onclick={() => (open = { ...open, [g.key]: !isOpen })} aria-label={t("recs-details")} title={t("recs-details")}>
                    <Icon name="chevron" size={14} />
                  </button>
                </div>
                {#if g.blocked}
                  <p class="blocked"><Icon name="lock" size={12} />{t(blockedKey(g.blocked), { code: g.blocked })}</p>
                {/if}
                {#if isOpen}
                  <div class="more">
                    <p class="impact">{bi(g.impact, lang)}</p>
                    {#if g.untouched}
                      <p class="small"><b>{t("advice-untouched")}:</b> {bi(g.untouched, lang)}</p>
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
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
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
      compact
      sticky={false}
      action={{ label: (count) => t("recs-clean-n", { count }), icon: "clean", hint: t("advice-clean-hint"), testid: "advice", danger: false, run: confirmPicks }}
    >
      <button class="btn small" data-recs-copy disabled={!advice.picks.length || advice.running} onclick={copyList} title={t("recs-copy-hint")}>
        <Icon name="copy" size={14} />{t("recs-copy")}
      </button>
    </SelectionBar>
  {/if}
</section>

<style>
  .sugg {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding-bottom: 14px;
  }
  .panel-head {
    margin-bottom: 8px;
  }
  .wait {
    flex: 1;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    padding: 20px;
  }
  .status {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    white-space: nowrap;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0 -18px;
    padding: 0 18px 8px;
  }
  .sec {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    margin: 14px 0 4px;
  }
  .sec:first-child {
    margin-top: 4px;
  }
  .sec-total {
    font-size: 0.86em;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .hint {
    margin: 2px 0 4px;
    font-size: 0.8em;
    color: var(--text-muted);
  }
  .groups {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .group {
    border-bottom: 1px solid var(--border);
  }
  .group:last-child {
    border-bottom: none;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 9px 0;
  }
  .line input[type="checkbox"],
  .nocheck {
    flex: none;
    width: 16px;
    height: 16px;
    margin: 0;
  }
  .nocheck {
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }
  .what {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    border: none;
    background: none;
    padding: 0;
    text-align: left;
    color: var(--text);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .name strong {
    font-weight: 600;
  }
  .where {
    font-size: 0.8em;
    color: var(--text-muted);
    min-width: 0;
  }
  .size {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    white-space: nowrap;
  }
  .chev {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--text-muted);
    transition: transform 0.15s ease;
  }
  .chev:hover {
    background: var(--hover);
    color: var(--text);
  }
  .chev.on {
    transform: rotate(90deg);
  }
  .blocked {
    display: flex;
    gap: 5px;
    align-items: center;
    margin: -4px 0 8px 26px;
    font-size: 0.8em;
    color: var(--risk-caution-ink);
  }
  .more {
    margin: -2px 0 10px 26px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .more p {
    margin: 0;
  }
  .impact {
    font-size: 0.9em;
    color: var(--text-muted);
  }
  .items {
    list-style: none;
    margin: 0;
    padding: 4px 8px;
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
    color: var(--accent-ink);
    text-decoration: underline;
  }
  .acts {
    display: flex;
    gap: 6px;
  }
  .danger-text {
    color: var(--danger);
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
