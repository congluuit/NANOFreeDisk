<script lang="ts">
  // Activity log (PLAN §13 G6): every clean and delete by day, with Restore while the
  // quarantine (7 days) or the Recycle Bin still holds the items.
  import { formatDateTime, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, expert, fail, toast } from "../state.svelte";
  import { cleanApi } from "../clean/api";
  import { reasonText } from "../clean/state.svelte";
  import type { ActivityDetail, OperationRow } from "../clean/types";
  import Icon from "../components/Icon.svelte";
  import StatCard from "../components/StatCard.svelte";

  const loc = $derived(intlLocale());
  let ops = $state<OperationRow[] | null>(null);
  let open = $state<ActivityDetail | null>(null);
  let busy = $state<string | null>(null);

  async function load() {
    try {
      ops = await cleanApi.activity(200);
    } catch (e) {
      fail(e);
      ops = [];
    }
  }

  $effect(() => {
    void app.version;
    void load();
  });

  const days = $derived.by(() => {
    const out: { day: string; rows: OperationRow[] }[] = [];
    for (const o of ops ?? []) {
      const day = new Date(o.startedAt * 1000).toLocaleDateString(loc, { weekday: "long", year: "numeric", month: "long", day: "numeric" });
      const last = out.at(-1);
      if (last?.day === day) last.rows.push(o);
      else out.push({ day, rows: [o] });
    }
    return out;
  });

  const freed = $derived((ops ?? []).reduce((n, o) => n + o.freed, 0));
  const restorable = $derived((ops ?? []).reduce((n, o) => n + o.restorable, 0));

  async function details(o: OperationRow) {
    if (open?.operation.id === o.id) {
      open = null;
      return;
    }
    try {
      open = await cleanApi.activityItems(o.id);
    } catch (e) {
      fail(e);
    }
  }

  async function restore(o: OperationRow, items?: number[]) {
    busy = o.id;
    try {
      const out = await cleanApi.restore(o.id, items);
      const ok = out.filter((r) => r.status === "restored").length;
      const bad = out.filter((r) => r.status === "failed" || r.status === "conflicts");
      toast(t("activity-restored", { done: ok, failed: bad.length }), bad.length ? "error" : "ok");
      for (const r of bad.slice(0, 3)) toast(`${r.path}: ${r.message ?? t(`activity-restore-${r.status}`)}`, "error");
      if (open?.operation.id === o.id) open = await cleanApi.activityItems(o.id);
      app.version++;
    } catch (e) {
      fail(e);
    } finally {
      busy = null;
    }
  }

  async function purge(o: OperationRow) {
    if (!confirm(t("activity-purge-confirm", { size: formatSize(o.freed, loc) }))) return;
    busy = o.id;
    try {
      await cleanApi.purge(o.id);
      toast(t("activity-purged"), "ok");
      await load();
    } catch (e) {
      fail(e);
    } finally {
      busy = null;
    }
  }
</script>

