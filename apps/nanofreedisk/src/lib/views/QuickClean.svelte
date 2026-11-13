<script lang="ts">
  // Quick clean (docs/QUICK_CLEAN.md): one button checks the PC with every tool,
  // one screen shows what was found in three plain groups (safe to remove, worth a
  // look, good to know), one confirmation runs the ticked parts through each tool's
  // own run function, one result screen says what was freed and what stayed.
  import { titleOf } from "../advice";
  import { clean, reasonText } from "../clean/state.svelte";
  import { leftoverAction, leftoverBody, leftoverTitle } from "../clean/leftovers";
  import { dev } from "../devcache/state.svelte";
  import { formatCount, formatDateTime, formatPercent, formatSize } from "../format";
  import { i18n, intlLocale, t } from "../i18n.svelte";
  import { currentOs, hasFeature } from "../platform";
  import { app } from "../state.svelte";
  import { outcomeTotals, planSteps, progressOf, runPlan, totals, type Detail, type Group, type Row, type RowId, type Step } from "../quick/model";
  import { askClean, emptyBin, openTool, quick, runQuick, scanAll, stopQuick, stopScan, toggleTick } from "../quick/state.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Icon from "../components/Icon.svelte";

  const loc = $derived(intlLocale());
  const lang = $derived(i18n.locale);
  const os = $derived(currentOs(app.info?.os));
  const preview = $derived(planSteps({ junk: hasFeature(os, "clean"), bin: hasFeature(os, "recycle-bin"), drive: true }));
  const tot = $derived(totals(quick.rows, quick.ticks));
  const plan = $derived(runPlan(quick.rows, quick.ticks));
  const done = $derived(outcomeTotals(quick.parts));
  const last = $derived(app.settings.lastQuick);

  const GROUPS: Group[] = ["safe", "look", "info"];
  const rowsOf = (g: Group) => quick.rows.filter((r) => r.group === g);

  const ICON: Record<RowId, string> = {
    junk: "trash",
    dev: "devcache",
    advice: "analyze",
    empty: "folder",
    links: "link",
    "junk-caution": "alert",
    dupes: "duplicates",
    large: "drive",
    more: "zoom",
    bin: "trash",
    guide: "shield",
    admin: "lock",
  };
  /** Where "Review" leads. */
  const REVIEW: Partial<Record<RowId, "duplicates" | "oldlarge" | "analyze">> = { dupes: "duplicates", large: "oldlarge", more: "analyze" };

  let open = $state<Record<string, boolean>>({});
  let ack = $state(false);
  let bigAck = $state(false);
  let binAsk = $state(false);

  function rowSub(r: Row): string {
    if (r.id === "junk") return t("quick-row-junk-sub", { count: r.details.length });
    return t(`quick-row-${r.id}-sub`, { count: formatCount(r.count, loc) });
  }

  function detailLabel(d: Detail): string {
    if (!d.group) return d.label;
    const title = titleOf(d.group, lang);
    return d.group.location ? `${t(title.key, title.args)} · ${d.group.location}` : t(title.key, title.args);
  }

  function stepFound(s: Step): string {
    if (s.status === "done") {
      if (s.id === "scan") return t("quick-found-scan", { count: formatCount(s.count ?? 0, loc), size: formatSize(s.bytes ?? 0, loc) });
      if (s.id === "advice") return s.bytes ? t("quick-found-advice", { size: formatSize(s.bytes, loc) }) : t("quick-found-nothing");
      if (!s.bytes && !s.count) return t("quick-found-nothing");
      if (s.bytes != null && s.count != null) return t("quick-found-size-items", { size: formatSize(s.bytes, loc), count: formatCount(s.count, loc) });
      if (s.bytes != null) return formatSize(s.bytes, loc);
      return t("quick-found-items", { count: formatCount(s.count ?? 0, loc) });
    }
    if (s.status === "running" && s.id === "scan" && app.scanning?.progress) {
      const p = app.scanning.progress;
      return t("quick-found-scan", { count: formatCount(p.files, loc), size: formatSize(p.onDisk, loc) });
    }
    return t(`quick-status-${s.status}`);
  }

  const STEP_ICON: Record<Step["status"], string> = { waiting: "", running: "", done: "check", skipped: "info", failed: "alert", cancelled: "x" };

  function ask() {
    ack = false;
    bigAck = false;
    void askClean();
  }
  const canGo = $derived((!tot.caution || ack) && (!quick.tidyPlan?.extraConfirm || bigAck));
  const blocked = $derived(quick.tidyPlan?.items.filter((i) => !i.allowed).length ?? 0);
  const taskBytes = (tool: string, bytes: number) => (tool === "tidy" && quick.tidyPlan ? quick.tidyPlan.onDisk : bytes);

  function progressText(tool: string): string {
    const p = tool === "junk" ? clean.running : tool === "dev" ? dev.running : null;
    if (!p || !p.total) return t("quick-status-running");
    return t("quick-progress", { done: formatCount(p.done, loc), total: formatCount(p.total, loc), size: formatSize(p.freed, loc) });
  }

  const finishable = $derived(quick.leftovers.some((g) => leftoverAction(g) !== null));
