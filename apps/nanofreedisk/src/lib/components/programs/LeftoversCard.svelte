<script lang="ts">
  // What an uninstalled program left behind, grouped files / shortcuts / registry /
  // services, each with the evidence that ties it to the program. 🟢 items are
  // ticked; 🟡 ones (a vendor folder, a name another program shares, a weak match)
  // only by hand; blocked ones never. All-users items need one administrator prompt.
  import { formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { expert, fail } from "../../state.svelte";
  import { reasonText } from "../../clean/state.svelte";
  import { programsApi } from "../../programs/api";
  import { LEFTOVER_GROUPS, groupOf, leftoverItems, shortHive } from "../../programs/model";
  import { dismissLeftovers, pg, removeLeftovers } from "../../programs/state.svelte";
  import type { Leftover, LeftoverReport } from "../../programs/types";
  import { summarize, toggle as toggleId } from "../../selection";
  import AppIcon from "./AppIcon.svelte";
  import CliHint from "../CliHint.svelte";
  import GroupCheck from "../GroupCheck.svelte";
  import Icon from "../Icon.svelte";
  import SelectionBar from "../SelectionBar.svelte";

  let { report }: { report: LeftoverReport } = $props();
  const loc = $derived(intlLocale());
  const id = $derived(report.program.id);
  const items = $derived(leftoverItems(report));
  const picked = $derived(pg.leftPicked[id] ?? []);
  const setPicked = (ids: string[]) => (pg.leftPicked[id] = ids);
  const sum = $derived(summarize(items, picked, expert()));
  const machinePicked = $derived(report.items.filter((i) => i.scope === "machine" && picked.includes(i.id)).length);
  let scope = $state<HTMLElement | null>(null);

  const ICON: Record<string, string> = { files: "folder", shortcuts: "link", registry: "data", services: "cpu" };
  function shown(l: Leftover): string {
    if (l.kind === "registry-key" || l.kind === "registry-value") return shortHive(l.path) + (l.value ? ` → ${l.value}` : "");
    if (l.kind === "service") return l.detail ?? l.path;
    return l.path;
  }
  const reveal = (l: Leftover) => programsApi.reveal(l.path).catch(fail);
  const busy = $derived(pg.removing === id);
</script>

<div class="card pg-card left" bind:this={scope} data-pg-leftovers={id}>
  <div class="pg-row head">
    <AppIcon id={id} name={report.program.name} size={30} />
    <div class="pg-grow">
      <strong>{t(`pg-left-title-${report.mode}`, { name: report.program.name })}</strong>
      <p class="muted pg-small">{report.items.length ? t("pg-left-intro") : t("pg-left-none")}</p>
    </div>
    <button class="btn ghost icon" onclick={() => dismissLeftovers(id)} disabled={busy} aria-label={t("clean-dismiss")} title={t("pg-left-dismiss")}><Icon name="x" size={16} /></button>
  </div>
  {#each LEFTOVER_GROUPS as g (g.id)}
    {@const rows = report.items.filter((i) => groupOf(i) === g.id)}
    {#if rows.length}
      <h3>
        {#if g.id !== "services"}<GroupCheck {items} group={g.id} {picked} onchange={setPicked} expert={expert()} name={t(`pg-left-group-${g.id}`)} />{/if}
        <Icon name={ICON[g.id]} size={16} />{t(`pg-left-group-${g.id}`)}
        <span class="count">{rows.length}</span>
      </h3>
      <ul class="rows">
        {#each rows as l (l.id)}
          <li class:blocked={!!l.blocked} data-pg-leftover={l.id}>
            {#if l.kind === "service"}
              <span class="dot"></span>
            {:else}
              <input
                type="checkbox"
                checked={picked.includes(l.id)}
                disabled={!!l.blocked || busy}
                onchange={() => setPicked(toggleId(items, picked, l.id, expert()))}
                aria-label={t("pg-left-pick")}
              />
            {/if}
            <span class="pg-badge {l.blocked ? 'blocked' : l.level}">{l.blocked ? t("pg-left-blocked") : t(`clean-level-${l.level}`)}</span>
            <div class="what">
              <span class="mono ellipsis path" title={l.path}>{shown(l)}</span>
              <span class="muted pg-small">
                {t(`pg-reason-${l.reason}`)}
                {#if l.also}· {t("pg-left-also", { name: l.also })}{/if}
                {#if l.large}· <b class="large">{t("pg-left-large")}</b>{/if}
                {#if l.blocked && l.kind !== "service"}· {reasonText(l.blocked)}{/if}
                {#if l.kind === "service" && l.detail}· <code>sc delete "{l.detail}"</code>{/if}
              </span>
            </div>
            {#if l.scope === "machine" && l.kind !== "service"}<span class="admin" title={t("pg-left-machine-hint")}><Icon name="shield" size={14} /></span>{:else}<span></span>{/if}
            <span class="pg-num">{l.bytes ? formatSize(l.bytes, loc) : ""}</span>
            {#if l.kind === "folder" || l.kind === "file" || l.kind === "shortcut"}
              <button class="btn ghost icon" onclick={() => reveal(l)} title={t("pg-reveal")} aria-label={t("pg-reveal")}><Icon name="reveal" size={14} /></button>
            {:else}<span></span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/each}
  {#if machinePicked}
    <p class="pg-small admin-note"><Icon name="shield" size={14} />{t("pg-left-admin", { count: machinePicked })}</p>
  {/if}
  {#if report.items.length}
    <CliHint command={report.mode === "broken" ? `nfd programs leftovers "${report.program.name}" --apply` : `nfd programs leftovers "${report.program.name}" --forced`} />
    <SelectionBar
      {items}
      {picked}
      onchange={setPicked}
      expert={expert()}
      {scope}
      sticky={false}
      nested
      {busy}
      action={{ label: (count, size) => t("pg-left-remove", { count, size }), icon: "trash", hint: t("pg-left-remove-hint"), testid: "leftovers", run: () => removeLeftovers(id) }}
    />
    {#if sum.count === 0}<p class="muted pg-small">{t("pg-left-nothing-picked")}</p>{/if}
  {/if}
</div>

<style>
  .left {
    border-left: 4px solid var(--accent);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .head p {
    margin: 2px 0 0;
  }
  h3 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 8px 0 2px;
    font-size: 0.95em;
  }
  .count {
    font-size: 0.8em;
    font-weight: normal;
    padding: 0 8px;
    border-radius: 8px;
    background: var(--accent-soft);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .rows li {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr) 20px 84px 30px;
    gap: 10px;
    align-items: center;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }
  .rows li:last-child {
    border-bottom: none;
  }
  .rows li.blocked {
    opacity: 0.7;
  }
  .what {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .path {
    font-size: 0.88em;
  }
  .dot {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: var(--border);
  }
  .admin {
    color: var(--text-muted);
    display: inline-flex;
  }
  .large {
    color: var(--risk-caution);
    font-weight: 600;
  }
  .admin-note {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text-muted);
    margin: 4px 0 0;
  }
</style>
