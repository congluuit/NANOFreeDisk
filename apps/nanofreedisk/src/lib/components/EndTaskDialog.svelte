<script lang="ts">
  // Confirmation for Close app / End process / End process tree
  // (docs/HARDWARE_MONITOR.md §5). The backend decides (task_verdict) and decides
  // again on a live handle when the user confirms (task_end): PID + creation time,
  // hard blocks, and the second confirmation for warned processes.
  import { errorText } from "../api";
  import { t } from "../i18n.svelte";
  import { fail, toast } from "../state.svelte";
  import { hwApi } from "../hw/api";
  import type { EndMode, EndResult, Task, TaskDetails, TaskVerdict } from "../hw/types";
  import Icon from "./Icon.svelte";

  interface Props {
    task: Task;
    mode: EndMode;
    onclose: () => void;
    /** After a WM_CLOSE: the parent re-checks and may reopen with mode "kill". */
    onrequested?: (task: Task) => void;
    note?: string;
  }
  let { task, mode, onclose, onrequested, note }: Props = $props();

  let verdict = $state<TaskVerdict | null>(null);
  let details = $state<TaskDetails | null>(null);
  let ack = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let dialog: HTMLDivElement;

  $effect(() => {
    hwApi
      .verdict(task.pid, task.create_time)
      .then((v) => (verdict = v))
      .catch((e) => (error = errorText(e)));
    hwApi
      .details(task.pid, task.create_time)
      .then((d) => (details = d))
      .catch(() => (details = null));
  });

  $effect(() => {
    if (verdict) dialog?.querySelector<HTMLElement>(".cancel")?.focus();
  });

  const warn = $derived(verdict?.verdict === "warn" ? verdict.reasons : []);
  const blocked = $derived(verdict?.verdict === "blocked" ? verdict.reasons : null);
  const gone = $derived(verdict?.verdict === "unavailable");
  const canGo = $derived(!!verdict && !blocked && !gone && (!warn.length || ack) && !busy);

  function close() {
    if (!busy) onclose();
  }

  function report(results: EndResult[]) {
    const bad = results.filter((r) => r.outcome.status !== "ended" && r.outcome.status !== "close-requested");
    const ended = results.filter((r) => r.outcome.status === "ended").length;
    for (const r of results) {
      if (r.outcome.status === "close-requested") {
        toast(t("task-close-sent", { name: task.title ?? task.name }));
        onrequested?.(task);
      }
    }
    if (ended) toast(t("task-ended", { count: ended, name: task.name }), "ok");
    for (const r of bad) {
      const o = r.outcome;
      const why =
        o.status === "blocked"
          ? t(`task-block-${o.detail}`)
          : o.status === "failed"
            ? o.detail
            : t(`task-outcome-${o.status}`);
      toast(t("task-not-ended", { name: r.name || String(r.pid), reason: why }), "error");
    }
  }

  async function go() {
    if (!canGo) return;
    busy = true;
    try {
      report(await hwApi.end(task.pid, task.create_time, mode, warn.length > 0 && ack));
      busy = false;
      onclose();
    } catch (e) {
      busy = false;
      fail(e);
    }
  }
</script>

<div
  class="backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && close()}
  onkeydown={(e) => e.key === "Escape" && close()}
>
  <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="end-title" bind:this={dialog}>
    <header>
      <span class="ic" class:soft={mode === "close"}><Icon name={mode === "close" ? "x" : "alert"} size={22} /></span>
      <h2 id="end-title">{t(`task-title-${mode}`)}</h2>
    </header>

    <dl class="who">
      <dt class="muted">{t("task-name")}</dt>
      <dd><strong>{task.title ?? task.name}</strong>{#if task.title}<span class="muted"> · {task.name}</span>{/if}</dd>
      <dt class="muted">PID</dt>
      <dd class="mono">{task.pid}</dd>
      <dt class="muted">{t("task-publisher")}</dt>
      <dd>
        {details?.publisher ?? task.publisher ?? t("task-publisher-unknown")}
        {#if details?.verified}<span class="ok"> · {t("task-signed")}</span>{/if}
      </dd>
      {#if task.path}
        <dt class="muted">{t("task-path")}</dt>
        <dd class="mono ellipsis" title={task.path}>{task.path}</dd>
      {/if}
      {#if mode === "kill-tree" && details}
        <dt class="muted">{t("task-children")}</dt>
        <dd>{details.children}</dd>
      {/if}
    </dl>

    {#if note}<p class="note-top">{note}</p>{/if}

    {#if error}
      <p class="err">{t("hw-error", { message: error })}</p>
    {:else if !verdict}
      <p class="muted">…</p>
    {:else if blocked}
      <p class="blocked"><Icon name="shield" size={16} />{t(`task-block-${blocked}`)}</p>
    {:else if gone}
      <p class="muted">{t("task-outcome-changed")}</p>
    {:else}
      <p class="what">{t(`task-what-${mode}`)}</p>
      {#if warn.length}
        <div class="warn">
          {#each warn as w (w)}
            <p><Icon name="alert" size={16} />{t(`task-warn-${w}`)}</p>
            {#if w === "service-host" && details?.services.length}
              <ul>
                {#each details.services as s (s)}<li>{s}</li>{/each}
              </ul>
            {/if}
          {/each}
          <label class="extra">
            <input type="checkbox" bind:checked={ack} />
            <span>{t("task-ack")}</span>
          </label>
        </div>
      {/if}
      <p class="muted small">{t("task-no-undo")}</p>
    {/if}

    <footer>
      <button class="btn cancel" onclick={close} disabled={busy}>{t("task-cancel")}</button>
      {#if !blocked && !gone}
        <button class="btn" class:danger={mode !== "close"} class:primary={mode === "close"} onclick={go} disabled={!canGo}>
          {busy ? t("task-working") : t(`task-confirm-${mode}`)}
        </button>
      {/if}
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
    width: min(600px, 94vw);
    max-height: 86vh;
    overflow: auto;
    padding: 22px 24px;
    box-shadow: var(--shadow);
  }
  header {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-bottom: 12px;
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
  .ic.soft {
    background: var(--accent-soft);
    color: var(--accent);
  }
  h2 {
    margin: 0;
    font-size: 1.2em;
  }
  .who {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 14px;
    margin: 0 0 12px;
    padding: 10px 12px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  dt,
  dd {
    margin: 0;
    min-width: 0;
  }
  .ok {
    color: var(--risk-safe);
  }
  .blocked {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--risk-protected);
    font-weight: 600;
  }
  .warn {
    border: 1px solid var(--risk-caution);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
    margin: 10px 0;
  }
  .warn p {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 6px 0;
  }
  .warn ul {
    margin: 0 0 6px 24px;
    padding: 0;
    font-size: 0.9em;
  }
  .extra {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 8px;
    font-weight: 600;
  }
  .note-top {
    color: var(--risk-caution);
  }
  .err {
    color: var(--danger);
  }
  .small {
    font-size: 0.86em;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
