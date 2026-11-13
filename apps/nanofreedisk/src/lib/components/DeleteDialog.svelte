<script lang="ts">
  // Confirmation for deleting hand-picked items (Analyze, Old & large, Tidy up), with
  // the shared confirmation and result (RemoveConfirm, RunResult). Where they go
  // comes from `askDelete` (the quarantine in Simple mode, see clean/destination.ts);
  // a big removal in Simple mode may choose the Recycle Bin instead. The backend
  // decides what is allowed (`plan_delete`); on confirm every item goes Planner →
  // SafetyGate (live re-check) → Executor → Cleanup history (`delete_entries`).
  import { api } from "../api";
  import { cleanApi } from "../clean/api";
  import { outcomesResult, type ShownMode } from "../clean/destination";
  import type { OperationResult } from "../clean/types";
  import { formatCount, formatPercent, formatSize, templateLabel } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { afterDelete, app, expert, fail, toast } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import RemoveConfirm from "./RemoveConfirm.svelte";
  import RunResult from "./RunResult.svelte";
  import type { DeleteItem, DeletePlan } from "../types";

  let { ids }: { ids: number[] } = $props();
  let plan = $state<DeletePlan | null>(null);
  let ack = $state(false);
  let busy = $state(false);
  let result = $state<OperationResult | null>(null);
  // where the items go: set by askDelete, the big-removal choice may change it
  let mode = $state<ShownMode>(app.deleteMode.mode);
  const loc = $derived(intlLocale());

  $effect(() => {
    api
      .planDelete(ids)
      .then((p) => (plan = p))
      .catch((e) => {
        fail(e);
        close();
      });
  });

  const allowed = $derived(plan?.items.filter((i) => i.allowed) ?? []);
  const blocked = $derived(plan?.items.filter((i) => !i.allowed) ?? []);
  const canGo = $derived(!!plan && allowed.length > 0 && (!plan.extraConfirm || ack) && !busy);

  function close() {
    if (!busy) app.deleting = null;
  }

  function reason(i: DeleteItem): string {
    return t(`reason-${i.reason ?? "unresolvable"}`, { template: templateLabel(i.template) });
  }

  async function go() {
    if (!canGo) return;
    busy = true;
    const dest = mode === "recycle-bin" ? "recycle-bin" : "quarantine";
    const since = Math.floor(Date.now() / 1000) - 2;
    try {
      const out = await cleanApi.deleteEntries(allowed.map((i) => i.id), dest, app.deleteMode.kind);
      for (const o of out) if (o.status === "failed" && o.message) toast(o.message, "error");
      // the operation this delete wrote to Cleanup history (for what the quarantine holds)
      const op = (await cleanApi.activity(5).catch(() => [])).find((r) => r.kind === app.deleteMode.kind && r.startedAt >= since);
      result = outcomesResult(op?.id ?? "", out, new Map(allowed.map((i) => [i.id, i.path])));
      mode = dest;
      await afterDelete(out.filter((o) => o.status === "done").map((o) => o.id));
    } catch (e) {
      fail(e);
      app.deleting = null;
    } finally {
      busy = false;
    }
  }
</script>

{#if result}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={(e) => e.key === "Escape" && close()}>
    <div class="dialog card" role="dialog" aria-modal="true" aria-label={t("run-title", { size: formatSize(result.freed, loc) })} data-delete-result>
      <RunResult {result} {mode} flat onhistory={close} ondismiss={close} />
      <footer><button class="btn cancel" onclick={close}>{t("clean-dismiss")}</button></footer>
    </div>
  </div>
{:else if plan}
  <RemoveConfirm
    title={mode === "quarantine" ? t("delete-title-quarantine") : t("delete-title")}
    bind:mode
    bytes={plan.onDisk}
    paths={allowed.map((i) => i.path)}
    choice={!expert()}
    note={mode === "recycle-bin" ? `${t("delete-quarantine-note")} ${t("delete-nuke-note")}` : t("delete-quarantine-note")}
    confirm={busy ? t("delete-working") : mode === "quarantine" ? t("delete-confirm-quarantine") : t("delete-confirm")}
    disabled={!canGo}
    {busy}
    testid="delete"
    onconfirm={go}
    oncancel={close}
  >
    {#if allowed.length}
      <ul class="items">
        {#each allowed as i (i.id)}
          <li>
            <Icon name={i.kind === "dir" ? "folder" : "file"} size={16} />
            <span class="ellipsis mono" title={i.path}>{i.path}</span>
            <span class="num">{formatSize(i.onDisk, loc)}</span>
          </li>
        {/each}
      </ul>
      <p class="total">
        <strong>{t("delete-total", { size: formatSize(plan.onDisk, loc), files: formatCount(plan.files, loc) })}</strong>
      </p>
    {:else}
      <p>{t("delete-nothing")}</p>
    {/if}

    {#if blocked.length}
      <h3><Icon name="shield" size={16} /> {t("delete-blocked-title")}</h3>
      <ul class="items blocked">
        {#each blocked as i (i.id)}
          <li>
            <span class="ellipsis mono" title={i.path}>{i.path}</span>
            <span class="why">{reason(i)}</span>
          </li>
        {/each}
      </ul>
    {/if}

    {#snippet extra()}
      {#if plan?.extraConfirm && allowed.length}
        <label class="extra">
          <input type="checkbox" bind:checked={ack} />
          <span>
            {t("delete-extra", {
              size: formatSize(plan.onDisk, loc),
              percent: formatPercent(app.summary?.volumeTotal ? plan.onDisk / app.summary.volumeTotal : 0, loc),
            })}
          </span>
        </label>
      {/if}
    {/snippet}
  </RemoveConfirm>
{:else}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={(e) => e.key === "Escape" && close()}>
    <div class="dialog card loading"><p class="muted">…</p><footer><button class="btn cancel" onclick={close}>{t("delete-cancel")}</button></footer></div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--bg) 65%, transparent);
    backdrop-filter: blur(2px);
  }
  .dialog {
    width: min(680px, 94vw);
    max-height: 86vh;
    overflow: auto;
    padding: 22px 24px;
    box-shadow: var(--shadow);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.95em;
    margin: 18px 0 6px;
    color: var(--risk-protected);
  }
  .items {
    list-style: none;
    margin: 8px 0;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-height: 210px;
    overflow: auto;
  }
  .items li {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 4px 6px;
  }
  .items.blocked li {
    grid-template-columns: 1fr;
    gap: 2px;
  }
  .why {
    font-size: 0.86em;
    color: var(--text-muted);
  }
  .total {
    margin: 8px 0 0;
  }
  .extra {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    margin-top: 14px;
    padding: 10px 12px;
    border: 1px solid var(--risk-protected);
    border-radius: var(--radius-sm);
    background: var(--danger-soft);
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
</style>
