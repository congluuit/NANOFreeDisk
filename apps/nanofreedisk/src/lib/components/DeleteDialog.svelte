<script lang="ts">
  // Confirmation for "Move to Recycle Bin". The backend decides what is allowed
  // (`plan_delete`) and re-checks every item against the live filesystem when the
  // user confirms (`recycle`).
  import { api } from "../api";
  import { formatCount, formatPercent, formatSize, templateLabel } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { afterDelete, app, fail, toast } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import type { DeleteItem, DeletePlan } from "../types";

  let { ids }: { ids: number[] } = $props();
  let plan = $state<DeletePlan | null>(null);
  let ack = $state(false);
  let busy = $state(false);
  let dialog: HTMLDivElement;
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

  $effect(() => {
    if (plan) dialog?.querySelector<HTMLElement>(".cancel")?.focus();
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
    try {
      const out = await api.recycle(allowed.map((i) => i.id));
      const done = out.filter((o) => o.status === "done");
      const bytes = done.reduce((s, o) => s + o.onDisk, 0);
      const failed = out.length - done.length;
      toast(
        t("delete-result", { done: done.length, size: formatSize(bytes, loc), failed }),
        failed ? "error" : "ok",
      );
      for (const o of out) {
        if (o.status === "failed" && o.message) toast(o.message, "error");
        if (o.status === "declined") toast(t("delete-declined"));
        if (o.status === "blocked" && o.reason) toast(t(`reason-${o.reason}`, { template: templateLabel(null) }), "error");
      }
      await afterDelete(done.map((o) => o.id));
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
      app.deleting = null;
    }
  }
</script>

<div
  class="backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && close()}
  onkeydown={(e) => e.key === "Escape" && close()}
>
  <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="del-title" bind:this={dialog}>
    <header>
      <span class="ic"><Icon name="trash" size={22} /></span>
      <h2 id="del-title">{t("delete-title")}</h2>
    </header>

    {#if !plan}
      <p class="muted">…</p>
    {:else}
      {#if allowed.length}
        <p>{t("delete-intro")}</p>
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

      {#if plan.extraConfirm && allowed.length}
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
      <p class="note muted">{t("delete-nuke-note")}</p>
    {/if}

    <footer>
      <button class="btn cancel" onclick={close} disabled={busy}>{t("delete-cancel")}</button>
      <button class="btn danger" onclick={go} disabled={!canGo} data-effect title={t("action-recycle-hint")}>
        <Icon name="trash" size={16} />
        {busy ? t("delete-working") : t("delete-confirm")}
      </button>
    </footer>
  </div>
</div>

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
  header {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .ic {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--danger-soft);
    color: var(--danger);
  }
  h2 {
    margin: 0;
    font-size: 1.2em;
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
  .note {
    font-size: 0.85em;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
</style>
