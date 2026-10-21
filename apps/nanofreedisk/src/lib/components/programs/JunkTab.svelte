<script lang="ts">
  // Programs → Software junk: what installed programs pile up and never need — app
  // caches (Chromium / Electron), crash dumps, logs, temporary files, old versions
  // their updaters keep — grouped by program. Profiles, site data and anything a
  // rule pack of the Clean view already covers are never listed here.
  import { formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { expert, fail } from "../../state.svelte";
  import { reasonText } from "../../clean/state.svelte";
  import { programsApi } from "../../programs/api";
  import { junkItems } from "../../programs/model";
  import { cleanJunk, loadJunk, pg } from "../../programs/state.svelte";
  import type { JunkItem } from "../../programs/types";
  import { needsAck, sanitize, summarize, toggle as toggleId } from "../../selection";
  import CliHint from "../CliHint.svelte";
  import ConfirmTotals from "../ConfirmTotals.svelte";
  import GroupCheck from "../GroupCheck.svelte";
  import Icon from "../Icon.svelte";
  import SelectionBar from "../SelectionBar.svelte";

  const loc = $derived(intlLocale());
  let scope = $state<HTMLElement | null>(null);
  let ack = $state(false);

  $effect(() => {
    if (!pg.junk && !pg.junkLoading && !pg.running) void loadJunk();
  });

  const items = $derived(junkItems(pg.junk));
  const picked = $derived(sanitize(items, pg.junkPicked, expert()));
  const setPicked = (ids: string[]) => (pg.junkPicked = ids);
  const sum = $derived(summarize(items, picked, expert()));
  const reveal = (i: JunkItem) => programsApi.reveal(i.path).catch(fail);
  function open() {
    ack = false;
    pg.junkConfirm = true;
  }
  function go() {
    if (needsAck(sum) && !ack) return;
    void cleanJunk();
  }
  const rel = (folder: string, path: string) => (path.toLowerCase().startsWith(folder.toLowerCase()) ? path.slice(folder.length).replace(/^[\\/]+/, "") || path : path);
</script>

<div class="card pg-card">
  <div class="pg-row">
    <span class="lead"><Icon name="clean" size={20} /></span>
    <p class="pg-grow intro">{t("pg-junk-intro")}</p>
    <button class="btn" onclick={() => loadJunk()} disabled={pg.junkLoading || !!pg.running} data-effect>
      <Icon name="refresh" size={16} />{t("pg-junk-rescan")}
    </button>
  </div>
</div>

{#if pg.junkLoading && !pg.junk}
  <div class="card pg-loading"><Icon name="search" size={22} /><span>{t("pg-junk-loading")}</span></div>
{:else if pg.junk}
  <div class="groups" bind:this={scope} data-pg-junk>
    {#if !pg.junk.groups.length}
      <div class="card pg-card none"><Icon name="check" size={20} /><span class="muted">{t("pg-junk-none")}</span></div>
    {/if}
    {#each pg.junk.groups as g (g.key)}
      <div class="card pg-card group" data-pg-junk-group={g.key}>
        <h3>
          <GroupCheck {items} group={g.key} {picked} onchange={setPicked} expert={expert()} name={g.name} />
          <span class="ellipsis" title={g.folder}>{g.name}</span>
          {#if !g.program}<span class="pg-badge" title={t("pg-junk-unknown-hint")}>{t("pg-junk-unknown")}</span>{/if}
          {#if g.running}<span class="pg-badge caution" title={t("pg-junk-running-hint", { apps: g.requiresClosed.join(", ") })}><Icon name="alert" size={11} />{t("pg-junk-running")}</span>{/if}
          <span class="pg-grow"></span>
          <span class="pg-num muted">{formatSize(g.bytes, loc)}</span>
        </h3>
        <ul class="rows">
          {#each g.items as i (i.path)}
            <li class:blocked={!!i.blocked} data-pg-junk-item={i.path}>
              <input
                type="checkbox"
                checked={picked.includes(i.path)}
                disabled={!!i.blocked || !!pg.running}
                onchange={() => setPicked(toggleId(items, picked, i.path, expert()))}
                aria-label={t("pg-left-pick")}
              />
              <span class="pg-badge {i.blocked ? 'blocked' : i.level}">{i.blocked ? t("pg-left-blocked") : t(`clean-level-${i.level}`)}</span>
              <div class="what">
                <span><b>{t(`pg-junk-what-${i.what}`)}</b> <span class="mono muted path">{rel(g.folder, i.path)}</span></span>
                <span class="muted pg-small">{i.blocked ? reasonText(i.blocked) : t(`pg-junk-impact-${i.kind}`)}</span>
              </div>
              <span class="pg-num">{formatSize(i.bytes, loc)}</span>
              <button class="btn ghost icon" onclick={() => reveal(i)} title={t("pg-reveal")} aria-label={t("pg-reveal")}><Icon name="reveal" size={14} /></button>
            </li>
          {/each}
        </ul>
      </div>
    {/each}
  </div>
  <CliHint command={`nfd programs junk${sum.byLevel.some((l) => l.level === "caution") ? " --caution" : ""} --apply`} />
  <SelectionBar
    {items}
    {picked}
    onchange={setPicked}
    expert={expert()}
    {scope}
    primary
    busy={!!pg.running}
    action={{ label: (count, size) => t("pg-junk-clean", { count, size }), icon: "clean", hint: t("pg-junk-clean-hint"), testid: "junk", run: open }}
  />
{/if}

{#if pg.junkConfirm}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (pg.junkConfirm = false)} onkeydown={(e) => e.key === "Escape" && (pg.junkConfirm = false)}>
    <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="pg-junk-confirm" data-pg-junk-confirm>
      <h2 id="pg-junk-confirm"><Icon name="clean" size={20} />{t("pg-junk-confirm-title")}</h2>
      <p>{t("pg-junk-confirm-body")}</p>
      <ConfirmTotals summary={sum} bind:ack />
      <footer>
        <button class="btn" onclick={() => (pg.junkConfirm = false)}>{t("delete-cancel")}</button>
        <button class="btn primary" data-pg-junk-go onclick={go} disabled={needsAck(sum) && !ack} data-effect>
          <Icon name="clean" size={16} />{t("pg-junk-confirm-go")}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .lead {
    color: var(--accent);
    display: inline-flex;
  }
  .intro {
    margin: 0;
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .none {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  h3 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0 0 8px;
    font-size: 1em;
    min-width: 0;
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
    grid-template-columns: auto auto minmax(0, 1fr) 84px 30px;
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
    font-size: 0.86em;
    word-break: break-all;
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
    width: min(560px, 94vw);
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
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
