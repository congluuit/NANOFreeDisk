<script lang="ts">
  // Clean → "Web browser caches" → "Advanced: clean one website" (crates/nfd-browsers):
  // the websites in each browser profile's caches with their size, searchable; ticked
  // sites lose only their own cache entries — Code Cache, Service Worker caches and
  // Firefox's cache. Chromium's blockfile HTTP cache is listed, never edited per site;
  // cookies, logins, site storage, history, passwords and extensions are never read
  // or touched. Expanded by default in Expert mode, a disclosure in Simple mode.
  import { formatCount, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, expert, fail } from "../state.svelte";
  import { browsersApi } from "../browsers/api";
  import { profileGroup, siteId, siteItems, siteMatches } from "../browsers/sites";
  import { br, cleanSites, confirmSites, loadSites } from "../browsers/state.svelte";
  import type { BrowserReport, ProfileReport } from "../browsers/types";
  import { sanitize, toggle as toggleId } from "../selection";
  import GroupCheck from "./GroupCheck.svelte";
  import Icon from "./Icon.svelte";
  import SelectionBar from "./SelectionBar.svelte";

  const PAGE = 40;
  const loc = $derived(intlLocale());
  let userOpen = $state<boolean | null>(null);
  const open = $derived(userOpen ?? expert());
  let panel = $state<HTMLElement | null>(null);
  let shown = $state<Record<string, number>>({});

  $effect(() => {
    if (open && !br.report && !br.loading) void loadSites();
  });

  const items = $derived(siteItems(br.report, br.query));
  const allItems = $derived(siteItems(br.report));
  const picked = $derived(sanitize(allItems, br.picked, expert()));
  const setPicked = (ids: string[]) => (br.picked = ids);
  const sitesOf = (p: ProfileReport) => p.sites.filter((s) => !br.query || siteMatches(s.site, br.query));
  const busy = $derived(!!br.running);

  function more(key: string) {
    shown = { ...shown, [key]: (shown[key] ?? PAGE) + PAGE * 4 };
  }
  const storeList = (kinds: string[]) => kinds.map((k) => t(`br-store-${k}`)).join(" · ");
  const blockfile = (b: BrowserReport) => b.profiles.some((p) => p.stores.some((s) => s.format === "blockfile"));
</script>

