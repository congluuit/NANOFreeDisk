<script lang="ts">
  // DevCache → Cache: vendor → app → folder with size, files, last write, risk,
  // "if deleted…", apps to close, the 🔒 Keep list and a "Clean safe" per vendor.
  import { formatCount, formatDate, formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { expert, fail } from "../../state.svelte";
  import { cleanable, impactText, levelOf } from "../../clean/state.svelte";
  import type { Candidate } from "../../clean/types";
  import { devApi } from "../../devcache/api";
  import { bytesOf, dev, runDev, safeIds, scanDev } from "../../devcache/state.svelte";
  import type { DevRule, VendorGroup } from "../../devcache/types";
  import { needsAck, sanitize, summarize, toggle as toggleId, type SelItem } from "../../selection";
  import CliHint from "../CliHint.svelte";
  import ConfirmTotals from "../ConfirmTotals.svelte";
  import GroupCheck from "../GroupCheck.svelte";
  import Icon from "../Icon.svelte";
  import SelectionBar from "../SelectionBar.svelte";
  import StatCard from "../StatCard.svelte";

  const loc = $derived(intlLocale());
  let open = $state<string[]>([]);
  let preview = $state<{ ruleId: string; items: Candidate[] | null } | null>(null);
  let permanent = $state(false);
  let typed = $state("");
  let ack = $state(false);
  let scope = $state<HTMLElement | null>(null);

  $effect(() => {
    if (!dev.report && !dev.scanning && !dev.running) void scanDev(true);
  });

  const report = $derived(dev.report);
  const visible = (rules: DevRule[]) => rules.filter((r) => cleanable(r, expert()));
  const vendors = $derived(
    (report?.vendors ?? []).filter((v) => v.apps.some((a) => visible(a.rules).length || a.kept.length)),
  );
  const all = $derived(vendors.flatMap((v) => v.apps.flatMap((a) => visible(a.rules))));
  const selItems = $derived<SelItem[]>(
    vendors.flatMap((v) =>
      v.apps.flatMap((a) =>
        visible(a.rules).map((r) => ({ id: r.ruleId, bytes: r.bytes, files: r.files, level: levelOf(r), group: v.vendor, skip: r.running.length ? r.running.join(", ") : null })),
      ),
    ),
  );
  const picked = $derived(sanitize(selItems, dev.picked, expert()));
  const confirmSummary = $derived(summarize(selItems, dev.confirm ?? [], expert()));
  const setPicked = (ids: string[]) => (dev.picked = ids);
  const allSafe = $derived(safeIds(report));
  const confirmRules = $derived(all.filter((r) => dev.confirm?.includes(r.ruleId)));
  const canPermanent = $derived(expert() && confirmRules.every((r) => r.level === "safe"));

  function toggleOpen(v: string) {
    open = open.includes(v) ? open.filter((x) => x !== v) : [...open, v];
  }
  function toggle(id: string) {
    dev.picked = toggleId(selItems, picked, id, expert());
  }
  function ask(ids: string[]) {
    if (!ids.length) return;
    permanent = false;
    typed = "";
    ack = false;
    dev.confirm = ids;
  }
  async function go() {
    const ids = dev.confirm;
    if (!ids) return;
    const perm = permanent && canPermanent;
    if (perm && typed !== "DELETE") return;
    if (needsAck(confirmSummary) && !ack) return;
    dev.confirm = null;
    await runDev(ids, expert(), perm);
  }
  async function showItems(r: DevRule) {
    if (preview?.ruleId === r.ruleId) {
      preview = null;
      return;
    }
    preview = { ruleId: r.ruleId, items: null };
    try {
      const items = await devApi.items(r.ruleId, 50);
      if (preview?.ruleId === r.ruleId) preview = { ruleId: r.ruleId, items };
    } catch (e) {
      fail(e);
      preview = null;
    }
  }
  const vendorSafe = (v: VendorGroup) => safeIds(report, v.vendor);
  const cliCommand = $derived(
    `nfd devcache clean --level ${picked.some((id) => all.find((r) => r.ruleId === id)?.level === "advanced") ? "advanced" : picked.some((id) => all.find((r) => r.ruleId === id)?.level === "caution") ? "caution" : "safe"} --rule ${picked.join(",") || "…"} --apply`,
  );
</script>

{#if !report}
  <div class="card dc-loading"><Icon name="search" size={22} /><span>{dev.scanning ? t("dev-scanning") : t("clean-not-scanned")}</span></div>
{:else}
  <div class="dc-scope" bind:this={scope}>
  <div class="summary">
    <div class="stats">
      <StatCard label={t("stat-safe")} tone="safe" value={formatSize(report.safe, loc)} hint={t("dev-safe-all-hint")} />
      <StatCard label={t("stat-review")} tone="caution" value={formatSize(report.caution, loc)} />
      {#if expert()}
        <StatCard label={t("clean-level-advanced")} tone="advanced" value={formatSize(report.advanced, loc)} />
      {/if}
      <StatCard label={t("stat-kept")} value={String(report.kept)} unit={t("stat-kept-unit")} hint={t("dev-kept-hint")} />
    </div>
    <div class="dc-row actions">
      <button class="btn ghost icon" onclick={() => scanDev()} disabled={dev.scanning || !!dev.running} title={t("clean-rescan")} aria-label={t("clean-rescan")}><Icon name="refresh" /></button>
      <button class="btn primary" disabled={!allSafe.length || !!dev.running} onclick={() => ask(allSafe)} data-effect title={t("dev-safe-all-hint")}>
        <Icon name="clean" />{t("clean-safe-button", { size: formatSize(bytesOf(allSafe), loc) })}
      </button>
    </div>
  </div>

  {#each vendors as v, i (v.vendor)}
    {@const isOpen = open.includes(v.vendor) !== i < 3}
    {@const vSafe = vendorSafe(v)}
    <div class="card dc-card vendor" class:over={v.overThreshold} data-vendor={v.vendor}>
      <div class="dc-row head">
        <GroupCheck items={selItems} group={v.vendor} picked={picked} onchange={setPicked} expert={expert()} name={v.vendor} busy={!!dev.running} />
        <button class="btn ghost icon" onclick={() => toggleOpen(v.vendor)} aria-expanded={isOpen} aria-label={v.vendor}>
          <span class="chev" class:open={isOpen}><Icon name="chevron" size={16} /></span>
        </button>
        <h2>{v.vendor}</h2>
        <b class="num">{formatSize(v.bytes, loc)}</b>
        {#if v.safeBytes}<span class="dc-badge safe">{t("dev-safe-part", { size: formatSize(v.safeBytes, loc) })}</span>{/if}
        {#if v.overThreshold && v.threshold}
          <span class="dc-badge caution" title={t("dev-over-hint")}><Icon name="alert" size={12} /> {t("dev-over", { size: formatSize(v.threshold, loc) })}</span>
        {/if}
        <span class="dc-grow"></span>
        {#if v.running.length}<span class="dc-warn dc-small"><Icon name="alert" size={14} />{t("clean-close-first", { apps: v.running.join(", ") })}</span>{/if}
        <button class="btn" disabled={!vSafe.length || !!dev.running} onclick={() => ask(vSafe)} data-effect title={t("dev-safe-vendor-hint", { vendor: v.vendor })}>
          <Icon name="clean" size={16} />{t("dev-safe-vendor", { size: formatSize(bytesOf(vSafe), loc) })}
        </button>
      </div>
      {#if isOpen}
        {#each v.apps as a (a.app)}
          {@const rules = visible(a.rules)}
          {#if rules.length || a.kept.length}
            <div class="app">
              <h3>{a.app}<span class="muted num">{formatSize(a.bytes, loc)}</span></h3>
              <ul class="dc-list">
                {#each rules as r (r.ruleId)}
                  <li class="dc-item" data-rule={r.ruleId}>
                    <label title={t("clean-pick-hint")}>
                      <input type="checkbox" checked={picked.includes(r.ruleId)} onchange={() => toggle(r.ruleId)} disabled={!!dev.running} />
                    </label>
                    <div>
                      <div class="dc-row">
                        <span class="dc-badge {levelOf(r)}">{t(`clean-level-${levelOf(r)}`)}</span>
                        {#if expert()}<code class="muted dc-small">{r.ruleId}</code>{/if}
                        <span class="muted dc-small">{t("dev-last-write", { date: r.newest ? formatDate(r.newest, loc) : "—" })}</span>
                      </div>
                      <p class="impact dc-small"><span class="muted">{t("clean-if-deleted")}</span> {impactText(r)}</p>
                      {#if r.requiresClosed.length}
                        <p class="dc-small" class:dc-warn={r.running.length} class:muted={!r.running.length}>
                          {#if r.running.length}<Icon name="alert" size={14} />{t("dev-running", { apps: r.running.join(", ") })}{:else}{t("dev-needs-closed", { apps: r.requiresClosed.join(", ") })}{/if}
                        </p>
                      {/if}
                      {#if preview?.ruleId === r.ruleId}
                        <div class="preview">
                          {#if !preview.items}
                            <p class="muted dc-small">…</p>
                          {:else}
                            <ul>
                              {#each preview.items as c (c.path)}
                                <li class="dc-small">
                                  <Icon name={c.isDir ? "folder" : "file"} size={14} />
                                  <span class="mono ellipsis" title={c.path}>{c.path}</span>
                                  <span class="num">{formatSize(c.bytes, loc)}</span>
                                  <button class="btn ghost icon" onclick={() => devApi.reveal(c.path).catch(fail)} title={t("dev-reveal")} aria-label={t("dev-reveal")}><Icon name="reveal" size={14} /></button>
                                </li>
                              {/each}
                            </ul>
                            {#if r.candidates > preview.items.length}
                              <p class="muted dc-small">{t("clean-preview-more", { shown: preview.items.length, count: r.candidates })}</p>
                            {/if}
                          {/if}
                        </div>
                      {/if}
                    </div>
                    <div class="dc-size">
                      <b class="num">{formatSize(r.bytes, loc)}</b>
                      <span class="muted dc-small">{t("clean-files", { count: formatCount(r.files, loc) })}</span>
                      <button class="btn ghost small" onclick={() => showItems(r)}>{preview?.ruleId === r.ruleId ? t("clean-hide") : t("dev-open-items")}</button>
                    </div>
                  </li>
                {/each}
              </ul>
              {#if a.kept.length}
                <ul class="kept">
                  {#each a.kept as k (k.ruleId)}
                    <li class="dc-small muted" title={t("dev-kept-hint")}><Icon name="lock" size={13} /> <span>{t("dev-kept")}</span> {impactText(k)}</li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/if}
        {/each}
      {/if}
    </div>
  {:else}
    <div class="card dc-loading"><Icon name="check" size={22} /><span>{t("dev-nothing")}</span></div>
  {/each}

  <CliHint command={cliCommand} />
  <SelectionBar
    items={selItems}
    {picked}
    onchange={setPicked}
    expert={expert()}
    busy={!!dev.running}
    {scope}
    primary
    action={{ label: (count, size) => t("sel-clean-n", { count, size }), hint: t("clean-run-hint"), testid: "devcache", run: () => ask(picked) }}
  />
  </div>
{/if}

{#if dev.confirm}
  <div class="dc-backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (dev.confirm = null)} onkeydown={(e) => e.key === "Escape" && (dev.confirm = null)}>
    <div class="dc-dialog card" role="alertdialog" aria-modal="true" aria-labelledby="dev-confirm-title">
      <h2 id="dev-confirm-title"><Icon name="clean" size={20} />{t("clean-confirm-title")}</h2>
      <p>{t("clean-confirm-body", { size: formatSize(bytesOf(dev.confirm), loc), rules: confirmRules.length })}</p>
      <ul class="dc-confirm-list">
        {#each confirmRules as r (r.ruleId)}
          <li><span class="dc-badge {levelOf(r)}">{t(`clean-level-${levelOf(r)}`)}</span><span class="ellipsis">{r.vendor} · {r.app}</span><span class="num">{formatSize(r.bytes, loc)}</span></li>
        {/each}
      </ul>
      <ConfirmTotals summary={confirmSummary} bind:ack />
      <p class="muted dc-small">{permanent && canPermanent ? t("clean-confirm-permanent-note") : t("clean-confirm-note")}</p>
      {#if canPermanent}
        <label class="perm"><input type="checkbox" bind:checked={permanent} /><span>{t("clean-permanent")}</span></label>
        {#if permanent}
          <label class="perm"><span>{t("clean-type-delete")}</span><input class="mono" bind:value={typed} placeholder="DELETE" /></label>
        {/if}
      {/if}
      <footer>
        <button class="btn" onclick={() => (dev.confirm = null)}>{t("delete-cancel")}</button>
        <button class="btn danger" onclick={go} disabled={(permanent && canPermanent && typed !== "DELETE") || (needsAck(confirmSummary) && !ack)} data-effect title={permanent && canPermanent ? t("clean-confirm-permanent-note") : t("clean-confirm-note")}>
          <Icon name="clean" size={16} />{permanent && canPermanent ? t("clean-confirm-permanent") : t("clean-confirm-go")}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .dc-scope {
    display: contents;
  }
  .summary {
    display: flex;
    gap: 12px;
    align-items: stretch;
    flex-wrap: wrap;
  }
  .summary .stats {
    flex: 1 1 480px;
    margin: 0;
  }
  .summary .actions {
    flex: none;
    align-self: center;
  }
  .chev {
    display: inline-flex;
    transition: transform 0.15s;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .vendor.over {
    border-color: var(--risk-caution);
  }
  .head h2 {
    margin: 0;
    font-size: 1.08em;
  }
  .app {
    margin: 10px 0 0 34px;
  }
  .app h3 {
    display: flex;
    justify-content: space-between;
    margin: 6px 0 2px;
    font-size: 0.98em;
  }
  .impact {
    margin: 4px 0 0;
  }
  .dc-item p {
    margin: 4px 0 0;
  }
  .kept {
    list-style: none;
    margin: 4px 0 0;
    padding: 6px 8px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .kept li {
    display: flex;
    gap: 6px;
    align-items: baseline;
    padding: 2px 0;
  }
  .kept li span {
    color: var(--risk-protected);
    white-space: nowrap;
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
    grid-template-columns: auto 1fr auto auto;
    gap: 8px;
    align-items: center;
    padding: 2px 4px;
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
</style>
