<script lang="ts">
  // Activity log (PLAN §13 G6): every clean and delete by day, with Restore while the
  // quarantine (7 days) or the Recycle Bin still holds the items. The quarantine has
  // its own panel: what it holds, where, when it deletes things by itself, and
  // "delete for good" — per item, per operation or all at once — for when the space
  // is needed now.
  import { formatDateTime, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, fail, toast } from "../state.svelte";
  import { cleanApi } from "../clean/api";
  import { programsApi } from "../programs/api";
  import { reasonText } from "../clean/state.svelte";
  import { daysLeft, heldItem, restorableItem } from "../clean/quarantine";
  import type { ActivityDetail, OperationItem, OperationRow, PurgeResult, QuarantineInfo } from "../clean/types";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Icon from "../components/Icon.svelte";
  import StatCard from "../components/StatCard.svelte";

  const loc = $derived(intlLocale());
  let ops = $state<OperationRow[] | null>(null);
  let q = $state<QuarantineInfo | null>(null);
  let open = $state<ActivityDetail | null>(null);
  let busy = $state<string | null>(null);
  /** Show only the operations the quarantine still holds items of. */
  let onlyHeld = $state(false);
  /** A "delete for good" waiting for the user's yes. */
  let ask = $state<null | { title: string; body: string; detail: string | null; confirm: string; run: () => void }>(null);

  async function load() {
    try {
      const [list, info] = await Promise.all([cleanApi.activity(200), cleanApi.quarantine()]);
      ops = list;
      q = info;
      if (open && !list.some((o) => o.id === open?.operation.id)) open = null;
      else if (open) open = await cleanApi.activityItems(open.operation.id);
    } catch (e) {
      fail(e);
      ops ??= [];
    }
  }

  $effect(() => {
    void app.version;
    void load();
  });

  const shown = $derived((ops ?? []).filter((o) => !onlyHeld || o.quarantined > 0));
  const days = $derived.by(() => {
    const out: { day: string; rows: OperationRow[] }[] = [];
    for (const o of shown) {
      const day = new Date(o.startedAt * 1000).toLocaleDateString(loc, { weekday: "long", year: "numeric", month: "long", day: "numeric" });
      const last = out.at(-1);
      if (last?.day === day) last.rows.push(o);
      else out.push({ day, rows: [o] });
    }
    return out;
  });

  const freed = $derived((ops ?? []).reduce((n, o) => n + o.freed, 0));
  const restorable = $derived((ops ?? []).reduce((n, o) => n + o.restorable, 0));
  const expires = (at: number) => {
    const d = daysLeft(at);
    return { text: t("activity-expires", { days: d }), title: formatDateTime(at, loc) };
  };

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
      let out = await cleanApi.restore(o.id, items);
      // all-users leftovers (Program Files, ProgramData, HKLM) go back with one
      // administrator prompt
      const admin = out.filter((r) => r.status === "needs-admin").map((r) => r.n);
      if (admin.length && confirm(t("activity-restore-admin-confirm", { count: admin.length }))) {
        const again = await programsApi.restoreAdmin(o.id, admin);
        out = [...out.filter((r) => r.status !== "needs-admin"), ...again];
      }
      const ok = out.filter((r) => r.status === "restored").length;
      const bad = out.filter((r) => r.status === "failed" || r.status === "conflicts" || r.status === "needs-admin");
      toast(t("activity-restored", { done: ok, failed: bad.length }), bad.length ? "error" : "ok");
      for (const r of bad.slice(0, 3)) toast(`${r.path}: ${r.message ?? t(`activity-restore-${r.status}`)}`, "error");
      app.version++;
    } catch (e) {
      fail(e);
    } finally {
      busy = null;
    }
  }

  function purged(r: PurgeResult) {
    toast(t("activity-purged", { count: r.items, size: formatSize(r.bytes, loc) }), r.failed.length ? "error" : "ok");
    if (r.failed.length) toast(t("activity-purge-failed", { count: r.failed.length }), "error");
    for (const f of r.failed.slice(0, 3)) toast(f, "error");
  }

  async function purge(o: OperationRow, items?: number[]) {
    ask = null;
    busy = o.id;
    try {
      purged(await cleanApi.purge(o.id, items));
      app.version++;
    } catch (e) {
      fail(e);
    } finally {
      busy = null;
    }
  }

  async function emptyAll() {
    ask = null;
    busy = "*";
    try {
      purged(await cleanApi.emptyQuarantine());
      app.version++;
    } catch (e) {
      fail(e);
    } finally {
      busy = null;
    }
  }

  function askPurge(o: OperationRow) {
    ask = {
      title: t("activity-purge-title"),
      body: t("activity-purge-body", { count: o.quarantined, size: formatSize(o.held, loc) }),
      detail: null,
      confirm: t("activity-purge"),
      run: () => purge(o),
    };
  }
  function askPurgeItem(o: OperationRow, i: OperationItem) {
    ask = {
      title: t("activity-purge-title"),
      body: t("activity-purge-one-body", { size: formatSize(i.freed, loc) }),
      detail: i.path,
      confirm: t("activity-purge"),
      run: () => purge(o, [i.n]),
    };
  }
  function askEmpty() {
    if (!q?.items) return;
    ask = {
      title: t("quarantine-empty-title"),
      body: t("quarantine-empty-body", { count: q.items, size: formatSize(q.bytes, loc) }),
      detail: null,
      confirm: t("quarantine-empty"),
      run: emptyAll,
    };
  }
  const reveal = (index: number) => cleanApi.revealQuarantine(index).catch(fail);
  const kindIcon = (k: string) =>
    k === "clean" ? "clean" : k === "tidy" ? "folder" : k === "dupes" ? "duplicates" : k === "devcache" ? "devcache" : k === "ai-leftovers" || k === "worktree" ? "agent" : k === "uninstall" || k === "software-junk" ? "apps" : "trash";
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

  {#if q}
    <section class="panel quarantine" class:empty={!q.items} data-quarantine aria-labelledby="quarantine-title">
      <div class="q-head">
        <span class="q-icon"><Icon name="shield" size={22} /></span>
        <div class="q-what">
          <h2 id="quarantine-title">{t("quarantine-title")}</h2>
          <p class="muted">{t("quarantine-intro", { days: q.ttlDays })}</p>
        </div>
        <div class="q-actions">
          {#each q.roots as root, i (root)}
            <button class="btn ghost" onclick={() => reveal(i)} title={`${t("quarantine-open-hint")}\n${root}`}>
              <Icon name="reveal" size={16} />{q.roots.length > 1 ? `${t("quarantine-open")} (${root.slice(0, 2)})` : t("quarantine-open")}
            </button>
          {/each}
          <button class="btn danger" disabled={!q.items || !!busy} onclick={askEmpty} title={t("quarantine-empty-hint")} data-quarantine-empty>
            <Icon name="trash" size={16} />{t("quarantine-empty")}
          </button>
        </div>
      </div>
      {#if q.items}
        <div class="q-facts">
          <div class="fact"><b data-quarantine-bytes>{formatSize(q.bytes, loc)}</b><span>{t("quarantine-held")}</span></div>
          <div class="fact"><b data-quarantine-items>{q.items}</b><span>{t("quarantine-items", { ops: q.operations })}</span></div>
          <div class="fact">
            {#if q.nextExpiry}
              {@const e = expires(q.nextExpiry)}
              <b title={e.title}>{e.text}</b>
            {:else}
              <b>—</b>
            {/if}
            <span>{t("quarantine-next")}</span>
          </div>
        </div>
        <p class="muted small note"><Icon name="info" size={14} />{t("quarantine-recycle-note")}</p>
      {:else}
        <p class="muted q-none"><Icon name="check" size={16} />{t("quarantine-none")}</p>
      {/if}
      {#if ops?.length}
        <label class="only small"><input type="checkbox" bind:checked={onlyHeld} data-quarantine-only />{t("quarantine-only")}</label>
      {/if}
    </section>
  {/if}

  {#if ops && !ops.length}
    <div class="panel none"><Icon name="check" size={22} /><span class="muted">{t("activity-empty")}</span></div>
  {:else if ops && !shown.length}
    <div class="panel none"><Icon name="check" size={22} /><span class="muted">{t("quarantine-none")}</span></div>
  {/if}

  {#each days as d (d.day)}
    <section class="panel day-panel">
    <h2 class="day">{d.day}</h2>
    <ul class="ops">
      {#each d.rows as o (o.id)}
        <li class="op {o.state}" data-activity-op={o.id}>
          <div class="line">
            <span class="kind"><Icon name={kindIcon(o.kind)} size={18} /></span>
            <div class="what">
              <strong>{t(`activity-kind-${o.kind}`)}</strong>
              <span class="muted small">
                {formatDateTime(o.startedAt, loc)} · {t("activity-items", { count: o.done })} · {t("activity-freed", { size: formatSize(o.freed, loc) })}
              </span>
              {#if o.quarantined > 0}
                <span class="held small" data-activity-held>
                  <Icon name="shield" size={12} />{t("activity-in-quarantine", { count: o.quarantined, size: formatSize(o.held, loc) })}
                  {#if o.expiresAt}
                    {@const e = expires(o.expiresAt)}
                    <span class="muted" title={e.title}>· {e.text}</span>
                  {/if}
                </span>
              {/if}
            </div>
            <span class="state small">{t(`activity-state-${o.state}`)}{o.cancelled ? ` · ${t("clean-was-cancelled")}` : ""}</span>
            <button class="btn ghost" onclick={() => details(o)}>{open?.operation.id === o.id ? t("clean-hide") : t("activity-details")}</button>
            {#if o.restorable > 0}
              <button class="btn" disabled={!!busy} onclick={() => restore(o)} data-effect title={t("activity-restore-hint")}>
                <Icon name="undo" size={16} />{t("activity-restore", { count: o.restorable })}
              </button>
            {/if}
            {#if o.quarantined > 0}
              <button class="btn ghost purge" disabled={!!busy} onclick={() => askPurge(o)} title={t("activity-purge-hint")} data-activity-purge={o.id}>
                <Icon name="trash" size={16} />{t("activity-purge")}
              </button>
            {/if}
          </div>
          {#if open?.operation.id === o.id}
            {@const state = open.operation.state}
            <ul class="items">
              {#each open.items as i (i.n)}
                <li class="small" class:gone={i.purged}>
                  <Icon name={i.mode === "registry" ? "data" : i.isDir ? "folder" : "file"} size={14} />
                  <span class="mono ellipsis" title={i.path}>{i.path}</span>
                  <span class="muted">{t(`activity-mode-${i.mode}`)}</span>
                  <span class="num">{formatSize(i.freed, loc)}</span>
                  <span class="status {i.purged ? 'purged' : i.status}" title={i.reason ? reasonText(i.reason) : undefined}>
                    {i.purged ? t("activity-item-purged") : i.restored ? t("activity-item-restored") : t(`clean-status-${i.status}`)}
                  </span>
                  {#if restorableItem(i, state)}
                    <button class="btn ghost icon" disabled={!!busy} onclick={() => restore(o, [i.n])} title={t("activity-restore-one")} aria-label={t("activity-restore-one")}>
                      <Icon name="undo" size={14} />
                    </button>
                  {:else}
                    <span></span>
                  {/if}
                  {#if heldItem(i, state)}
                    <button class="btn ghost icon purge" disabled={!!busy} onclick={() => askPurgeItem(o, i)} title={t("activity-purge-one")} aria-label={t("activity-purge-one")} data-activity-purge-item={i.n}>
                      <Icon name="trash" size={14} />
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

{#if ask}
  <ConfirmDialog title={ask.title} body={ask.body} detail={ask.detail} confirm={ask.confirm} onconfirm={ask.run} oncancel={() => (ask = null)} />
{/if}

<style>
  .none {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .quarantine {
    margin-bottom: 14px;
    border-left: 4px solid var(--accent);
  }
  .quarantine.empty {
    border-left-color: var(--risk-safe);
  }
  .q-head {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    flex-wrap: wrap;
  }
  .q-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 11px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .empty .q-icon {
    color: var(--risk-safe-ink);
  }
  .q-what {
    flex: 1;
    min-width: 240px;
  }
  .q-what h2 {
    margin: 0 0 2px;
    font-size: 1.1em;
  }
  .q-what p {
    margin: 0;
  }
  .q-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .q-facts {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    margin: 14px 0 6px;
  }
  .fact {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 150px;
    padding: 10px 14px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .fact b {
    font-size: 1.25em;
    font-variant-numeric: tabular-nums;
  }
  .fact span {
    font-size: 0.84em;
    color: var(--text-muted);
  }
  .note,
  .q-none {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 8px 0 0;
  }
  .only {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    margin-top: 10px;
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
    flex-wrap: wrap;
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
    min-width: 200px;
  }
  .held {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    color: var(--accent);
    flex-wrap: wrap;
  }
  .state {
    color: var(--text-muted);
  }
  .purge {
    color: var(--danger);
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
    grid-template-columns: auto 1fr auto auto auto 28px 28px;
    gap: 10px;
    align-items: center;
    padding: 2px 4px;
  }
  .items li.gone .mono {
    text-decoration: line-through;
    color: var(--text-muted);
  }
  .status.done,
  .status.partial {
    color: var(--risk-safe-ink);
  }
  .status.skipped {
    color: var(--risk-caution-ink);
  }
  .status.failed,
  .status.purged {
    color: var(--danger);
  }
  .small {
    font-size: 0.86em;
  }
</style>
