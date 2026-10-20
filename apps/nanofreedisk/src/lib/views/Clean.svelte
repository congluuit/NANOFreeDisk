<script lang="ts">
  // Clean (PHASE3_CLEAN §2.5): junk found by the rule packs, grouped by pack and rule,
  // with what each deletion costs. Everything goes to the quarantine (restorable for
  // 7 days from the Activity log); 🟢 Safe rules are ticked by default, 🟡 Caution
  // must be ticked by hand, 🟠 Advanced shows in Expert mode only and 🔴 Protected is
  // guidance only.
  import { formatCount, formatDate, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, expert, fail } from "../state.svelte";
  import { cleanApi } from "../clean/api";
  import { bytesOf, clean, cleanable, impactText, levelOf, reasonText, runClean, scanJunk } from "../clean/state.svelte";
  import type { Candidate, RuleSummary } from "../clean/types";
  import { needsAck, sanitize, summarize, toggle as toggleId, type SelItem } from "../selection";
  import BrowserSites from "../components/BrowserSites.svelte";
  import CliHint from "../components/CliHint.svelte";
  import ConfirmTotals from "../components/ConfirmTotals.svelte";
  import GroupCheck from "../components/GroupCheck.svelte";
  import Icon from "../components/Icon.svelte";
  import SelectionBar from "../components/SelectionBar.svelte";
  import StatCard from "../components/StatCard.svelte";

  const loc = $derived(intlLocale());
  let preview = $state<{ ruleId: string; items: Candidate[] | null } | null>(null);
  let permanent = $state(false);
  let typed = $state("");
  let showGuide = $state(false);
  let ack = $state(false);
  let page = $state<HTMLElement | null>(null);

  $effect(() => {
    if (!clean.scan && !clean.scanning && !clean.running) void scanJunk();
  });

  const rules = $derived(clean.scan?.rules ?? []);
  const visible = $derived(rules.filter((r) => r.level !== "protected" && cleanable(r, expert())));
  const guide = $derived(rules.filter((r) => r.level === "protected"));
  const packs = $derived(
    (clean.scan?.packs ?? [])
      .map((p) => ({ ...p, rules: visible.filter((r) => r.packId === p.id) }))
      .filter((p) => p.rules.length),
  );
  // what the selection bar counts: a rule whose app is running is likely skipped
  const selItems = $derived<SelItem[]>(
    visible.map((r) => ({ id: r.ruleId, bytes: r.bytes, files: r.files, level: levelOf(r), group: r.packId, skip: r.running.length ? r.running.join(", ") : null })),
  );
  const picked = $derived(sanitize(selItems, clean.picked, expert()));
  const confirmSummary = $derived(summarize(selItems, clean.confirm ?? [], expert()));
  const safeIds = $derived(visible.filter((r) => r.level === "safe").map((r) => r.ruleId));
  const confirmRules = $derived(rules.filter((r) => clean.confirm?.includes(r.ruleId)));
  const confirmAllSafe = $derived(confirmRules.every((r) => r.level === "safe"));
  const canPermanent = $derived(expert() && confirmAllSafe);

  function toggle(id: string) {
    clean.picked = toggleId(selItems, picked, id, expert());
  }
  const setPicked = (ids: string[]) => (clean.picked = ids);

  async function showItems(r: RuleSummary) {
    if (preview?.ruleId === r.ruleId) {
      preview = null;
      return;
    }
    preview = { ruleId: r.ruleId, items: null };
    try {
      const items = await cleanApi.items(r.ruleId, 50);
      if (preview?.ruleId === r.ruleId) preview = { ruleId: r.ruleId, items };
    } catch (e) {
      fail(e);
      preview = null;
    }
  }

  function ask(ids: string[]) {
    if (!ids.length) return;
    permanent = false;
    typed = "";
    ack = false;
    clean.confirm = ids;
  }

  async function go() {
    const ids = clean.confirm;
    if (!ids) return;
    const perm = permanent && canPermanent;
    if (perm && typed !== "DELETE") return;
    if (needsAck(confirmSummary) && !ack) return;
    clean.confirm = null;
    await runClean(ids, expert(), perm);
  }

  const levelLabel = (r: RuleSummary) => t(`clean-level-${levelOf(r)}`);
  const countOf = (level: string) => visible.filter((r) => r.level === level).length;
  const cliCommand = $derived(
    `nfd clean --level ${picked.some((id) => rules.find((r) => r.ruleId === id)?.level === "advanced") ? "advanced" : picked.some((id) => rules.find((r) => r.ruleId === id)?.level === "caution") ? "caution" : "safe"} --rule ${picked.join(",") || "…"} --apply`,
  );
