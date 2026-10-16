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
  <header>
    <div>
      <h1>{t("activity-title")}</h1>
      <p class="muted">{t("activity-intro")}</p>
    </div>
    <button class="btn ghost" onclick={load} title={t("home-refresh")}><Icon name="refresh" /></button>
  </header>

  {#if ops && !ops.length}
    <div class="card none"><Icon name="check" size={22} /><span class="muted">{t("activity-empty")}</span></div>
  {/if}

  {#each days as d (d.day)}
    <h2 class="day">{d.day}</h2>
    <ul class="ops">
      {#each d.rows as o (o.id)}
        <li class="card op {o.state}">
          <div class="line">
            <span class="kind"><Icon name={o.kind === "clean" ? "clean" : o.kind === "tidy" ? "folder" : "trash"} size={18} /></span>
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
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
  {/each}
</section>

<style>
  .page {
    height: 100%;
    overflow: auto;
    padding: 28px 32px 40px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 14px;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 1.6em;
  }
  header p {
    margin: 0;
    max-width: 70ch;
  }
  .none {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 16px;
  }
  .day {
    font-size: 0.95em;
    color: var(--text-muted);
    margin: 18px 0 8px;
  }
  .ops {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .op {
    padding: 10px 14px;
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
    color: var(--accent);
  }
  .what {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .small {
    font-size: 0.86em;
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
  .items li {
    display: grid;
    grid-template-columns: auto 1fr auto auto auto 28px;
    gap: 10px;
    align-items: center;
    padding: 2px 4px;
  }
  .status.done,
  .status.partial {
    color: var(--risk-safe);
  }
  .status.skipped {
    color: var(--risk-caution);
  }
  .status.failed {
    color: var(--danger);
  }
</style>
