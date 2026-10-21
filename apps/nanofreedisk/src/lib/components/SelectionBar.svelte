<script lang="ts">
  // The one selection bar of every list the user ticks before acting: always visible
  // (sticky at the bottom of the view), "N selected · M files · X will be freed"
  // live, what is likely skipped counted apart, Select all / Clear / only one risk
  // level, and the main action with the exact total. Ctrl+A selects everything in
  // its scope, Esc clears. Ticking never bypasses Planner → SafetyGate → Executor.
  import type { Snippet } from "svelte";
  import { formatCount, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { keyForScope, levelsPresent, sanitize, selectAll, selectLevel, summarize, type PickLevel, type SelItem } from "../selection";
  import Icon from "./Icon.svelte";

  interface Props {
    /** Items the buttons and Ctrl+A act on. */
    items: SelItem[];
    /** Items the totals count (default `items`; e.g. every list of Old & large). */
    all?: SelItem[];
    picked: string[];
    onchange: (ids: string[]) => void;
    expert: boolean;
    /** An operation is running: everything is disabled. */
    busy?: boolean;
    /** The element whose keys (Ctrl+A, Esc) this bar handles; it gets
     *  `data-selection-scope`. */
    scope?: HTMLElement | null;
    /** Also handle keys pressed with nothing focused (the view's main list). */
    primary?: boolean;
    /** Show the files count (lists whose rows know it). */
    files?: boolean;
    /** The main action: `label(count, size)`; disabled with nothing selected. */
    action?: { label: (count: number, size: string) => string; icon?: string; hint?: string; danger?: boolean; testid?: string; run: () => void };
    /** Hide the "only <level>" buttons (lists without risk levels). */
    noLevels?: boolean;
    /** Stick to the bottom of a scrolling view (off where the layout already keeps
     *  it at the bottom, or for a bar nested in a card). */
    sticky?: boolean;
    /** Nested in a card: flat look. */
    nested?: boolean;
    /** More buttons next to the action (Duplicates: links, quarantine). */
    children?: Snippet;
    /** Footer of a panel: "X selected · N items", only "Select safe" / Clear. */
    compact?: boolean;
    /** With nothing selected, show only this hint (no buttons). */
    idle?: string;
  }
  let { items, all, picked, onchange, expert, busy = false, scope = null, primary = false, files = true, action, noLevels = false, sticky = true, nested = false, children, compact = false, idle }: Props = $props();

  const loc = $derived(intlLocale());
  const counted = $derived(all ?? items);
  const s = $derived(summarize(counted, picked, expert));
  const levels = $derived(noLevels ? [] : levelsPresent(items, expert));
  const anySelectable = $derived(selectAll(items, expert).length > 0);

  $effect(() => {
    scope?.setAttribute("data-selection-scope", "");
  });

  function only(level: PickLevel) {
    onchange(selectLevel(items, level, expert));
  }
  function everything() {
    // keep what is picked elsewhere (other lists counted in `all`)
    onchange(sanitize(counted, [...picked, ...selectAll(items, expert)], expert));
  }
  function clear() {
    const mine = new Set(items.map((i) => i.id));
    onchange(picked.filter((id) => !mine.has(id)));
  }
  function onkey(e: KeyboardEvent) {
    if (busy || !keyForScope(e.target, scope, primary, document)) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "a") {
      e.preventDefault();
      everything();
    } else if (e.key === "Escape" && picked.length) {
      e.preventDefault();
      clear();
    }
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="selbar" class:card={!nested && !compact} class:sticky class:nested class:compact data-selection-bar aria-live="polite">
  <div class="sum">
    {#if s.count && compact}
      <span data-sel-summary><strong>{t("sel-compact", { size: formatSize(s.bytes, loc) })}</strong> <span class="muted">· {t("sel-compact-items", { count: s.count })}</span></span>
    {:else if s.count}
      <strong data-sel-summary>
        {files
          ? t("sel-summary", { count: formatCount(s.count, loc), files: formatCount(s.files, loc), size: formatSize(s.bytes, loc) })
          : t("sel-summary-nofiles", { count: formatCount(s.count, loc), size: formatSize(s.bytes, loc) })}
      </strong>
      {#if s.skipCount}
        <span class="skip small" data-sel-skip><Icon name="alert" size={14} />{t("sel-skip", { count: s.skipCount, apps: s.skipReasons.join(", "), size: formatSize(s.skipBytes, loc) })}</span>
      {/if}
    {:else}
      <span class="muted" data-sel-summary>{idle ?? (compact ? t("sel-none-short") : t("sel-none"))}</span>
    {/if}
  </div>
  <div class="row" hidden={!!idle && !s.count}>
    {#if compact}
      <div class="picks" role="group" aria-label={t("sel-group-label")}>
        {#if picked.length}
          <button class="btn ghost small" data-sel="none" disabled={busy} onclick={clear} title={t("sel-keys-hint")}>{t("sel-clear-short")}</button>
        {/if}
        {#if levels.includes("safe")}
          <button class="btn small" data-sel="safe" disabled={busy} onclick={() => only("safe")} title={t("sel-only-hint")}>{t("sel-select-safe")}</button>
        {:else}
          <button class="btn small" data-sel="all" disabled={busy || !anySelectable} onclick={everything} title={t("sel-keys-hint")}>{t("sel-all")}</button>
        {/if}
      </div>
    {:else}
      <div class="picks" role="group" aria-label={t("sel-group-label")}>
        <button class="btn ghost small" data-sel="all" disabled={busy || !anySelectable} onclick={everything} title={t("sel-keys-hint")}>{t("sel-all")}</button>
        <button class="btn ghost small" data-sel="none" disabled={busy || !picked.length} onclick={clear} title={t("sel-keys-hint")}>{t("sel-clear")}</button>
        {#each levels as l (l)}
          <button class="btn ghost small lvl" data-sel={l} disabled={busy} onclick={() => only(l)} title={t("sel-only-hint")}>
            <span class="dot {l}"></span>{t("sel-only", { level: t(`clean-level-${l}`) })}
          </button>
        {/each}
      </div>
    {/if}
    <span class="grow"></span>
    {@render children?.()}
    {#if action}
      <button
        class="btn {action.danger === false ? 'primary' : 'danger'}"
        data-sel-action={action.testid ?? ""}
        disabled={busy || !s.count}
        onclick={action.run}
        data-effect
        title={s.count ? action.hint : t("sel-none")}
      >
        <Icon name={action.icon ?? "clean"} size={16} />{action.label(s.count, formatSize(s.bytes, loc))}
      </button>
    {/if}
  </div>
</div>

<style>
  .selbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 16px;
    padding: 12px 16px;
  }
  .selbar.card {
    box-shadow: var(--shadow);
  }
  /* the scrolling views have a 40px bottom padding, which sticky positioning keeps
     clear: sit on the very bottom edge instead */
  .selbar.sticky {
    position: sticky;
    bottom: -40px;
    z-index: 5;
  }
  .selbar.nested,
  .selbar.compact {
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
  .selbar.compact {
    padding: 12px 0 0;
  }
  .sum {
    flex: 1 1 260px;
    display: flex;
    gap: 4px 14px;
    align-items: baseline;
    flex-wrap: wrap;
    min-width: 0;
  }
  .compact .sum {
    flex-basis: 150px;
  }
  .row[hidden] {
    display: none;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
    margin-left: auto;
  }
  .picks {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .grow {
    display: none;
  }
  .skip {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    color: var(--risk-caution-ink);
  }
  .lvl {
    gap: 6px;
  }
</style>
