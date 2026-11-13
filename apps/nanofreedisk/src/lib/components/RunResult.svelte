<script lang="ts">
  // The one result block of every removing tool: what was removed, how many items
  // went and how many stayed, what the quarantine still holds (freed for good in 7
  // days, or now from Cleanup history), what stayed grouped by why, and a link to
  // Cleanup history. Junk files passes `onaction` for its leftover buttons (retry,
  // administrator rights); the other tools group their problems by reason.
  import type { ShownMode } from "../clean/destination";
  import { cleanApi } from "../clean/api";
  import { ACTION_KEYS, leftoverAction, leftoverBody, leftoverGroups, leftoverTitle, type LeftoverAction } from "../clean/leftovers";
  import { reasonText } from "../clean/state.svelte";
  import type { OperationResult, Problem } from "../clean/types";
  import { formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, expert } from "../state.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    result: OperationResult;
    /** Where the run sent its items (the held line, before the journal answers). */
    mode: ShownMode;
    /** Leftover buttons (Junk files only). */
    onaction?: (a: LeftoverAction) => void;
    /** "See it in Cleanup history" was clicked (a dialog closes itself). */
    onhistory?: () => void;
    ondismiss: () => void;
    /** Inside a dialog: no card frame, no dismiss button. */
    flat?: boolean;
  }
  let { result, mode, onaction, onhistory, ondismiss, flat = false }: Props = $props();
  const loc = $derived(intlLocale());
  const SHOWN = 6;

  /** Bytes the quarantine holds of this run; the journal knows it exactly. */
  let journal = $state<{ held: number } | null>(null);
  $effect(() => {
    const id = result.id;
    journal = null;
    if (!id) return;
    let gone = false;
    cleanApi
      .activity(30)
      .then((rows) => {
        const row = rows.find((r) => r.id === id);
        if (!gone && row) journal = { held: row.held };
      })
      .catch(() => {});
    return () => {
      gone = true;
    };
  });
  const held = $derived(journal ? journal.held : mode === "quarantine" ? result.freed : 0);
  /** Asked for the quarantine, but some items went to the Recycle Bin (a drive
   *  without a quarantine). */
  const fallback = $derived(mode === "quarantine" && !!journal && journal.held + 1024 < result.freed);
  const stayed = $derived(result.partial + result.skipped + result.failed);

  const groups = $derived(leftoverGroups(result));
  /** Without the backend's grouping: problems grouped by reason. */
  const byReason = $derived.by(() => {
    if (groups.length) return [];
    const out = new Map<string, { label: string; items: Problem[] }>();
    for (const p of result.problems) {
      const key = p.reason ?? p.status;
      if (!out.has(key)) out.set(key, { label: p.reason ? reasonText(p.reason) : t(`clean-status-${p.status}`), items: [] });
      out.get(key)!.items.push(p);
    }
    return [...out.values()];
  });

  function history() {
    app.view = "activity";
    onhistory?.();
  }
</script>

<div class="run" class:card={!flat} class:bad={result.failed > 0} data-run-result={mode}>
  <Icon name={result.failed ? "alert" : "check"} size={22} />
  <div class="grow">
    <strong>{t("run-title", { size: formatSize(result.freed, loc) })}</strong>
    <p class="muted small">
      {#if expert()}
        {t("clean-done-detail", { done: result.done, partial: result.partial, skipped: result.skipped, failed: result.failed })}
      {:else}
        {t("run-counts", { done: result.done, stayed })}
      {/if}
      {#if result.cancelled}· {t("clean-was-cancelled")}{/if}
    </p>
    {#if held > 0}
      <p class="held small" data-run-held>{t("run-held", { size: formatSize(held, loc) })}</p>
    {/if}
    {#if fallback}
      <p class="muted small" data-run-fallback>{t("run-fallback")}</p>
    {/if}
    {#if groups.length}
      <div class="left" data-testid="clean-leftovers">
        <h3 class="small">{t("clean-left-title")}</h3>
        {#each groups as g (g.cause)}
          {@const title = leftoverTitle(g)}
          {@const action = onaction ? leftoverAction(g) : null}
          <div class="left-group {g.cause}" data-left-cause={g.cause}>
            <div class="row">
              <Icon name={g.cause === "in-use" ? "lock" : g.cause === "needs-admin" ? "shield" : "info"} size={16} />
              <strong class="grow">{t(title.key, title.args)}</strong>
              <span class="muted small num">{t("clean-left-size", { size: formatSize(g.bytes, loc) })}</span>
            </div>
            <p class="muted small">{t(leftoverBody(g))}</p>
            <ul class="left-items">
              {#each g.items.slice(0, SHOWN) as i (i.path)}
                <li class="small">
                  <span class="mono ellipsis" title={i.path}>{i.path}</span>
                  <span class="muted">{i.programs.length ? i.programs.join(", ") : i.reason ? reasonText(i.reason) : ""}</span>
                </li>
              {/each}
            </ul>
            {#if g.items.length > SHOWN}
              <p class="muted small">{t("clean-left-more", { count: g.items.length - SHOWN })}</p>
            {/if}
            {#if action && onaction}
              <button class="btn small" class:primary={action === "admin"} data-left-action={action} onclick={() => onaction(action)}>
                <Icon name={action === "admin" ? "shield" : "refresh"} size={14} />{t(ACTION_KEYS[action])}
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {:else if byReason.length}
      <div class="left" data-run-stayed>
        <h3 class="small">{t("run-stayed-title")}</h3>
        {#each byReason as g (g.label)}
          <div class="left-group other">
            <strong class="small">{g.label}</strong>
            <ul class="left-items">
              {#each g.items.slice(0, SHOWN) as p (p.path)}
                <li class="small"><span class="mono ellipsis" title={p.path}>{p.path}</span></li>
              {/each}
            </ul>
            {#if g.items.length > SHOWN}
              <p class="muted small">{t("clean-left-more", { count: g.items.length - SHOWN })}</p>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
  <div class="acts">
    <button class="btn" data-run-history onclick={history}><Icon name="undo" size={16} />{t("run-history")}</button>
    {#if !flat}
      <button class="btn ghost icon" onclick={ondismiss} aria-label={t("clean-dismiss")}><Icon name="x" size={16} /></button>
    {/if}
  </div>
</div>

<style>
  .run {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    flex-wrap: wrap;
  }
  .run.card {
    padding: 14px 16px;
    border-left: 4px solid var(--risk-safe);
  }
  .run.card.bad {
    border-left-color: var(--danger);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .run p {
    margin: 2px 0 0;
  }
  .held {
    color: var(--text);
  }
  .small {
    font-size: 0.86em;
  }
  .acts {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .left {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 10px;
  }
  .left h3 {
    margin: 0;
    font-weight: 600;
  }
  .left-group {
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    border-left: 3px solid var(--border);
  }
  .left-group.in-use {
    border-left-color: var(--risk-caution);
  }
  .left-group.needs-admin {
    border-left-color: var(--accent);
  }
  .left-group p {
    margin: 4px 0;
  }
  .left-items {
    margin: 4px 0 6px;
    padding: 0;
    list-style: none;
    max-height: 140px;
    overflow: auto;
  }
  .left-items li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
  }
</style>