<section class="page">
  <header class="page-head">
    <div>
      <h1>{t("activity-title")}</h1>
      <p>{t("activity-intro")}</p>
    </div>
    <div class="page-actions">
      <button class="btn ghost icon" onclick={load} title={t("home-refresh")} aria-label={t("home-refresh")}><Icon name="refresh" /></button>
    </div>
  </header>

  {#if ops?.length}
    <div class="stats">
      <StatCard label={t("stat-freed")} value={formatSize(freed, loc)} unit={t("stat-operations", { count: ops.length })} />
      <StatCard label={t("stat-restorable")} tone={restorable ? "accent" : null} value={String(restorable)} unit={t("stat-restorable-unit")} />
    </div>
  {/if}

  {#if ops && !ops.length}
    <div class="panel none"><Icon name="check" size={22} /><span class="muted">{t("activity-empty")}</span></div>
  {/if}

  {#each days as d (d.day)}
    <section class="panel day-panel">
    <h2 class="day">{d.day}</h2>
    <ul class="ops">
      {#each d.rows as o (o.id)}
        <li class="op {o.state}">
          <div class="line">
            <span class="kind"><Icon name={o.kind === "clean" ? "clean" : o.kind === "tidy" ? "folder" : o.kind === "dupes" ? "duplicates" : o.kind === "devcache" ? "devcache" : o.kind === "ai-leftovers" || o.kind === "worktree" ? "agent" : "trash"} size={18} /></span>
            <div class="what">
              <strong>{t(`activity-kind-${o.kind}`)}</strong>
              <span class="muted small">
                {formatDateTime(o.startedAt, loc)} · {t("activity-items", { count: o.done })} · {t("activity-freed", { size: formatSize(o.freed, loc) })}
              </span>
            </div>
            <span class="state small">{t(`activity-state-${o.state}`)}{o.cancelled ? ` · ${t("clean-was-cancelled")}` : ""}</span>
            <button class="btn ghost" onclick={() => details(o)}>{open?.operation.id === o.id ? t("clean-hide") : t("activity-details")}</button>
            {#if o.restorable > 0}
              <button class="btn" disabled={busy === o.id} onclick={() => restore(o)} data-effect title={t("activity-restore-hint")}>
                <Icon name="undo" size={16} />{t("activity-restore", { count: o.restorable })}
              </button>
              {#if expert() && o.kind === "clean"}
                <button class="btn ghost" disabled={busy === o.id} onclick={() => purge(o)} title={t("activity-purge-hint")}>
                  <Icon name="trash" size={16} />
                </button>
              {/if}
            {/if}
          </div>
          {#if open?.operation.id === o.id}
            <ul class="items">
              {#each open.items as i (i.n)}
                <li class="small">
                  <Icon name={i.isDir ? "folder" : "file"} size={14} />
                  <span class="mono ellipsis" title={i.path}>{i.path}</span>
                  <span class="muted">{t(`activity-mode-${i.mode}`)}</span>
                  <span class="num">{formatSize(i.freed, loc)}</span>
                  <span class="status {i.status}" title={i.reason ? reasonText(i.reason) : undefined}>
                    {i.restored ? t("activity-item-restored") : t(`clean-status-${i.status}`)}
                  </span>
                  {#if !i.restored && (i.status === "done" || i.status === "partial") && i.mode !== "permanent" && o.state !== "purged"}
                    <button class="btn ghost icon" disabled={busy === o.id} onclick={() => restore(o, [i.n])} title={t("activity-restore-one")} aria-label={t("activity-restore-one")}>
                      <Icon name="undo" size={14} />
                    </button>
                  {:else}
                    <span></span>
                  {/if}
                  {#if i.message && i.mode === "permanent"}
                    <code class="mono msg muted" data-activity-message>{i.message}</code>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
    </section>
  {/each}
</section>

<style>
  .none {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .day-panel {
    margin-bottom: 14px;
    padding-top: 12px;
    padding-bottom: 6px;
  }
  .day {
    font-size: 0.86em;
    font-weight: 600;
    color: var(--text-muted);
    margin: 0 0 4px;
  }
  .ops {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .op {
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }
  .op:first-child {
    border-top: none;
  }
  .op.restored,
  .op.purged {
    opacity: 0.75;
  }
  .line {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .kind {
    display: grid;
    place-items: center;
    flex: none;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .what strong {
    font-weight: 600;
  }
  .what {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .state {
    color: var(--text-muted);
  }
  .items {
    list-style: none;
    margin: 10px 0 0;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-height: 280px;
    overflow: auto;
  }
  .items .msg {
    grid-column: 2 / -1;
    font-size: 0.92em;
    white-space: normal;
    word-break: break-all;
  }
  .items li {
    display: grid;
    grid-template-columns: auto 1fr auto auto auto 28px;
    gap: 10px;
    align-items: center;
    padding: 2px 4px;
  }
  .status.done,
  .status.partial {
    color: var(--risk-safe-ink);
  }
  .status.skipped {
    color: var(--risk-caution-ink);
  }
  .status.failed {
    color: var(--danger);
  }
</style>