</script>

<section class="page clean" bind:this={page}>
  <header class="page-head">
    <div>
      <h1>{t("clean-title")}</h1>
      <p>{t("clean-intro")}</p>
    </div>
    <div class="page-actions actions-top">
      <button class="btn ghost" onclick={() => (app.view = "activity")} title={t("clean-activity-hint")}>
        <Icon name="undo" />{t("nav-activity")}
      </button>
      <button class="btn ghost icon" onclick={() => scanJunk()} disabled={clean.scanning || !!clean.running} title={t("clean-rescan")} aria-label={t("clean-rescan")}>
        <Icon name="refresh" />
      </button>
      {#if clean.scan}
        <button class="btn primary" disabled={!safeIds.length || !!clean.running} onclick={() => ask(safeIds)} data-effect title={t("clean-safe-hint")}>
          <Icon name="clean" />{t("clean-safe-button", { size: formatSize(bytesOf(safeIds), loc) })}
        </button>
      {/if}
    </div>
  </header>

  {#if clean.running}
    <div class="card running" aria-live="polite">
      <div class="row">
        <strong>{t("clean-running", { done: clean.running.done, total: clean.running.total })}</strong>
        <span class="muted">{t("clean-freed-so-far", { size: formatSize(clean.running.freed, loc) })}</span>
        <span class="grow"></span>
        <button class="btn" onclick={() => cleanApi.cancel().catch(fail)}>{t("clean-cancel")}</button>
      </div>
      <div class="bar"><span style="width: {clean.running.total ? (clean.running.done / clean.running.total) * 100 : 0}%"></span></div>
      <p class="muted small ellipsis mono">{clean.running.path}</p>
    </div>
  {:else if clean.last}
    {@const r = clean.last}
    <div class="card result" class:bad={r.failed > 0}>
      <Icon name={r.failed ? "alert" : "check"} size={22} />
      <div class="grow">
        <strong>{t("clean-done-title", { size: formatSize(r.freed, loc) })}</strong>
        <p class="muted small">
          {t("clean-done-detail", { done: r.done, partial: r.partial, skipped: r.skipped, failed: r.failed })}
          {#if r.cancelled}· {t("clean-was-cancelled")}{/if}
        </p>
        {#if r.problems.length}
          <details>
            <summary class="small">{t("clean-problems", { count: r.problems.length })}</summary>
            <ul class="problems">
              {#each r.problems.slice(0, 20) as p (p.path)}
                <li class="small"><span class="mono ellipsis" title={p.path}>{p.path}</span><span class="muted">{t(`clean-status-${p.status}`)}{p.reason ? ` · ${reasonText(p.reason)}` : ""}</span></li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
      <button class="btn" onclick={() => (app.view = "activity")}><Icon name="undo" size={16} />{t("clean-undo")}</button>
      <button class="btn ghost icon" onclick={() => (clean.last = null)} aria-label={t("clean-dismiss")}><Icon name="x" size={16} /></button>
    </div>
  {/if}

  {#if !clean.scan}
    <div class="card loading">
      <Icon name="search" size={22} />
      <span>{clean.scanning ? t("clean-scanning") : t("clean-not-scanned")}</span>
    </div>
  {:else}
    <div class="stats">
      <StatCard label={t("stat-safe")} tone="safe" value={formatSize(clean.scan.safe, loc)} unit={t("stat-items", { count: countOf("safe") })} hint={t("clean-safe-hint")} />
      <StatCard label={t("stat-review")} tone="caution" value={formatSize(clean.scan.caution, loc)} unit={t("stat-items", { count: countOf("caution") })} />
      {#if expert()}
        <StatCard label={t("clean-level-advanced")} tone="advanced" value={formatSize(clean.scan.advanced, loc)} unit={t("stat-items", { count: countOf("advanced") })} />
      {/if}
      <StatCard label={t("stat-checked")} value={formatDate(clean.scan.at, loc)} unit={t("stat-rules", { count: visible.length })} />
    </div>

    {#each packs as p (p.id)}
      <div class="panel pack">
        <h2>
          <GroupCheck items={selItems} group={p.id} picked={picked} onchange={setPicked} expert={expert()} name={p.name} busy={!!clean.running} />
          <span class="grow">{p.name}</span><span class="muted num">{formatSize(p.bytes, loc)}</span>
        </h2>
        <ul class="rules">
          {#each p.rules as r (r.ruleId)}
            <li class="rule {levelOf(r)}">
              <label class="pick" title={t("clean-pick-hint")}>
                <input type="checkbox" data-rule-check={r.ruleId} checked={picked.includes(r.ruleId)} onchange={() => toggle(r.ruleId)} disabled={!!clean.running} />
              </label>
              <div class="what">
                <div class="name">
                  <strong>{r.app}</strong><span class="muted small">{r.vendor}</span>
                  <span class="tag {levelOf(r)}">{levelLabel(r)}</span>
                </div>
                <p class="impact small"><span class="muted">{t("clean-if-deleted")}</span> {impactText(r)}</p>
                {#if r.running.length}
                  <p class="warn small"><Icon name="alert" size={14} />{t("clean-close-first", { apps: r.running.join(", ") })}</p>
                {/if}
                {#if preview?.ruleId === r.ruleId}
                  <div class="preview">
                    {#if !preview.items}
                      <p class="muted small">…</p>
                    {:else}
                      <ul>
                        {#each preview.items as c (c.path)}
                          <li class="small">
                            <Icon name={c.isDir ? "folder" : "file"} size={14} />
                            <span class="mono ellipsis" title={c.path}>{c.path}</span>
                            <span class="num">{formatSize(c.bytes, loc)}</span>
                          </li>
                        {/each}
                      </ul>
                      {#if r.candidates > preview.items.length}
                        <p class="muted small">{t("clean-preview-more", { shown: preview.items.length, count: r.candidates })}</p>
                      {/if}
                    {/if}
                  </div>
                {/if}
              </div>
              <div class="size">
                <b class="num">{formatSize(r.bytes, loc)}</b>
                <span class="muted small">{t("clean-files", { count: formatCount(r.files, loc) })}</span>
                <button class="btn ghost small" onclick={() => showItems(r)}>{preview?.ruleId === r.ruleId ? t("clean-hide") : t("clean-preview")}</button>
              </div>
            </li>
          {/each}
        </ul>
        {#if p.id === "browsers"}
          <BrowserSites />
        {/if}
      </div>
    {:else}
      <div class="card loading"><Icon name="check" size={22} /><span>{t("clean-nothing")}</span></div>
    {/each}

    {#if guide.length}
      <div class="panel pack guide">
        <button class="btn ghost" onclick={() => (showGuide = !showGuide)}>
          <Icon name="shield" size={16} />{t("clean-guide-title", { count: guide.length })}
        </button>
        {#if showGuide}
          <ul class="rules">
            {#each guide as r (r.ruleId)}
              <li class="rule protected">
                <span></span>
                <div class="what">
                  <div class="name"><strong>{r.app}</strong><span class="muted small">{r.vendor}</span><span class="tag protected">{levelLabel(r)}</span></div>
                  <p class="impact small">{impactText(r)}</p>
                </div>
                <span></span>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}

    <CliHint command={cliCommand} />
    <SelectionBar
      items={selItems}
      {picked}
      onchange={setPicked}
      expert={expert()}
      busy={!!clean.running}
      scope={page}
      primary
      action={{ label: (count, size) => t("sel-clean-n", { count, size }), hint: t("clean-run-hint"), testid: "clean", run: () => ask(picked) }}
    />
  {/if}
</section>

{#if clean.confirm}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (clean.confirm = null)} onkeydown={(e) => e.key === "Escape" && (clean.confirm = null)}>
    <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="clean-confirm-title">
      <h2 id="clean-confirm-title"><Icon name="clean" size={20} />{t("clean-confirm-title")}</h2>
      <p>{t("clean-confirm-body", { size: formatSize(bytesOf(clean.confirm), loc), rules: confirmRules.length })}</p>
      <ul class="confirm-list">
        {#each confirmRules as r (r.ruleId)}
          <li><span class="tag {levelOf(r)}">{levelLabel(r)}</span><span class="ellipsis">{r.app}</span><span class="num">{formatSize(r.bytes, loc)}</span></li>
        {/each}
      </ul>
      <ConfirmTotals summary={confirmSummary} bind:ack />
      <p class="muted small">{permanent && canPermanent ? t("clean-confirm-permanent-note") : t("clean-confirm-note")}</p>
      {#if canPermanent}
        <label class="perm">
          <input type="checkbox" bind:checked={permanent} />
          <span>{t("clean-permanent")}</span>
        </label>
        {#if permanent}
          <label class="perm">
            <span>{t("clean-type-delete")}</span>
            <input class="mono" bind:value={typed} placeholder="DELETE" />
          </label>
        {/if}
      {/if}
      <footer>
        <button class="btn" onclick={() => (clean.confirm = null)}>{t("delete-cancel")}</button>
        <button class="btn danger" data-confirm-go onclick={go} disabled={(permanent && canPermanent && typed !== "DELETE") || (needsAck(confirmSummary) && !ack)} data-effect title={permanent && canPermanent ? t("clean-confirm-permanent-note") : t("clean-confirm-note")}>
          <Icon name="clean" size={16} />{permanent && canPermanent ? t("clean-confirm-permanent") : t("clean-confirm-go")}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .clean {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .clean > .page-head,
  .clean > .stats {
    margin-bottom: 0;
  }
  .grow {
    flex: 1;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .running,
  .result,
  .loading,
  .summary {
    padding: 14px 16px;
  }
  .running .bar {
    margin: 10px 0 6px;
    height: 8px;
  }
  .running p {
    margin: 0;
  }
  .result {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    border-left: 4px solid var(--risk-safe);
  }
  .result.bad {
    border-left-color: var(--danger);
  }
  .result p {
    margin: 2px 0 0;
  }
  .problems {
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
    max-height: 180px;
    overflow: auto;
  }
  .problems li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
  }
  .loading {
    display: flex;
    gap: 10px;
    align-items: center;
    color: var(--text-muted);
  }
  .pack h2 {
    display: flex;
    gap: 10px;
    align-items: center;
    margin: 0 0 6px;
    font-size: 1.06em;
  }
  .rules {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .rule {
    display: grid;
    grid-template-columns: 26px 1fr auto;
    gap: 10px;
    padding: 11px 0;
    border-top: 1px solid var(--border);
  }
  .rule:first-child {
    border-top: none;
  }
  .pick {
    padding-top: 2px;
  }
  .name {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .name strong {
    font-weight: 600;
  }
  .impact {
    margin: 3px 0 0;
    color: var(--text-muted);
  }
  .warn {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 4px 0 0;
    color: var(--risk-caution-ink);
  }
  .size {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    min-width: 110px;
  }
  .preview ul {
    list-style: none;
    margin: 8px 0 0;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-height: 220px;
    overflow: auto;
  }
  .preview li {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 2px 4px;
  }
  .guide .rules {
    margin-top: 8px;
  }
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
    width: min(620px, 94vw);
    max-height: 86vh;
    overflow: auto;
    padding: 22px 24px;
    box-shadow: var(--shadow);
  }
  .dialog h2 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0 0 8px;
    font-size: 1.2em;
  }
  .confirm-list {
    list-style: none;
    margin: 8px 0;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    max-height: 200px;
    overflow: auto;
  }
  .confirm-list li {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 3px 4px;
  }
  .perm {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 8px;
  }
  .perm input.mono {
    width: 120px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
  }
  .dialog footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
</style>
