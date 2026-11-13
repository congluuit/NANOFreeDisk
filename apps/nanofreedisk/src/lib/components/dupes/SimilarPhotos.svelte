<script lang="ts">
  // "Similar photos" (docs/SIMILAR_PHOTOS.md): pictures that look alike without
  // being byte-identical, in groups with the suggested best photo first. Nothing is
  // ticked for the user: similar is not identical. "Select all but the best" ticks
  // a group on request (never a risky photo), a group can only lose every photo
  // with its own "remove all" tick, and the backend checks each photo again right
  // before the move (Recycle Bin, or the quarantine in Expert mode).
  import { untrack } from "svelte";
  import { pickFolder } from "../../api";
  import { destinationFor, removeKeys } from "../../clean/destination";
  import { reasonText } from "../../clean/state.svelte";
  import { formatCount, formatDate, formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { app, expert, fail } from "../../state.svelte";
  import { similarApi } from "../../similar/api";
  import { bestReason, folderOf, megapixels, nameOf, resolution, totals, wouldEmpty } from "../../similar/logic";
  import {
    changePreset,
    clearGroup,
    clearPicks,
    findSimilar,
    large,
    loadLarge,
    loadThumb,
    pickAllButBest,
    runSimilar,
    setKeepNone,
    similar,
    togglePhoto,
  } from "../../similar/state.svelte";
  import type { Photo, Preset, SimilarAction, SimilarSkip } from "../../similar/types";
  import Icon from "../Icon.svelte";

  const loc = $derived(intlLocale());
  const PRESETS: Preset[] = ["strict", "normal", "loose"];
  const report = $derived(similar.report);
  const groups = $derived(report?.groups.slice(0, similar.shown) ?? []);
  const sum = $derived(totals(report?.groups ?? [], similar.picks));
  const skips = $derived(
    Object.entries(report?.skipped ?? {})
      .filter(([, n]) => (n ?? 0) > 0)
      .map(([reason, n]) => ({ reason: reason as SimilarSkip, count: n ?? 0, samples: (report?.skippedSamples ?? []).filter((s) => s.reason === reason).slice(0, 3).map((s) => s.path) })),
  );
  const skipTotal = $derived(skips.reduce((s, l) => s + l.count, 0));
  const busy = $derived(!!similar.searching || !!similar.running);
  // where removed photos go: the same rule as identical files (Simple mode: the
  // quarantine, restorable for 7 days)
  const dest = $derived(destinationFor("dupes", expert()));
  let acknowledged = $state(false);

  // grid previews of the groups on screen (the backend decodes two at a time)
  $effect(() => {
    for (const g of groups) for (const p of g.photos) untrack(() => loadThumb(p.path));
  });

  // the side-by-side view: the chosen photo next to the group's best one
  const compared = $derived.by(() => {
    const c = similar.compare;
    const g = c ? report?.groups.find((x) => x.id === c.group) : undefined;
    if (!c || !g || g.photos.length < 2) return null;
    const best = g.photos[0];
    const other = c.path === best.path ? g.photos[1] : (g.photos.find((p) => p.path === c.path) ?? g.photos[1]);
    return { group: g, best, other };
  });
  $effect(() => {
    const c = compared;
    if (c) untrack(() => void loadLarge([c.best.path, c.other.path]));
  });

  async function addFolder() {
    const p = await pickFolder(t("dupes-pick-folder"));
    if (p && !similar.folders.includes(p)) similar.folders = [...similar.folders, p];
    similar.scope = "folders";
  }
  function addDrive(root: string) {
    if (!similar.folders.includes(root)) similar.folders = [...similar.folders, root];
    similar.scope = "folders";
  }
  function removeFolder(p: string) {
    similar.folders = similar.folders.filter((x) => x !== p);
  }
  function phaseText(): string {
    const p = similar.searching;
    if (!p) return "";
    if (p.phase === "scan") return t("similar-phase-scan", { files: formatCount(p.done, loc) });
    return t(`similar-phase-${p.phase}`, { done: formatCount(p.done, loc), total: formatCount(p.total, loc) });
  }
  function ratio(): number {
    const p = similar.searching;
    return p && p.total ? Math.min(1, p.done / p.total) : 0;
  }
  function flagsOf(p: Photo): string[] {
    const out: string[] = [];
    if (p.flags.cloud) out.push("cloud");
    if (p.flags.git) out.push("git");
    if (p.flags.appDir) out.push("app");
    if (p.flags.readonly) out.push("readonly");
    if (p.flags.clone) out.push("clone");
    return out;
  }
  const picked = (group: number, path: string): boolean => similar.picks[group]?.remove.includes(path) ?? false;
  function ask(action: SimilarAction) {
    acknowledged = false;
    similar.confirm = action;
  }
  async function go() {
    const action = similar.confirm;
    similar.confirm = null;
    if (action) await runSimilar(action);
  }
  function onKey(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    if (similar.compare) similar.compare = null;
    else if (similar.confirm) similar.confirm = null;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="similar" data-similar>
  <div class="card notice" data-similar-notice>
    <Icon name="alert" size={20} />
    <div>
      <strong>{t("similar-notice-title")}</strong>
      <p class="small">{t("similar-notice-body")}</p>
    </div>
  </div>

  <div class="panel scope">
    <div class="row wrap">
      {#if similar.searching}
        <button class="btn" onclick={() => similarApi.cancel().catch(fail)}>{t("clean-cancel")}</button>
      {:else}
        <button class="btn primary big" data-similar-find-mine onclick={() => void findSimilar("personal")} disabled={busy} data-effect title={t("similar-find-mine-hint")}>
          <Icon name="search" size={18} />{report ? t("similar-find-mine-again") : t("similar-find-mine")}
        </button>
        <button class="btn" onclick={() => (similar.scope = "folders")} disabled={busy} data-effect title={t("similar-choose-hint")}><Icon name="folder" size={16} />{t("similar-choose")}</button>
        {#if app.summary}
          <button class="btn ghost" onclick={() => void findSimilar("current")} disabled={busy} data-effect title={app.summary.root}>{t("similar-use-scan")}</button>
        {/if}
      {/if}
      <span class="grow"></span>
      <label class="field" title={t(`similar-preset-${similar.preset}-hint`)}>
        {t("similar-preset")}
        <select value={similar.preset} onchange={(e) => void changePreset((e.currentTarget as HTMLSelectElement).value as Preset)} disabled={busy} data-similar-preset data-effect>
          {#each PRESETS as p (p)}<option value={p}>{t(`similar-preset-${p}`)}</option>{/each}
        </select>
      </label>
    </div>
    <p class="muted small">{t(`similar-preset-${similar.preset}-hint`)}</p>
    {#if similar.scope === "folders"}
      <div class="row wrap">
        {#each similar.folders as f (f)}
          <span class="chip mono" title={f}>{f}<button class="x" onclick={() => removeFolder(f)} aria-label={t("dupes-remove-folder")}><Icon name="x" size={12} /></button></span>
        {/each}
        <button class="btn" onclick={addFolder} disabled={busy}><Icon name="folder" size={16} />{t("dupes-add-folder")}</button>
        {#each app.volumes.filter((v) => v.ready && (v.kind === "fixed" || v.kind === "removable")) as v (v.root)}
          <button class="btn ghost" onclick={() => addDrive(v.root)} disabled={busy || similar.folders.includes(v.root)}><Icon name="drive" size={16} />{v.root}</button>
        {/each}
        <span class="grow"></span>
        <button class="btn primary" data-similar-find onclick={() => void findSimilar("folders")} disabled={busy || !similar.folders.length} data-effect><Icon name="search" size={16} />{t("similar-find")}</button>
      </div>
    {/if}
    {#if expert()}
      <label class="field" title={t("dupes-app-dirs-hint")}><input type="checkbox" bind:checked={similar.includeAppDirs} disabled={busy} />{t("dupes-app-dirs")}</label>
    {/if}
  </div>

  {#if similar.searching}
    <div class="card running" aria-live="polite">
      <div class="row"><strong>{phaseText()}</strong></div>
      <div class="bar"><span style="width: {ratio() * 100}%"></span></div>
      {#if similar.searching.path}<p class="muted small ellipsis mono">{similar.searching.path}</p>{/if}
    </div>
  {:else if similar.running}
    <div class="card running" aria-live="polite">
      <div class="row">
        <strong>{t("clean-running", { done: similar.running.done, total: similar.running.total })}</strong>
        <span class="muted">{t("clean-freed-so-far", { size: formatSize(similar.running.freed, loc) })}</span>
      </div>
      <div class="bar"><span style="width: {similar.running.total ? (similar.running.done / similar.running.total) * 100 : 0}%"></span></div>
      <p class="muted small ellipsis mono">{similar.running.path}</p>
    </div>
  {:else if similar.last}
    {@const r = similar.last}
    <div class="card result" class:bad={r.failed > 0}>
      <Icon name={r.failed ? "alert" : "check"} size={22} />
      <div class="grow">
        <strong>{t("clean-done-title", { size: formatSize(r.freed, loc) })}</strong>
        <p class="muted small">{t("clean-done-detail", { done: r.done, partial: r.partial, skipped: r.skipped, failed: r.failed })}</p>
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
      <button class="btn ghost icon" onclick={() => (similar.last = null)} aria-label={t("clean-dismiss")}><Icon name="x" size={16} /></button>
    </div>
  {/if}

  {#if report && !similar.searching}
    <div class="card headline" data-similar-summary>
      {#if report.totalGroups}
        <h2>{t("similar-headline", { count: report.totalGroups, size: formatSize(report.extraBytes, loc) })}</h2>
        <p>{t("similar-headline-note")}</p>
      {:else}
        <h2>{t("similar-headline-none")}</h2>
      {/if}
      <p class="muted small">
        {t("similar-stats", { images: formatCount(report.images, loc), decoded: formatCount(report.decoded, loc), cached: formatCount(report.cacheHits, loc), seconds: (report.elapsedMs / 1000).toFixed(1) })}
        {#if report.totalGroups > report.groups.length}· {t("dupes-capped", { count: formatCount(report.groups.length, loc) })}{/if}
      </p>
    </div>

    {#if skips.length}
      <details class="card skipped" data-similar-skipped>
        <summary><Icon name="info" size={16} />{t("similar-not-compared", { count: formatCount(skipTotal, loc) })}</summary>
        <ul>
          {#each skips as l (l.reason)}
            <li>
              <div><strong>{t(`similar-skip-${l.reason}`)}</strong> <span class="muted">{formatCount(l.count, loc)}</span></div>
              <p class="muted small">{t(`similar-skip-${l.reason}-why`, { size: formatSize(report.minSize, loc) })}</p>
              {#each l.samples as p (p)}<span class="mono small muted ellipsis sample" title={p}>{p}</span>{/each}
            </li>
          {/each}
        </ul>
      </details>
    {/if}

    {#each groups as g (g.id)}
      {@const pick = similar.picks[g.id]}
      <div class="card group" data-similar-group={g.id}>
        <div class="ghead">
          <strong>{t("similar-group-title", { count: g.photos.length })}</strong>
          <span class="muted small">{t(`similar-best-${bestReason(g)}`)}</span>
          <span class="grow"></span>
          {#if pick?.remove.length}<span class="tag sel">{t("similar-group-picked", { count: pick.remove.length })}</span>{/if}
          <b class="num" title={t("similar-extra-hint")}>{formatSize(g.extra, loc)}</b>
          <button class="btn small" data-all-but-best onclick={() => pickAllButBest(g.id)} disabled={busy} data-effect title={t("similar-all-but-best-hint")}>{t("similar-all-but-best")}</button>
          {#if pick?.remove.length}<button class="btn small ghost" onclick={() => clearGroup(g.id)} disabled={busy}>{t("similar-untick")}</button>{/if}
        </div>
        <ul class="tiles">
          {#each g.photos as p, i (p.path)}
            {@const on = picked(g.id, p.path)}
            <li class="tile" class:best={i === 0} class:on data-photo={nameOf(p.path)}>
              <button class="pic" onclick={() => togglePhoto(g.id, p.path)} aria-pressed={on} disabled={busy} title={on ? t("similar-tile-untick") : t("similar-tile-tick")}>
                {#if similar.thumbs[p.path]}
                  <img src={similar.thumbs[p.path]} alt={nameOf(p.path)} />
                {:else}
                  <span class="ph"><Icon name="grid" size={22} /></span>
                {/if}
                {#if i === 0}<span class="badge best-badge">{t("similar-best")}</span>{/if}
                {#if on}<span class="badge gone-badge">{t("similar-remove")}</span>{/if}
              </button>
              <label class="tick">
                <input type="checkbox" checked={on} onchange={() => togglePhoto(g.id, p.path)} disabled={busy} data-effect />
                <span class="small">{t("similar-remove")}</span>
              </label>
              <div class="meta">
                <span class="small"><b>{resolution(p)}</b> <span class="muted">({t("similar-mp", { mp: megapixels(p, loc) })})</span></span>
                <span class="small">{formatSize(p.size, loc)} · {formatDate(p.mtime, loc)}</span>
                <span class="small ellipsis name" title={p.path}>{nameOf(p.path)}</span>
                <span class="mono small muted ellipsis" title={p.path}>{folderOf(p.path)}</span>
                <span class="flags">
                  {#if p.identicalTo}<span class="tag same" title={t("similar-identical-hint", { name: nameOf(p.identicalTo) })}>{t("similar-identical")}</span>{/if}
                  {#each flagsOf(p) as f (f)}<span class="tag risky" title={t(`dupes-flag-${f}-hint`)}>{t(`dupes-flag-${f}`)}</span>{/each}
                </span>
              </div>
              <div class="tools">
                <button class="btn ghost icon" onclick={() => (similar.compare = { group: g.id, path: p.path })} title={t("similar-compare")} aria-label={t("similar-compare")}><Icon name="zoom" size={14} /></button>
                <button class="btn ghost icon" onclick={() => similarApi.reveal(p.path).catch(fail)} title={t("dev-reveal")} aria-label={t("dev-reveal")}><Icon name="reveal" size={14} /></button>
              </div>
            </li>
          {/each}
        </ul>
        {#if pick && wouldEmpty(g, pick.remove)}
          <div class="warn small" data-keep-none>
            <p><Icon name="alert" size={16} />{t("similar-all-ticked")}</p>
            <label><input type="checkbox" checked={pick.keepNone} onchange={(e) => setKeepNone(g.id, (e.currentTarget as HTMLInputElement).checked)} disabled={busy} />{t("similar-keep-none")}</label>
          </div>
        {/if}
      </div>
    {:else}
      {#if report.totalGroups === 0}
        <div class="card empty"><Icon name="check" size={22} /><span>{t("similar-none")}</span></div>
      {/if}
    {/each}
    {#if report.groups.length > similar.shown}
      <button class="btn ghost more" onclick={() => (similar.shown += 30)}>{t("dupes-more", { count: formatCount(report.groups.length - similar.shown, loc) })}</button>
    {/if}

    {#if sum.files || sum.blocked}
      <div class="card actionbar" data-similar-actions>
        <div class="grow">
          <strong>{t("similar-picked", { count: sum.files, size: formatSize(sum.bytes, loc) })}</strong>
          {#if sum.blocked}<p class="warn-text small">{t("similar-blocked", { count: sum.blocked })}</p>{/if}
        </div>
        <button class="btn ghost" onclick={clearPicks} disabled={busy}>{t("similar-untick-all")}</button>
        {#if expert() && dest === "recycle-bin"}
          <button class="btn" disabled={!sum.files || busy} onclick={() => ask("quarantine")} data-effect title={t("dupes-quarantine-hint")}><Icon name="shield" size={16} />{t("dupes-quarantine")}</button>
        {/if}
        <button class="btn danger" data-similar-remove disabled={!sum.files || busy} onclick={() => ask(dest === "quarantine" ? "quarantine" : "recycle")} data-effect title={t(removeKeys(dest).hint)}>
          <Icon name="trash" size={16} />{t(removeKeys(dest).bar, { count: sum.files, size: formatSize(sum.bytes, loc) })}
        </button>
      </div>
    {/if}
  {/if}
</div>

{#if similar.confirm}
  {@const action = similar.confirm}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (similar.confirm = null)} onkeydown={onKey}>
    <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="similar-confirm-title">
      <h2 id="similar-confirm-title"><Icon name={action === "recycle" ? "trash" : "shield"} size={20} />{t(`similar-confirm-${action}`)}</h2>
      <p>{t("similar-confirm-body", { files: sum.files, groups: sum.groups, size: formatSize(sum.bytes, loc) })}</p>
      <p class="muted small">{t("similar-confirm-note")}</p>
      {#if sum.emptied}<p class="warn"><Icon name="alert" size={16} />{t("similar-confirm-emptied", { count: sum.emptied })}</p>{/if}
      {#if sum.risky.length}
        <div class="warn">
          <p><Icon name="alert" size={16} />{t("dupes-confirm-risky", { count: sum.risky.length })}</p>
          <ul>
            {#each sum.risky.slice(0, 8) as p (p.path)}
              <li class="small"><span class="mono ellipsis" title={p.path}>{p.path}</span>{#each flagsOf(p) as f (f)}<span class="tag risky">{t(`dupes-flag-${f}`)}</span>{/each}</li>
            {/each}
          </ul>
          <label class="small"><input type="checkbox" bind:checked={acknowledged} />{t("dupes-confirm-ack")}</label>
        </div>
      {/if}
      <footer>
        <button class="btn" onclick={() => (similar.confirm = null)}>{t("delete-cancel")}</button>
        <button class="btn danger" data-similar-confirm onclick={go} disabled={sum.risky.length > 0 && !acknowledged} data-effect title={t("similar-confirm-note")}>
          <Icon name={action === "recycle" ? "trash" : "shield"} size={16} />{t(`dupes-go-${action}`)}
        </button>
      </footer>
    </div>
  </div>
{/if}

{#if compared}
  {@const c = compared}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (similar.compare = null)} onkeydown={onKey}>
    <div class="dialog card wide" role="dialog" aria-modal="true" aria-labelledby="similar-compare-title" data-similar-compare>
      <h2 id="similar-compare-title"><Icon name="zoom" size={20} />{t("similar-compare-title")}</h2>
      <div class="pair">
        {#each [c.best, c.other] as p, i (p.path)}
          <figure>
            <div class="frame">
              {#if large.urls[p.path]}
                <img src={large.urls[p.path]} alt={nameOf(p.path)} />
              {:else if similar.thumbs[p.path]}
                <img src={similar.thumbs[p.path]} alt={nameOf(p.path)} />
              {:else}
                <span class="ph"><Icon name="grid" size={28} /></span>
              {/if}
            </div>
            <figcaption>
              <strong>{i === 0 ? t("similar-best") : nameOf(p.path)}</strong>
              <span class="small">{resolution(p)} · {formatSize(p.size, loc)} · {formatDate(p.mtime, loc)}</span>
              <span class="mono small muted ellipsis" title={p.path}>{p.path}</span>
              {#if i > 0}
                <label class="small"><input type="checkbox" checked={picked(c.group.id, p.path)} onchange={() => togglePhoto(c.group.id, p.path)} disabled={busy} />{t("similar-remove")}</label>
              {/if}
            </figcaption>
          </figure>
        {/each}
      </div>
      {#if c.group.photos.length > 2}
        <div class="row wrap others">
          {#each c.group.photos.slice(1) as p (p.path)}
            <button class="btn small" class:on={p.path === c.other.path} onclick={() => (similar.compare = { group: c.group.id, path: p.path })}>{nameOf(p.path)}</button>
          {/each}
        </div>
      {/if}
      <footer>
        <button class="btn" onclick={() => (similar.compare = null)}>{t("similar-close")}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .similar {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .notice {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px 16px;
    border-left: 4px solid var(--risk-caution);
    color: var(--risk-caution-ink);
    background: var(--risk-caution-soft);
  }
  .notice p {
    margin: 2px 0 0;
  }
  .scope {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .scope > p {
    margin: 0;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: center;
    min-width: 0;
  }
  .wrap {
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
  }
  .btn.big {
    padding: 10px 18px;
    font-size: 1.05em;
  }
  .field {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .field select {
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 360px;
    padding: 3px 4px 3px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-2);
    overflow: hidden;
    white-space: nowrap;
  }
  .chip .x {
    border: none;
    background: none;
    padding: 2px;
    color: var(--text-muted);
  }
  .running,
  .result,
  .empty,
  .headline {
    padding: 14px 16px;
  }
  .running .bar {
    margin: 10px 0 6px;
    height: 8px;
  }
  .running p,
  .result p,
  .headline p {
    margin: 2px 0 0;
  }
  .result {
    display: flex;
    gap: 12px;
    align-items: center;
    border-left: 4px solid var(--risk-safe);
  }
  .result.bad {
    border-left-color: var(--danger);
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
  .headline {
    border-left: 4px solid var(--risk-caution);
  }
  .headline h2 {
    margin: 0 0 4px;
    font-size: 1.2em;
  }
  .skipped {
    padding: 10px 16px;
  }
  .skipped > summary {
    display: flex;
    gap: 8px;
    align-items: center;
    cursor: pointer;
  }
  .skipped ul {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
  }
  .skipped li {
    display: flex;
    flex-direction: column;
    padding: 8px 0;
    border-top: 1px solid var(--border);
    min-width: 0;
  }
  .skipped li p {
    margin: 2px 0;
  }
  .sample {
    display: block;
  }
  .group {
    padding: 10px 14px 12px;
  }
  .ghead {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
  }
  .tag {
    margin: 0 2px;
  }
  .tag.sel {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
  .tag.risky {
    background: var(--risk-caution-soft);
    color: var(--risk-caution-ink);
  }
  .tag.same {
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 10px;
    list-style: none;
    margin: 10px 0 0;
    padding: 0;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    min-width: 0;
  }
  .tile.best {
    border-color: var(--risk-safe);
    background: var(--risk-safe-soft);
  }
  .tile.on {
    border-color: var(--danger);
  }
  .pic {
    position: relative;
    display: grid;
    place-items: center;
    height: 130px;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    overflow: hidden;
    cursor: pointer;
  }
  .pic img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .tile.on .pic img {
    opacity: 0.55;
  }
  .ph {
    color: var(--text-muted);
  }
  .badge {
    position: absolute;
    top: 6px;
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 0.8em;
    font-weight: 600;
  }
  .best-badge {
    left: 6px;
    background: var(--risk-safe);
    color: var(--surface);
  }
  .gone-badge {
    right: 6px;
    background: var(--danger);
    color: var(--surface);
  }
  .tick {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .meta {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .meta .name {
    font-weight: 600;
  }
  .flags {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    margin-top: 2px;
  }
  .tools {
    display: flex;
    justify-content: flex-end;
    gap: 2px;
  }
  .warn {
    margin: 10px 0 0;
    padding: 10px 12px;
    border-left: 4px solid var(--risk-caution);
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .warn p {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 0 0 6px;
  }
  .warn ul {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
  }
  .warn li,
  .warn label {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .warn-text {
    margin: 2px 0 0;
    color: var(--risk-caution-ink);
  }
  .empty {
    display: flex;
    gap: 10px;
    align-items: center;
    color: var(--text-muted);
  }
  .more {
    align-self: center;
  }
  .actionbar {
    position: sticky;
    bottom: 8px;
    z-index: 5;
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
    padding: 10px 14px;
    box-shadow: var(--shadow);
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
    width: min(640px, 94vw);
    max-height: 88vh;
    overflow: auto;
    padding: 22px 24px;
    box-shadow: var(--shadow);
  }
  .dialog.wide {
    width: min(1180px, 96vw);
  }
  .dialog h2 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0 0 8px;
    font-size: 1.2em;
  }
  .dialog footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .pair figure {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    min-width: 0;
  }
  .frame {
    display: grid;
    place-items: center;
    height: min(58vh, 560px);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    overflow: hidden;
  }
  .frame img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  figcaption {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  figcaption label {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .others {
    margin-top: 10px;
  }
  .others .btn.on {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
</style>