<div class="sites" bind:this={panel} data-browser-sites>
  <button class="btn ghost disclose" aria-expanded={open} onclick={() => (userOpen = !open)} data-br-toggle title={t("br-hint")}>
    <span class="chev" class:open><Icon name="chevron" size={14} /></span>
    <Icon name="search" size={16} />{t("br-title")}
  </button>

  {#if open}
    <p class="muted small intro">{t("br-intro")}</p>
    <div class="find">
      <input type="search" bind:value={br.query} placeholder={t("br-search")} aria-label={t("br-search")} data-br-search />
      <button class="btn ghost" onclick={() => loadSites()} disabled={br.loading || busy} title={t("br-rescan")}><Icon name="refresh" size={16} /></button>
    </div>

    {#if br.running}
      <div class="running" aria-live="polite">
        <strong>{t("clean-running", { done: br.running.done, total: br.running.total })}</strong>
        <span class="muted small">{t("clean-freed-so-far", { size: formatSize(br.running.freed, loc) })}</span>
        <button class="btn ghost small" onclick={() => browsersApi.cancel().catch(fail)}>{t("clean-cancel")}</button>
      </div>
    {:else if br.last}
      <div class="result" data-br-result>
        <Icon name={br.last.failed ? "alert" : "check"} size={18} />
        <span class="grow">{t("br-done", { size: formatSize(br.last.freed, loc), done: br.last.done + br.last.partial })}</span>
        <button class="btn ghost small" onclick={() => (app.view = "activity")}><Icon name="undo" size={14} />{t("clean-undo")}</button>
        <button class="btn ghost icon" onclick={() => (br.last = null)} aria-label={t("clean-dismiss")}><Icon name="x" size={14} /></button>
      </div>
    {/if}

    {#if br.loading && !br.report}
      <p class="muted small"><Icon name="search" size={14} /> {t("br-loading")}</p>
    {:else if br.report && !br.report.browsers.length}
      <p class="muted small">{t("br-none")}</p>
    {:else if br.report}
      {#each br.report.browsers as b (b.id)}
        <div class="browser" data-br-browser={b.id}>
          {#each b.profiles as p (p.path)}
            {@const key = profileGroup(b, p)}
            {@const list = sitesOf(p)}
            <div class="profile">
              <h3>
                <GroupCheck items={items} group={key} picked={picked} onchange={setPicked} expert={expert()} name={`${b.name} · ${p.name}`} {busy} />
                <span class="grow">{b.name} <span class="muted small">· {t("br-profile", { name: p.name })}</span></span>
                <span class="muted small num">{t("br-sites", { count: formatCount(p.sites.length, loc) })}</span>
              </h3>
              {#if b.running}
                <p class="warn small"><Icon name="alert" size={14} />{t("br-running", { exe: b.exe })}</p>
              {/if}
              <ul class="list">
                {#each list.slice(0, shown[key] ?? PAGE) as s (s.site)}
                  {@const id = siteId(b, p, s)}
                  {@const can = s.cleanableBytes > 0 || s.cleanableEntries > 0}
                  <li class:off={!can} data-br-site={s.site}>
                    <input
                      type="checkbox"
                      checked={picked.includes(id)}
                      disabled={!can || busy}
                      onchange={() => setPicked(toggleId(allItems, picked, id, expert()))}
                      title={can ? t("clean-pick-hint") : t("br-listed-only-hint")}
                      aria-label={s.site}
                    />
                    <div class="what">
                      <strong class="ellipsis" title={s.site}>{s.site}</strong>
                      <span class="muted small ellipsis">{storeList(s.stores)}</span>
                    </div>
                    <div class="size">
                      <b class="num">{formatSize(s.cleanableBytes, loc)}</b>
                      {#if s.listedBytes}<span class="muted small num" title={t("br-listed-only-hint")}>{t("br-site-listed", { size: formatSize(s.listedBytes, loc) })}</span>{/if}
                    </div>
                  </li>
                {:else}
                  <li class="muted small">{t("br-no-match")}</li>
                {/each}
              </ul>
              {#if list.length > (shown[key] ?? PAGE)}
                <button class="btn ghost small" onclick={() => more(key)}>{t("br-more", { count: formatCount(list.length - (shown[key] ?? PAGE), loc) })}</button>
              {/if}
            </div>
          {/each}
          {#if blockfile(b)}
            <p class="muted small note"><Icon name="shield" size={14} />{t("br-blockfile-note")}</p>
          {/if}
        </div>
      {/each}
      <SelectionBar
        {items}
        all={allItems}
        {picked}
        onchange={setPicked}
        expert={expert()}
        {busy}
        scope={panel}
        noLevels
        sticky={false}
        nested
        files={false}
        action={{ label: (count, size) => t("br-clean-n", { count, size }), hint: t("br-clean-hint"), testid: "browser-sites", run: () => confirmSites(picked) }}
      />
    {/if}
  {/if}
</div>

{#if br.confirm}
  {@const c = br.confirm}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (br.confirm = null)} onkeydown={(e) => e.key === "Escape" && (br.confirm = null)}>
    <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="br-confirm-title" data-br-confirm>
      <h2 id="br-confirm-title"><Icon name="clean" size={20} />{t("br-confirm-title")}</h2>
      {#if c.plan.items}
        <p>{t("br-confirm-body", { entries: formatCount(c.plan.entries, loc), size: formatSize(c.plan.bytes, loc), count: c.ids.length })}</p>
        <ul class="stores">
          {#each c.plan.byStore as st (st.kind)}
            <li><span>{t(`br-store-${st.kind}`)}</span><span class="num">{t("br-confirm-store", { entries: formatCount(st.entries, loc), size: formatSize(st.bytes, loc) })}</span></li>
          {/each}
        </ul>
        {#if c.plan.indexFiles}<p class="muted small">{t("br-confirm-index", { count: c.plan.indexFiles })}</p>{/if}
      {:else}
        <p>{t("br-confirm-nothing")}</p>
      {/if}
      {#if c.plan.listedOnly}<p class="muted small">{t("br-confirm-listed", { size: formatSize(c.plan.listedOnly, loc) })}</p>{/if}
      {#if c.plan.running.length}<p class="warn small"><Icon name="alert" size={14} />{t("br-confirm-running", { apps: c.plan.running.join(", ") })}</p>{/if}
      <p class="muted small">{t("br-confirm-note")}</p>
      <footer>
        <button class="btn" onclick={() => (br.confirm = null)}>{t("delete-cancel")}</button>
        <button class="btn danger" data-br-go disabled={!c.plan.items} onclick={cleanSites} data-effect title={t("br-confirm-note")}><Icon name="clean" size={16} />{t("clean-confirm-go")}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .sites {
    margin-top: 10px;
    border-top: 1px solid var(--border);
    padding-top: 8px;
  }
  .disclose {
    font-weight: 600;
  }
  .chev {
    display: inline-flex;
    transition: transform 0.15s;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .intro {
    margin: 4px 0 8px;
    max-width: 90ch;
  }
  .find {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 8px;
  }
  .find input {
    flex: 1;
    max-width: 360px;
  }
  .small {
    font-size: 0.86em;
  }
  .btn.small {
    padding: 0.3em 0.65em;
  }
  .grow {
    flex: 1;
  }
  .running,
  .result {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 8px 10px;
    margin-bottom: 8px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .profile {
    margin: 8px 0;
  }
  .profile h3 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 6px 0;
    font-size: 0.98em;
  }
  .warn {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 2px 0 6px;
    color: var(--risk-caution);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 420px;
    overflow: auto;
  }
  .list li {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) auto;
    gap: 8px;
    align-items: center;
    padding: 5px 4px;
    border-top: 1px solid var(--border);
  }
  .list li:first-child {
    border-top: none;
  }
  .list li.off {
    opacity: 0.7;
  }
  .what {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .size {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }
  .note {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    margin: 4px 0 10px;
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
  .stores {
    list-style: none;
    margin: 8px 0;
    padding: 6px 8px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .stores li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 2px 0;
  }
  .dialog footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
</style>