</script>

<section class="page quick" data-quick-phase={quick.phase}>
  <div class="wrap">
    <header class="page-head">
      <div>
        <h1>{t("quick-title")}</h1>
        <p>{t("quick-intro")}</p>
      </div>
    </header>

    {#if quick.phase === "idle"}
      <div class="hero card">
        <span class="hero-icon"><Icon name="clean" size={40} /></span>
        <button class="btn primary big" data-quick-scan onclick={() => scanAll()} data-effect title={t("quick-scan-hint")}>
          <Icon name="search" size={18} />{t("quick-scan")}
        </button>
        <p class="safety"><Icon name="shield" size={16} />{t("quick-safety")}</p>
        {#if last}
          <p class="muted small">{t("quick-last", { size: formatSize(last.freed, loc), date: formatDateTime(last.at, loc) })}</p>
        {/if}
        <div class="checks">
          <span class="muted small">{t("quick-checks")}</span>
          <ul>
            {#each preview as s (s.id)}
              <li class="tag">{t(`quick-step-${s.id}`)}</li>
            {/each}
          </ul>
        </div>
      </div>
    {:else if quick.phase === "scanning"}
      <div class="card work" aria-live="polite">
        <div class="work-head">
          <div class="grow">
            <h2>{t("quick-scanning")}</h2>
            <p class="muted small">{t("quick-scanning-sub")}</p>
          </div>
          <button class="btn" onclick={() => stopScan()} disabled={quick.stopping} data-quick-stop data-effect title={t("quick-stop-hint")}>
            <Icon name="x" size={16} />{t("quick-stop")}
          </button>
        </div>
        <div class="bar"><span style="width: {progressOf(quick.steps) * 100}%"></span></div>
        {@render stepList(quick.steps)}
      </div>
    {:else if quick.phase === "ready"}
      <div class="summary card" class:empty={!quick.rows.some((r) => r.tickable)}>
        <div class="grow">
          <h2 data-quick-total={tot.bytes}>
            {#if tot.bytes > 0}
              {t(tot.upTo ? "quick-ready-title-upto" : "quick-ready-title", { size: formatSize(tot.bytes, loc) })}
            {:else}
              {t("quick-ready-nothing")}
            {/if}
          </h2>
          <p class="muted small">{t("quick-ready-sub")}</p>
          {#if tot.upTo}<p class="muted small">{t("quick-upto-note")}</p>{/if}
          {#if quick.stopped}<p class="warn small"><Icon name="info" size={14} />{t("quick-stopped-note")}</p>{/if}
        </div>
        <div class="sum-actions">
          <button class="btn ghost" onclick={() => scanAll()} data-effect title={t("quick-rescan-hint")}>
            <Icon name="refresh" size={16} />{t("quick-rescan")}
          </button>
          <button class="btn primary big" data-quick-clean disabled={!plan.length || tot.bytes + tot.count === 0} onclick={ask} data-effect title={t("quick-clean-hint")}>
            <Icon name="clean" size={18} />{t(tot.upTo ? "quick-clean-upto" : "quick-clean", { size: formatSize(tot.bytes, loc) })}
          </button>
        </div>
      </div>

      {#each GROUPS as g (g)}
        {@const rows = rowsOf(g)}
        {#if rows.length}
          <section class="panel group {g}" data-quick-group={g}>
            <header>
              <h2><span class="dot {g === 'safe' ? 'safe' : g === 'look' ? 'caution' : 'accent'}"></span>{t(`quick-group-${g}`)}</h2>
              <p class="muted small">{t(`quick-group-${g}-sub`)}</p>
            </header>
            <ul class="rows">
              {#each rows as r (r.id)}
                {@render rowView(r)}
              {/each}
            </ul>
          </section>
        {/if}
      {/each}

      <details class="panel how">
        <summary>{t("quick-how")}</summary>
        {@render stepList(quick.steps)}
      </details>
    {:else if quick.phase === "cleaning"}
      <div class="card work" aria-live="polite">
        <div class="work-head">
          <div class="grow">
            <h2>{t("quick-cleaning")}</h2>
            <p class="muted small">{t("quick-cleaning-sub")}</p>
          </div>
          <button class="btn" onclick={() => stopQuick()} data-effect title={t("quick-stop-clean-hint")}>
            <Icon name="x" size={16} />{t("quick-stop")}
          </button>
        </div>
        <ul class="steps">
          {#each quick.parts as p (p.tool)}
            <li class="step {p.status}" data-quick-part={p.tool} data-status={p.status}>
              <span class="st">
                {#if p.status === "running"}<span class="spinner" aria-hidden="true"></span>
                {:else if p.status === "done"}<Icon name="check" size={16} />
                {:else if p.status === "failed"}<Icon name="alert" size={16} />
                {:else if p.status === "cancelled"}<Icon name="x" size={16} />
                {:else}<span class="ring" aria-hidden="true"></span>{/if}
              </span>
              <span class="label">{t(`quick-tool-${p.tool}`)}</span>
              <span class="found muted small">
                {#if p.status === "running"}{progressText(p.tool)}
                {:else if p.status === "done"}{t("quick-part-freed", { size: formatSize(p.freed, loc) })}
                {:else}{t(`quick-status-${p.status}`)}{/if}
              </span>
            </li>
          {/each}
        </ul>
      </div>
    {:else}
      <div class="card result" data-quick-done class:bad={done.failed > 0}>
        <span class="result-icon"><Icon name={done.freed > 0 ? "check" : "info"} size={30} /></span>
        <h2>{done.freed > 0 ? t("quick-done-title", { size: formatSize(done.freed, loc) }) : t("quick-done-nothing")}</h2>
        <p class="muted">{t("quick-done-sub", { count: formatCount(done.done, loc) })}</p>
        <div class="result-actions">
          <button class="btn primary" data-quick-history onclick={() => (app.view = "activity")}>
            <Icon name="undo" size={16} />{t("quick-history")}
          </button>
          <button class="btn" onclick={() => scanAll()} data-effect title={t("quick-rescan-hint")}>
            <Icon name="refresh" size={16} />{t("quick-rescan")}
          </button>
        </div>
      </div>

      <section class="panel stayed" data-quick-stayed>
        <h2>{t("quick-stayed-title")}</h2>
        <ul class="parts">
          {#each quick.parts as p (p.tool)}
            <li data-quick-part={p.tool} data-status={p.status}>
              <div class="part-head">
                <strong class="grow">{t(`quick-tool-${p.tool}`)}</strong>
                <span class="num">{t("quick-part-freed", { size: formatSize(p.freed, loc) })}</span>
              </div>
              {#if p.error}
                <p class="warn small"><Icon name="alert" size={14} />{t("quick-part-failed", { error: p.error })}</p>
              {:else if p.status === "cancelled"}
                <p class="muted small">{t("quick-status-cancelled")}</p>
              {/if}
              {#if p.stayed}
                <p class="muted small">{t("quick-stayed-line", { count: formatCount(p.stayed, loc) })}</p>
                <ul class="reasons">
                  {#each p.reasons.slice(0, 5) as r (r.code)}
                    <li class="small"><span>{reasonText(r.code)}</span><span class="num muted">{formatCount(r.count, loc)}</span></li>
                  {/each}
                </ul>
              {/if}
            </li>
          {/each}
        </ul>
        {#if quick.leftovers.length}
          <div class="left">
            {#each quick.leftovers as g (g.cause)}
              {@const title = leftoverTitle(g)}
              <div class="left-group {g.cause}" data-left-cause={g.cause}>
                <div class="part-head">
                  <Icon name={g.cause === "in-use" ? "lock" : g.cause === "needs-admin" ? "shield" : "info"} size={16} />
                  <strong class="grow">{t(title.key, title.args)}</strong>
                  <span class="muted small num">{t("clean-left-size", { size: formatSize(g.bytes, loc) })}</span>
                </div>
                <p class="muted small">{t(leftoverBody(g))}</p>
              </div>
            {/each}
            {#if finishable}
              <button class="btn" onclick={() => (app.view = "clean")}><Icon name="open" size={16} />{t("quick-finish-in-junk")}</button>
            {/if}
          </div>
        {:else if done.stayed === 0 && done.failed === 0}
          <p class="muted small ok"><Icon name="check" size={14} />{t("quick-stayed-none")}</p>
        {/if}
      </section>
    {/if}
  </div>
</section>

{#snippet stepList(steps: Step[])}
  <ul class="steps">
    {#each steps as s (s.id)}
      <li class="step {s.status}" data-quick-step={s.id} data-status={s.status}>
        <span class="st">
          {#if s.status === "running"}<span class="spinner" aria-hidden="true"></span>
          {:else if s.status === "waiting"}<span class="ring" aria-hidden="true"></span>
          {:else}<Icon name={STEP_ICON[s.status]} size={16} />{/if}
        </span>
        <span class="label">{t(`quick-step-${s.id}`)}</span>
        <span class="found small" class:muted={s.status !== "done"}>{stepFound(s)}</span>
        {#if s.error || s.note}
          <span class="note muted small">{s.error ?? t(s.note ?? "")}</span>
        {/if}
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet rowView(r: Row)}
  {@const ticked = quick.ticks.includes(r.id)}
  <li class="row" class:ticked data-quick-row={r.id}>
    <div class="row-main">
      {#if r.tickable}
        <label class="pick">
          <input type="checkbox" data-quick-pick={r.id} checked={ticked} onchange={() => toggleTick(r.id)} aria-label={t(`quick-row-${r.id}`)} />
        </label>
      {:else}
        <span class="pick"></span>
      {/if}
      <span class="ric {r.group}"><Icon name={ICON[r.id]} size={18} /></span>
      <div class="what">
        <strong>{t(`quick-row-${r.id}`)}</strong>
        <span class="muted small">{rowSub(r)}</span>
        {#if r.running.length && ticked}
          <span class="warn small"><Icon name="alert" size={13} />{t("quick-close-apps", { apps: r.running.join(", ") })}</span>
        {/if}
      </div>
      <span class="size num">{r.bytes > 0 ? formatSize(r.bytes, loc) : ""}</span>
      <div class="row-actions">
        {#if r.details.length}
          <button class="btn ghost small" aria-expanded={!!open[r.id]} onclick={() => (open[r.id] = !open[r.id])}>
            {open[r.id] ? t("quick-hide") : t("quick-details")}
          </button>
        {/if}
        {#if REVIEW[r.id]}
          {@const target = REVIEW[r.id]}
          <button class="btn small" data-quick-review={r.id} onclick={() => target && (app.view = target)}>
            <Icon name="open" size={14} />{t("quick-review")}
          </button>
        {/if}
        {#if r.id === "bin"}
          <button class="btn small" data-quick-bin onclick={() => (binAsk = true)} data-effect title={t("rb-empty-hint")}>
            <Icon name="trash" size={14} />{t("rb-empty")}
          </button>
        {/if}
      </div>
    </div>
    {#if open[r.id]}
      <ul class="details">
        {#each r.details.slice(0, 12) as d, i (i)}
          <li class="small">
            <span class="ellipsis" class:mono={!d.group && (r.id === "empty" || r.id === "links")} title={d.label}>{detailLabel(d)}</span>
            {#if d.ruleId}
              {@const rule = d.ruleId}
              <button class="btn ghost small" onclick={() => openTool(rule)}><Icon name="open" size={13} />{t("quick-open-tool")}</button>
            {/if}
            <span class="num muted">{d.bytes > 0 ? formatSize(d.bytes, loc) : ""}</span>
          </li>
        {/each}
      </ul>
      {#if r.details.length > 12}
        <p class="muted small more">{t("quick-more-lines", { count: r.details.length - 12 })}</p>
      {/if}
    {/if}
  </li>
{/snippet}

{#if quick.confirm}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (quick.confirm = false)} onkeydown={(e) => e.key === "Escape" && (quick.confirm = false)}>
    <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="quick-confirm-title" data-quick-confirm>
      <h2 id="quick-confirm-title"><Icon name="clean" size={20} />{t(tot.upTo ? "quick-confirm-title-upto" : "quick-confirm-title", { size: formatSize(tot.bytes, loc) })}</h2>
      <p>{t("quick-confirm-intro")}</p>
      <ol class="confirm-list">
        {#each plan as task (task.tool)}
          <li>
            <span class="ellipsis">{t(`quick-tool-${task.tool}`)}</span>
            <span class="muted small">{t("quick-where-quarantine")}</span>
            <span class="num">{formatSize(taskBytes(task.tool, task.bytes), loc)}</span>
          </li>
        {/each}
      </ol>
      {#if blocked}
        <p class="muted small"><Icon name="shield" size={14} />{t("quick-confirm-tidy-blocked", { count: blocked })}</p>
      {/if}
      {#if tot.running.length}
        <p class="warn small"><Icon name="alert" size={14} />{t("quick-close-apps", { apps: tot.running.join(", ") })}</p>
      {/if}
      <p class="muted small">{t("quick-confirm-note")} {t("clean-confirm-note")}</p>
      {#if tot.caution}
        <label class="ack"><input type="checkbox" bind:checked={ack} data-quick-ack /><span>{t("quick-confirm-ack")}</span></label>
      {/if}
      {#if quick.tidyPlan?.extraConfirm}
        <label class="ack">
          <input type="checkbox" bind:checked={bigAck} />
          <span>{t("delete-extra", { size: formatSize(quick.tidyPlan.onDisk, loc), percent: formatPercent(app.summary?.volumeTotal ? quick.tidyPlan.onDisk / app.summary.volumeTotal : 0, loc) })}</span>
        </label>
      {/if}
      <footer>
        <button class="btn cancel" onclick={() => (quick.confirm = false)}>{t("delete-cancel")}</button>
        <button class="btn primary" data-quick-go disabled={!canGo} onclick={() => runQuick()} data-effect title={t("quick-confirm-go-hint")}>
          <Icon name="clean" size={16} />{t("quick-confirm-go")}
        </button>
      </footer>
    </div>
  </div>
{/if}

{#if binAsk}
  {@const bin = quick.rows.find((r) => r.id === "bin")}
  <ConfirmDialog
    title={t("rb-confirm-title")}
    body={t("rb-confirm-body", { count: formatCount(bin?.count ?? 0, loc), size: formatSize(bin?.bytes ?? 0, loc) })}
    confirm={t("rb-empty")}
    onconfirm={() => {
      binAsk = false;
      void emptyBin();
    }}
    oncancel={() => (binAsk = false)}
  />
{/if}

<style>
  .wrap {
    max-width: 1040px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .wrap > .page-head {
    margin-bottom: 0;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 1.12em;
  }
  .btn.big {
    padding: 0.7em 1.4em;
    font-size: 1.06em;
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 4px 0 0;
    color: var(--risk-caution-ink);
  }

  /* start */
  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 34px 24px 26px;
    text-align: center;
  }
  .hero-icon {
    display: grid;
    place-items: center;
    width: 76px;
    height: 76px;
    border-radius: 22px;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
  .safety {
    display: flex;
    gap: 8px;
    align-items: center;
    max-width: 560px;
    margin: 0;
    color: var(--text-muted);
  }
  .safety :global(svg) {
    flex: none;
    color: var(--risk-safe-ink);
  }
  .hero > p {
    margin: 0;
  }
  .checks {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
  }
  .checks ul {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
    list-style: none;
    margin: 0;
    padding: 0;
    max-width: 640px;
  }

  /* scanning and cleaning */
  .work {
    padding: 18px 20px;
  }
  .work-head {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .work-head p {
    margin: 2px 0 0;
  }
  .work .bar {
    height: 8px;
    margin: 14px 0 6px;
  }
  .steps {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
  }
  .step {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) auto;
    gap: 2px 10px;
    align-items: center;
    padding: 9px 0;
    border-bottom: 1px solid var(--border);
  }
  .step:last-child {
    border-bottom: none;
  }
  .step .label {
    min-width: 0;
  }
  .step.waiting .label,
  .step.skipped .label,
  .step.cancelled .label {
    color: var(--text-muted);
  }
  .step.running .label {
    font-weight: 600;
  }
  .step .found {
    text-align: right;
  }
  .step .note {
    grid-column: 2 / -1;
  }
  .st {
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }
  .step.done .st {
    color: var(--risk-safe-ink);
  }
  .step.failed .st {
    color: var(--risk-caution-ink);
  }
  .ring {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--border);
  }
  .spinner {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--surface-2);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* result */
  .summary {
    display: flex;
    gap: 16px;
    align-items: center;
    flex-wrap: wrap;
    padding: 18px 20px;
    border-left: 4px solid var(--risk-safe);
  }
  .summary.empty {
    border-left-color: var(--accent);
  }
  .summary h2 {
    font-size: 1.35em;
  }
  .summary p {
    margin: 4px 0 0;
  }
  .sum-actions {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .group header {
    margin-bottom: 4px;
  }
  .group h2 {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .group header p {
    margin: 2px 0 0;
  }
  .group.safe {
    border-top: 3px solid var(--risk-safe);
  }
  .group.look {
    border-top: 3px solid var(--risk-caution);
  }
  .group.info {
    border-top: 3px solid var(--border);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }
  .row:last-child {
    border-bottom: none;
    padding-bottom: 2px;
  }
  .row-main {
    display: grid;
    grid-template-columns: 22px 36px minmax(0, 1fr) auto auto;
    gap: 12px;
    align-items: center;
  }
  .pick {
    display: grid;
    place-items: center;
  }
  .pick input {
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
  }
  .ric {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .ric.safe {
    background: var(--risk-safe-soft);
    color: var(--risk-safe-ink);
  }
  .ric.look {
    background: var(--risk-caution-soft);
    color: var(--risk-caution-ink);
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .size {
    font-weight: 600;
    white-space: nowrap;
  }
  .row-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
  .details {
    list-style: none;
    margin: 8px 0 0 70px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    max-height: 260px;
    overflow: auto;
  }
  .details li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 10px;
    align-items: center;
    padding: 3px 0;
  }
  .more {
    margin: 4px 0 0 70px;
  }
  .how summary {
    cursor: pointer;
    font-weight: 600;
  }

  /* done */
  .result {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 26px 24px 22px;
    text-align: center;
    border-top: 4px solid var(--risk-safe);
  }
  .result.bad {
    border-top-color: var(--risk-caution);
  }
  .result p {
    margin: 0;
  }
  .result-icon {
    display: grid;
    place-items: center;
    width: 60px;
    height: 60px;
    border-radius: 50%;
    background: var(--risk-safe-soft);
    color: var(--risk-safe-ink);
  }
  .result h2 {
    font-size: 1.4em;
  }
  .result-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
    margin-top: 6px;
  }
  .stayed h2 {
    margin-bottom: 8px;
  }
  .parts {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .parts > li {
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .parts > li:last-child {
    border-bottom: none;
  }
  .parts p {
    margin: 4px 0 0;
  }
  .part-head {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .reasons {
    list-style: none;
    margin: 4px 0 0;
    padding: 0 0 0 12px;
  }
  .reasons li {
    display: flex;
    justify-content: space-between;
    gap: 10px;
  }
  .left {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
    margin-top: 10px;
  }
  .left-group {
    align-self: stretch;
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
    margin: 4px 0 0;
  }
  .ok {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 6px 0 0;
  }

  /* confirmation */
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
    width: min(560px, 92vw);
    max-height: 90vh;
    overflow: auto;
    padding: 22px 24px;
    box-shadow: var(--shadow);
  }
  .dialog h2 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0 0 8px;
  }
  .dialog p {
    margin: 0 0 8px;
  }
  .dialog p.small,
  .dialog p.warn {
    display: flex;
    gap: 6px;
    align-items: flex-start;
  }
  .confirm-list {
    margin: 0 0 10px;
    padding: 6px 10px 6px 28px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .confirm-list li {
    padding: 4px 0;
  }
  .confirm-list li > span:first-child {
    display: inline-block;
    max-width: 60%;
    vertical-align: bottom;
  }
  .confirm-list .num {
    float: right;
    font-weight: 600;
  }
  .confirm-list .muted {
    margin-left: 6px;
  }
  .ack {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 8px 0 0;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--risk-caution-soft);
  }
  .dialog footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }

  @media (max-width: 760px) {
    .row-main {
      grid-template-columns: 22px 36px minmax(0, 1fr) auto;
    }
    .row-actions {
      grid-column: 3 / -1;
      justify-content: flex-start;
    }
    .details,
    .more {
      margin-left: 0;
    }
  }
</style>
