<script lang="ts">
  // Duplicates (PHASE5_DUPLICATES §2.6): byte-identical files in the user's personal
  // folders, the Analyze scan or folders picked here, largest waste first. Simple
  // mode opens on one button (the personal folders) with the full form under
  // "Options"; Expert mode shows the form. One copy per group always stays, the
  // planner refuses anything else, and each removed copy and its keeper are hashed
  // again right before the move (Recycle Bin, or the quarantine in Expert mode;
  // Activity log, Restore).
  import { inTauri, pickFolder, pickSaveFile } from "../api";
  import { formatCount, formatDate, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, expert, fail, toast } from "../state.svelte";
  import { reasonText } from "../clean/state.svelte";
  import { dupesApi } from "../dupes/api";
  import {
    applyStrategy,
    cliCommand,
    copyId,
    dupes,
    filePickIds,
    findDupes,
    findPersonal,
    folderPickIds,
    folderTotals,
    linkPlan,
    loadThumb,
    minBytes,
    risky,
    runDupes,
    searchAgain,
    selectedTotals,
    setFilePicks,
    setFolderKeep,
    setFolderPicks,
    setKeep,
    setMinBytes,
    toggleFolderRemove,
    toggleRemove,
  } from "../dupes/state.svelte";
  import { splitSize, toBytes, type SizeUnit } from "../dupes/size";
  import { skipLines, type SkipAction } from "../dupes/skipped";
  import type { DupeAction, DupeGroup, FileCopy, FileKind, FolderCopy, KeepStrategy } from "../dupes/types";
  import { summarize, type SelItem } from "../selection";
  import CliHint from "../components/CliHint.svelte";
  import ConfirmTotals from "../components/ConfirmTotals.svelte";
  import GroupCheck from "../components/GroupCheck.svelte";
  import Icon from "../components/Icon.svelte";
  import SelectionBar from "../components/SelectionBar.svelte";
  import StatCard from "../components/StatCard.svelte";

  const loc = $derived(intlLocale());
  const KINDS: FileKind[] = ["all", "images", "videos", "music", "documents", "archives"];
  const STRATEGIES: KeepStrategy[] = ["auto", "oldest", "newest", "shortest"];
  const IMAGE = /\.(jpe?g|png|gif|webp|bmp)$/i;

  // before the first search the form offers the scan in Analyze when there is one
  // (the one-click button always searches the personal folders)
  if (!dupes.report && !dupes.searching && !dupes.lastScope && dupes.scope === "personal" && app.summary) dupes.scope = "current";
  $effect(() => {
    // the first visit follows the saved keep strategy
    if (!dupes.report && !dupes.searching) {
      dupes.strategy = (app.settings.dupeKeep as KeepStrategy) ?? "auto";
      if (!app.summary && dupes.scope === "current") dupes.scope = "personal";
    }
  });

  // the size limit: a number in KB or MB, saved as bytes
  let unit = $state<SizeUnit>(splitSize(minBytes()).unit);
  const sizeValue = $derived(Math.round((minBytes() / (unit === "MB" ? 2 ** 20 : 1024)) * 100) / 100);
  function onSize(e: Event) {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    if (v > 0) setMinBytes(toBytes(v, unit));
  }
  function onUnit(e: Event) {
    const u = (e.currentTarget as HTMLSelectElement).value as SizeUnit;
    setMinBytes(toBytes(sizeValue, u));
    unit = u;
  }
  function chooseFolders() {
    dupes.scope = "folders";
    dupes.optionsOpen = true;
  }
  function skipAction(a: SkipAction) {
    if (a.kind === "lower") {
      unit = splitSize(a.bytes).unit;
      void searchAgain(() => setMinBytes(a.bytes));
    } else if (a.kind === "app-dirs") void searchAgain(() => (dupes.includeAppDirs = true));
    else if (a.kind === "all-types") void searchAgain(() => (dupes.kind = "all"));
    else void searchAgain();
  }
  function actionLabel(a: SkipAction): string {
    if (a.kind === "lower") return t("dupes-skip-act-lower", { size: formatSize(a.bytes, loc) });
    return t(`dupes-skip-act-${a.kind}`);
  }

  const report = $derived(dupes.report);
  const groups = $derived(report?.groups.slice(0, dupes.shown) ?? []);
  const skips = $derived(report ? skipLines(report) : []);
  const skipTotal = $derived(skips.reduce((sum, l) => sum + l.count, 0));
  /** Copies beyond the first of every group (what removing the extras means). */
  const extraCopies = $derived((report?.groups ?? []).reduce((sum, g) => sum + g.copies.length - 1, 0));
  const rootNames = $derived((report?.roots ?? []).map((r) => nameOf(r.replace(/[\\/]+$/, "")) || r).join(", "));
  const totals = $derived.by(() => {
    void dupes.picks;
    return selectedTotals();
  });
  const ftotals = $derived.by(() => {
    void dupes.folderPicks;
    return folderTotals();
  });
  const folderTab = $derived(dupes.tab === "folders");
  let page = $state<HTMLElement | null>(null);
  // one selection item per copy; the copy a group keeps (and protected folders) never
  // tick, so "select all" always leaves one copy of everything
  const fileItems = $derived<SelItem[]>(
    (report?.groups ?? []).flatMap((g) =>
      g.copies.map((c) => ({ id: copyId(g.id, c.path), bytes: g.size, files: 1, level: risky(c) ? "caution" : "safe", group: String(g.id), locked: dupes.picks[g.id]?.keep === c.path })),
    ),
  );
  const folderItems = $derived<SelItem[]>(
    (report?.folderGroups ?? []).flatMap((g) =>
      g.folders.map((f) => ({ id: copyId(g.id, f.path), bytes: g.size, files: g.files, level: risky(f) ? "caution" : "safe", group: `f${g.id}`, locked: f.blocked || dupes.folderPicks[g.id]?.keep === f.path })),
    ),
  );
  const filePicked = $derived.by(() => {
    void dupes.picks;
    return filePickIds();
  });
  const folderPicked = $derived.by(() => {
    void dupes.folderPicks;
    return folderPickIds();
  });
  const confirmSummary = $derived(folderTab ? summarize(folderItems, folderPicked, true) : summarize(fileItems, filePicked, true));
  let acknowledged = $state(false);
  /** "Replace with links": also hard-link types that may be edited in place. */
  let otherTypes = $state(false);
  const links = $derived.by(() => {
    void dupes.picks;
    return linkPlan(otherTypes);
  });

  async function addFolder() {
    const p = await pickFolder(t("dupes-pick-folder"));
    if (p && !dupes.folders.includes(p)) dupes.folders = [...dupes.folders, p];
    dupes.scope = "folders";
  }
  function addDrive(root: string) {
    if (!dupes.folders.includes(root)) dupes.folders = [...dupes.folders, root];
    dupes.scope = "folders";
  }
  function removeFolder(p: string) {
    dupes.folders = dupes.folders.filter((x) => x !== p);
  }
  function toggle(g: DupeGroup) {
    dupes.expanded = dupes.expanded.includes(g.id) ? dupes.expanded.filter((x) => x !== g.id) : [...dupes.expanded, g.id];
    if (IMAGE.test(g.copies[0]?.path ?? "")) void loadThumb(g.copies[0].path);
  }
  function phaseText(): string {
    const p = dupes.searching;
    if (!p) return "";
    if (p.phase === "scan") return t("dupes-phase-scan", { files: formatCount(p.done, loc) });
    return t(`dupes-phase-${p.phase}`, { done: formatCount(p.done, loc), total: formatCount(p.total, loc) });
  }
  function ratio(): number {
    const p = dupes.searching;
    if (!p) return 0;
    if (p.totalBytes > 0) return Math.min(1, p.bytes / p.totalBytes);
    return p.total ? p.done / p.total : 0;
  }
  function flagsOf(c: FileCopy | FolderCopy): string[] {
    const out: string[] = [];
    if (c.flags.cloud) out.push("cloud");
    if (c.flags.git) out.push("git");
    if (c.flags.appDir) out.push("app");
    if (c.flags.readonly) out.push("readonly");
    if (c.flags.clone) out.push("clone");
    return out;
  }
  function folderOf(path: string): string {
    const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
    return i > 0 ? path.slice(0, i) : path;
  }
  function nameOf(path: string): string {
    return path.slice(Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/")) + 1);
  }
  function ask(action: DupeAction) {
    acknowledged = false;
    otherTypes = false;
    dupes.confirm = action;
  }
  async function go() {
    const action = dupes.confirm;
    dupes.confirm = null;
    if (action === "link") await runDupes("link", links.selections);
    else if (action) await runDupes(action, undefined, folderTab);
  }
  async function exportAs(format: "json" | "csv") {
    try {
      const path = await pickSaveFile(t("dupes-export"), `nanofreedisk-duplicates.${format}`, format);
      if (!path) return;
      await dupesApi.export(path, format);
      toast(t("analyze-exported", { path }), "ok");
    } catch (e) {
      fail(e);
    }
  }
</script>

<section class="page dupes" bind:this={page} data-mode={expert() ? "expert" : "simple"}>
  <header class="page-head">
    <div>
      <h1>{t("dupes-title")}</h1>
      <p>{t("dupes-intro")}</p>
    </div>
    <div class="page-actions">
      <button class="btn ghost" onclick={() => (app.view = "activity")} title={t("clean-activity-hint")}>
        <Icon name="undo" />{t("nav-activity")}
      </button>
    </div>
  </header>

  {#snippet searchForm()}
    <div class="row wrap" role="radiogroup" aria-label={t("dupes-scope")}>
      <label class="radio" title={t("dupes-scope-personal-hint")}>
        <input type="radio" name="dupes-scope" value="personal" bind:group={dupes.scope} disabled={!!dupes.searching} />
        {t("dupes-scope-personal")}
      </label>
      <label class="radio" title={app.summary ? app.summary.root : t("dupes-scope-current-none")}>
        <input type="radio" name="dupes-scope" value="current" bind:group={dupes.scope} disabled={!app.summary || !!dupes.searching} />
        {t("dupes-scope-current")}
        {#if app.summary}<span class="mono muted ellipsis">{app.summary.root}</span>{/if}
      </label>
      <label class="radio">
        <input type="radio" name="dupes-scope" value="folders" bind:group={dupes.scope} disabled={!!dupes.searching} />
        {t("dupes-scope-folders")}
      </label>
    </div>
    {#if dupes.scope === "folders"}
      <div class="row wrap">
        {#each dupes.folders as f (f)}
          <span class="chip mono" title={f}>{f}<button class="x" onclick={() => removeFolder(f)} aria-label={t("dupes-remove-folder")}><Icon name="x" size={12} /></button></span>
        {/each}
        <button class="btn" onclick={addFolder} disabled={!!dupes.searching}><Icon name="folder" size={16} />{t("dupes-add-folder")}</button>
        {#each app.volumes.filter((v) => v.ready && (v.kind === "fixed" || v.kind === "removable")) as v (v.root)}
          <button class="btn ghost" onclick={() => addDrive(v.root)} disabled={!!dupes.searching || dupes.folders.includes(v.root)}><Icon name="drive" size={16} />{v.root}</button>
        {/each}
      </div>
    {/if}
    <div class="row wrap">
      <label class="field">
        {t("dupes-min-size")}
        <input type="number" min="0.01" step="any" value={sizeValue} onchange={onSize} disabled={!!dupes.searching} data-effect data-min-size title={t("dupes-min-size-hint")} />
        <select value={unit} onchange={onUnit} disabled={!!dupes.searching} data-effect aria-label={t("dupes-min-size-unit")} title={t("dupes-min-size-hint")}>
          <option value="KB">KB</option>
          <option value="MB">MB</option>
        </select>
      </label>
      <label class="field">
        {t("dupes-kind")}
        <select bind:value={dupes.kind} disabled={!!dupes.searching}>
          {#each KINDS as k (k)}<option value={k}>{t(`dupes-kind-${k}`)}</option>{/each}
        </select>
      </label>
      <label class="field" title={t("dupes-folders-hint")}><input type="checkbox" bind:checked={dupes.findFolders} disabled={!!dupes.searching} data-effect title={t("dupes-folders-hint")} />{t("dupes-folders")}</label>
      <label class="field" title={t("dupes-app-dirs-hint")}><input type="checkbox" bind:checked={dupes.includeAppDirs} disabled={!!dupes.searching} />{t("dupes-app-dirs")}</label>
      {#if expert()}
        <label class="field" title={t("dupes-paranoid-hint")}><input type="checkbox" bind:checked={dupes.paranoid} disabled={!!dupes.searching} />{t("dupes-paranoid")}</label>
      {/if}
      <span class="grow"></span>
      {#if dupes.searching}
        <button class="btn" onclick={() => dupesApi.cancel().catch(fail)}>{t("clean-cancel")}</button>
      {:else}
        <button
          class="btn primary"
          data-find
          onclick={() => void findDupes()}
          disabled={(dupes.scope === "current" && !app.summary) || (dupes.scope === "folders" && !dupes.folders.length)}
          data-effect
          title={t("dupes-find-hint")}
        ><Icon name="search" size={16} />{t("dupes-find")}</button>
      {/if}
    </div>
  {/snippet}

  {#if expert()}
    <div class="panel scope">
      <h2 class="panel-title">{t("dupes-scope")}</h2>
      {@render searchForm()}
    </div>
  {:else}
    <div class="panel scope start" data-dupes-start>
      {#if !report && !dupes.searching}
        <div>
          <h2 class="start-title">{t("dupes-start-title")}</h2>
          <p class="muted">{t("dupes-start-body")}</p>
        </div>
      {/if}
      <div class="row wrap">
        {#if dupes.searching}
          <button class="btn" onclick={() => dupesApi.cancel().catch(fail)}>{t("clean-cancel")}</button>
        {:else}
          <button class="btn primary big" data-find-personal onclick={() => void findPersonal()} data-effect title={t("dupes-find-mine-hint")}>
            <Icon name="search" size={18} />{report ? t("dupes-find-mine-again") : t("dupes-find-mine")}
          </button>
          <button class="btn" data-choose-folders onclick={chooseFolders} data-effect title={t("dupes-choose-hint")}><Icon name="folder" size={16} />{t("dupes-choose")}</button>
        {/if}
      </div>
      <details class="options" bind:open={dupes.optionsOpen}>
        <summary>{t("dupes-options")}</summary>
        <div class="options-body">{@render searchForm()}</div>
      </details>
    </div>
  {/if}

  {#if dupes.searching}
    <div class="card running" aria-live="polite">
      <div class="row"><strong>{phaseText()}</strong><span class="grow"></span><span class="muted">{formatSize(dupes.searching.bytes, loc)}</span></div>
      <div class="bar"><span style="width: {ratio() * 100}%"></span></div>
      {#if dupes.searching.path}<p class="muted small ellipsis mono">{dupes.searching.path}</p>{/if}
    </div>
  {:else if dupes.running}
    <div class="card running" aria-live="polite">
      <div class="row">
        <strong>{t("clean-running", { done: dupes.running.done, total: dupes.running.total })}</strong>
        <span class="muted">{t("clean-freed-so-far", { size: formatSize(dupes.running.freed, loc) })}</span>
      </div>
      <div class="bar"><span style="width: {dupes.running.total ? (dupes.running.done / dupes.running.total) * 100 : 0}%"></span></div>
      <p class="muted small ellipsis mono">{dupes.running.path}</p>
    </div>
  {:else if dupes.last}
    {@const r = dupes.last}
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
      <button class="btn ghost icon" onclick={() => (dupes.last = null)} aria-label={t("clean-dismiss")}><Icon name="x" size={16} /></button>
    </div>
  {/if}

  {#if report && !dupes.searching}
    {#if expert()}
      <div class="stats" data-dupes-summary>
        <StatCard label={t("stat-wasted")} tone="caution" value={formatSize(report.wastedBytes, loc)} unit={t("stat-in-groups", { count: report.totalGroups })} />
        <StatCard label={t("stat-compared")} value={formatCount(report.scannedFiles, loc)} unit={t("stat-compared-unit", { seconds: (report.elapsedMs / 1000).toFixed(1) })} />
      </div>
    {:else}
      <div class="card headline" data-dupes-summary>
        {#if report.totalGroups}
          <h2>{t("dupes-headline", { size: formatSize(report.wastedBytes, loc), count: extraCopies })}</h2>
          <p>{t(`dupes-preselect-${dupes.strategy}`)}</p>
        {:else}
          <h2>{t("dupes-headline-none")}</h2>
        {/if}
        {#if rootNames}<p class="muted small ellipsis" title={report.roots.join("\n")}>{t("dupes-searched-in", { folders: rootNames })}</p>{/if}
      </div>
    {/if}
    <div class="summary">
      <div class="grow">
        <p class="muted small">
          {t("dupes-stats", { files: formatCount(report.scannedFiles, loc), read: formatSize(report.hashedBytes, loc), cached: formatCount(report.cacheHits, loc), seconds: (report.elapsedMs / 1000).toFixed(1) })}
          {#if report.totalGroups > report.groups.length}· {t("dupes-capped", { count: formatCount(report.groups.length, loc) })}{/if}
        </p>
      </div>
      <label class="field" title={t("dupes-keep-hint")}>
        {t("dupes-keep")}
        <select value={dupes.strategy} onchange={(e) => applyStrategy((e.currentTarget as HTMLSelectElement).value as KeepStrategy)} data-effect title={t("dupes-keep-hint")}>
          {#each STRATEGIES as s (s)}<option value={s}>{t(`dupes-keep-${s}`)}</option>{/each}
        </select>
      </label>
      {#if expert()}
        <button class="btn ghost" onclick={() => exportAs("csv")} disabled={!inTauri} data-effect title={t("hint-export")}><Icon name="export" size={16} />CSV</button>
        <button class="btn ghost" onclick={() => exportAs("json")} disabled={!inTauri} data-effect title={t("hint-export")}><Icon name="export" size={16} />JSON</button>
      {/if}
    </div>

    {#if skips.length}
      <details class="card skipped" data-not-compared>
        <summary><Icon name="info" size={16} />{t("dupes-not-compared", { count: formatCount(skipTotal, loc) })}</summary>
        <ul>
          {#each skips as l (l.reason)}
            <li data-skip={l.reason}>
              <div class="why">
                <div><strong>{t(`dupes-skip-${l.reason}`)}</strong> <span class="muted">{formatCount(l.count, loc)}</span></div>
                <p class="muted small">{t(`dupes-skip-${l.reason}-why`, { size: formatSize(report.minSize, loc) })}</p>
                {#each l.samples as p (p)}<span class="mono small muted ellipsis sample" title={p}>{p}</span>{/each}
              </div>
              {#if l.action}
                <button class="btn small" onclick={() => l.action && skipAction(l.action)} data-effect title={t(`dupes-skip-act-${l.action.kind}-hint`)}>{actionLabel(l.action)}</button>
              {/if}
            </li>
          {/each}
        </ul>
      </details>
    {/if}

    {#if dupes.findFolders || report.folderGroups?.length}
      <div class="tabrow" role="tablist">
        <button role="tab" class="tab" class:on={!folderTab} aria-selected={!folderTab} data-dupetab="files" onclick={() => (dupes.tab = "files")}>
          <Icon name="file" size={16} />{t("dupes-tab-files", { count: report.totalGroups })}
        </button>
        <button role="tab" class="tab" class:on={folderTab} aria-selected={folderTab} data-dupetab="folders" onclick={() => (dupes.tab = "folders")}>
          <Icon name="folder" size={16} />{t("dupes-tab-folders", { count: report.folderGroups?.length ?? 0 })}
        </button>
      </div>
    {/if}

    {#if folderTab}
      {#each report.folderGroups ?? [] as g (g.id)}
        {@const pick = dupes.folderPicks[g.id]}
        <div class="card group" data-folder-group={g.id}>
          <div class="head static">
            <GroupCheck items={folderItems} group={`f${g.id}`} picked={folderPicked} onchange={setFolderPicks} expert name={nameOf(g.folders[0]?.path ?? "")} busy={!!dupes.running} />
            <Icon name="folder" size={16} />
            <span class="name ellipsis" title={g.folders[0]?.path}>{nameOf(g.folders[0]?.path ?? "")}</span>
            <span class="muted small">{t("dupes-folder-copies", { count: g.folders.length, files: formatCount(g.files, loc), size: formatSize(g.size, loc) })}</span>
            <span class="grow"></span>
            {#if pick?.remove.length}<span class="tag sel">{t("dupes-group-selected", { count: pick.remove.length })}</span>{/if}
            <b class="num">{formatSize(g.wasted, loc)}</b>
          </div>
          <ul class="copies">
            {#each g.folders as f (f.path)}
              {@const keep = pick?.keep === f.path}
              <li class:keep>
                <label class="keepbox" title={t("dupes-keep-this-hint")}>
                  <input type="radio" name="fkeep-{g.id}" checked={keep} onchange={() => setFolderKeep(g.id, f.path)} disabled={!!dupes.running} data-effect title={t("dupes-keep-this-hint")} />
                  <span class="small">{t("dupes-keep-this")}</span>
                </label>
                <label class="rmbox" title={f.blocked ? t("dupes-folder-protected") : keep ? t("dupes-is-keeper") : t("dupes-remove-hint")}>
                  <input type="checkbox" checked={pick?.remove.includes(f.path) ?? false} disabled={keep || f.blocked || !!dupes.running} onchange={() => toggleFolderRemove(g.id, f.path)} data-effect title={f.blocked ? t("dupes-folder-protected") : keep ? t("dupes-is-keeper") : t("dupes-remove-hint")} />
                  <span class="small">{t("dupes-remove")}</span>
                </label>
                <div class="where">
                  <span class="mono ellipsis" title={f.path}>{f.path}</span>
                  <span class="muted small">{formatDate(f.mtime, loc)}</span>
                </div>
                <div class="flags">
                  {#if f.blocked}<span class="tag app" title={t("dupes-folder-protected")}>{t("dupes-flag-protected")}</span>{/if}
                  {#each flagsOf(f) as fl (fl)}<span class="tag {fl}" title={t(`dupes-flag-${fl}-hint`)}>{t(`dupes-flag-${fl}`)}</span>{/each}
                </div>
                <button class="btn ghost icon" onclick={() => dupesApi.reveal(f.path).catch(fail)} title={t("dev-reveal")} aria-label={t("dev-reveal")}><Icon name="reveal" size={14} /></button>
              </li>
            {/each}
          </ul>
        </div>
      {:else}
        <div class="card empty"><Icon name="folder" size={22} /><span>{dupes.findFolders ? t("dupes-folders-none") : t("dupes-folders-off")}</span></div>
      {/each}
      <CliHint command={cliCommand()} />
      <SelectionBar items={folderItems} picked={folderPicked} onchange={setFolderPicks} expert busy={!!dupes.running} scope={page} primary>
        {#if expert()}
          <button class="btn" disabled={!ftotals.folders || !!dupes.running} onclick={() => ask("quarantine")} data-effect title={t("dupes-quarantine-hint")}><Icon name="shield" size={16} />{t("dupes-quarantine")}</button>
        {/if}
        <button class="btn danger" data-recycle-folders disabled={!ftotals.folders || !!dupes.running} onclick={() => ask("recycle")} data-effect title={t("dupes-recycle-hint")}>
          <Icon name="trash" size={16} />{t("sel-recycle-n", { count: ftotals.folders, size: formatSize(ftotals.bytes, loc) })}
        </button>
      </SelectionBar>
    {:else}
    {#each groups as g (g.id)}
      {@const pick = dupes.picks[g.id]}
      {@const open = dupes.expanded.includes(g.id)}
      <div class="card group" data-group={g.id}>
        <div class="headrow">
        <GroupCheck items={fileItems} group={String(g.id)} picked={filePicked} onchange={setFilePicks} expert name={nameOf(g.copies[0]?.path ?? "")} busy={!!dupes.running} />
        <button class="head" onclick={() => toggle(g)} aria-expanded={open}>
          <Icon name="chevron" size={14} />
          <span class="name ellipsis" title={pick?.keep ?? g.copies[0]?.path}>{nameOf(pick?.keep ?? g.copies[0]?.path ?? "")}</span>
          <span class="muted small nowrap">{t("dupes-copies", { count: g.copies.length, size: formatSize(g.size, loc) })}</span>
          {#if !open && pick?.keep}<span class="muted small ellipsis keeps" title={pick.keep}>{t("dupes-keeps", { folder: folderOf(pick.keep) })}</span>{/if}
          <span class="grow"></span>
          {#if pick?.remove.length}<span class="tag sel">{t("dupes-group-selected", { count: pick.remove.length })}</span>{/if}
          <b class="num">{formatSize(g.wasted, loc)}</b>
        </button>
        </div>
        {#if open}
          {#if dupes.thumbs[g.copies[0].path]}
            <img class="thumb" src={dupes.thumbs[g.copies[0].path]} alt="" />
          {/if}
          <ul class="copies">
            {#each g.copies as c (c.path)}
              {@const keep = pick?.keep === c.path}
              <li class:keep>
                <label class="keepbox" title={t("dupes-keep-this-hint")}>
                  <input type="radio" name="keep-{g.id}" checked={keep} onchange={() => setKeep(g.id, c.path)} disabled={!!dupes.running} data-effect title={t("dupes-keep-this-hint")} />
                  <span class="small">{t("dupes-keep-this")}</span>
                </label>
                <label class="rmbox" title={keep ? t("dupes-is-keeper") : t("dupes-remove-hint")}>
                  <input type="checkbox" checked={pick?.remove.includes(c.path) ?? false} disabled={keep || !!dupes.running} onchange={() => toggleRemove(g.id, c.path)} data-effect title={keep ? t("dupes-is-keeper") : t("dupes-remove-hint")} />
                  <span class="small">{t("dupes-remove")}</span>
                </label>
                <div class="where">
                  <span class="mono ellipsis" title={c.path}>{folderOf(c.path)}</span>
                  <span class="muted small">{nameOf(c.path)} · {formatDate(c.mtime, loc)}{c.links?.length ? ` · ${t("dupes-links", { count: c.links.length })}` : ""}</span>
                </div>
                <div class="flags">
                  {#each flagsOf(c) as f (f)}<span class="tag {f}" title={t(`dupes-flag-${f}-hint`)}>{t(`dupes-flag-${f}`)}</span>{/each}
                </div>
                <button class="btn ghost icon" onclick={() => dupesApi.reveal(c.path).catch(fail)} title={t("dev-reveal")} aria-label={t("dev-reveal")}><Icon name="reveal" size={14} /></button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {:else}
      <div class="card empty"><Icon name="check" size={22} /><span>{t("dupes-none")}</span></div>
    {/each}
    {#if report.groups.length > dupes.shown}
      <button class="btn ghost more" onclick={() => (dupes.shown += 100)}>{t("dupes-more", { count: formatCount(report.groups.length - dupes.shown, loc) })}</button>
    {/if}

    <CliHint command={cliCommand()} />
    <SelectionBar items={fileItems} picked={filePicked} onchange={setFilePicks} expert busy={!!dupes.running} scope={page} primary>
      {#if expert()}
        <button class="btn" data-link disabled={!totals.files || !!dupes.running} onclick={() => ask("link")} data-effect title={t("dupes-link-hint")}><Icon name="link" size={16} />{t("dupes-link")}</button>
        <button class="btn" disabled={!totals.files || !!dupes.running} onclick={() => ask("quarantine")} data-effect title={t("dupes-quarantine-hint")}><Icon name="shield" size={16} />{t("dupes-quarantine")}</button>
      {/if}
      <button class="btn danger" data-recycle disabled={!totals.files || !!dupes.running} onclick={() => ask("recycle")} data-effect title={t("dupes-recycle-hint")}>
        <Icon name="trash" size={16} />{t("sel-recycle-n", { count: totals.files, size: formatSize(totals.bytes, loc) })}
      </button>
    </SelectionBar>
    {/if}
  {:else if !dupes.searching && expert()}
    <div class="card empty"><Icon name="duplicates" size={22} /><span>{t("dupes-start")}</span></div>
  {/if}
</section>

{#if dupes.confirm}
  {@const action = dupes.confirm}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (dupes.confirm = null)} onkeydown={(e) => e.key === "Escape" && (dupes.confirm = null)}>
    <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="dupes-confirm-title">
      <h2 id="dupes-confirm-title"><Icon name={action === "recycle" ? "trash" : action === "link" ? "link" : "shield"} size={20} />{t(`dupes-confirm-${action}`)}</h2>
      {#if action === "link"}
        <p>{t("dupes-link-body", { files: links.files, size: formatSize(links.bytes, loc) })}</p>
        {#if links.kinds.has("hardlink")}<p class="warn small"><Icon name="alert" size={16} />{t("dupes-link-hardlink-note")}</p>{/if}
        {#if links.kinds.has("clone")}<p class="muted small">{t("dupes-link-clone-note")}</p>{/if}
        {#if links.skipped}<p class="muted small">{t("dupes-link-skipped", { count: links.skipped })}</p>{/if}
        {#if links.otherTypes || otherTypes}
          <label class="small other"><input type="checkbox" bind:checked={otherTypes} data-effect title={t("dupes-link-other-types-hint")} />{t("dupes-link-other-types")}</label>
        {/if}
        <p class="muted small">{t("dupes-link-undo")}</p>
      {:else if folderTab}
        <p>{t("dupes-confirm-folders-body", { folders: ftotals.folders, files: formatCount(ftotals.files, loc), size: formatSize(ftotals.bytes, loc) })}</p>
        <p class="muted small">{t("dupes-confirm-folders-note")}</p>
      {:else}
        <p>{t("dupes-confirm-body", { files: totals.files, groups: totals.groups, size: formatSize(totals.bytes, loc) })}</p>
        <p class="muted small">{t("dupes-confirm-note")}</p>
      {/if}
      {#if folderTab && ftotals.risky.length && action !== "link"}
        <div class="warn">
          <p><Icon name="alert" size={16} />{t("dupes-confirm-risky", { count: ftotals.risky.length })}</p>
          <ul>
            {#each ftotals.risky.slice(0, 8) as f (f.path)}
              <li class="small"><span class="mono ellipsis" title={f.path}>{f.path}</span>{#each flagsOf(f) as fl (fl)}<span class="tag {fl}">{t(`dupes-flag-${fl}`)}</span>{/each}</li>
            {/each}
          </ul>
          <label class="small"><input type="checkbox" bind:checked={acknowledged} />{t("dupes-confirm-ack")}</label>
        </div>
      {:else if !folderTab && totals.risky.length && action !== "link"}
        <div class="warn">
          <p><Icon name="alert" size={16} />{t("dupes-confirm-risky", { count: totals.risky.length })}</p>
          <ul>
            {#each totals.risky.slice(0, 8) as c (c.path)}
              <li class="small"><span class="mono ellipsis" title={c.path}>{c.path}</span>{#each flagsOf(c) as f (f)}<span class="tag {f}">{t(`dupes-flag-${f}`)}</span>{/each}</li>
            {/each}
          </ul>
          <label class="small"><input type="checkbox" bind:checked={acknowledged} />{t("dupes-confirm-ack")}</label>
        </div>
      {/if}
      {#if action !== "link"}<ConfirmTotals summary={confirmSummary} />{/if}
      <CliHint command={cliCommand(action)} />
      <footer>
        <button class="btn" onclick={() => (dupes.confirm = null)}>{t("delete-cancel")}</button>
        <button
          class="btn danger"
          data-confirm
          onclick={go}
          disabled={action === "link" ? !links.files : (folderTab ? ftotals.risky.length : totals.risky.length) > 0 && !acknowledged}
          data-effect
          title={t(action === "link" ? "dupes-link-undo" : "dupes-confirm-note")}
        >
          <Icon name={action === "recycle" ? "trash" : action === "link" ? "link" : "shield"} size={16} />{t(`dupes-go-${action}`)}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .dupes {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .dupes > .page-head,
  .dupes > .stats {
    margin-bottom: 0;
  }
  .running,
  .result,
  .empty {
    padding: 14px 16px;
  }
  .scope {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .start-title {
    margin: 0 0 4px;
    font-size: 1.15em;
  }
  .start p {
    margin: 0;
  }
  .btn.big {
    padding: 10px 18px;
    font-size: 1.05em;
  }
  .options > summary {
    cursor: pointer;
    color: var(--text-muted);
  }
  .options-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 12px;
  }
  .headline {
    padding: 14px 16px;
    border-left: 4px solid var(--risk-caution);
  }
  .headline h2 {
    margin: 0 0 4px;
    font-size: 1.2em;
  }
  .headline p {
    margin: 2px 0 0;
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
    gap: 12px;
    align-items: center;
    padding: 8px 0;
    border-top: 1px solid var(--border);
  }
  .skipped .why {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .skipped .why p {
    margin: 2px 0;
  }
  .sample {
    display: block;
  }
  .nowrap {
    white-space: nowrap;
  }
  .head .keeps {
    min-width: 0;
    max-width: 40%;
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
  .radio,
  .field {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .radio .mono {
    max-width: 380px;
  }
  .field input[type="number"] {
    width: 90px;
  }
  .field input[type="number"],
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
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip .x {
    border: none;
    background: none;
    padding: 2px;
    color: var(--text-muted);
  }
  .running .bar {
    margin: 10px 0 6px;
    height: 8px;
  }
  .running p,
  .result p,
  .summary p {
    margin: 2px 0 0;
  }
  .result,
  .summary {
    display: flex;
    gap: 12px;
    align-items: center;
    flex-wrap: wrap;
  }
  .result {
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
  .tag {
    margin: 0 4px;
  }
  .tag.sel {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
  .tag.cloud,
  .tag.git,
  .tag.readonly,
  .tag.clone,
  .tag.app {
    background: var(--risk-caution-soft);
    color: var(--risk-caution-ink);
  }
  .group {
    padding: 0;
  }
  .head {
    display: flex;
    gap: 10px;
    align-items: center;
    width: 100%;
    padding: 10px 14px;
    border: none;
    background: none;
    text-align: left;
    min-width: 0;
  }
  .head[aria-expanded="true"] :global(svg:first-child) {
    transform: rotate(90deg);
  }
  .head.static {
    cursor: default;
  }
  .headrow {
    display: flex;
    align-items: center;
    padding-left: 14px;
  }
  .headrow .head {
    padding-left: 8px;
  }
  .head.static :global(.gcheck) {
    margin-right: 2px;
  }

  .head .name {
    font-weight: 600;
    min-width: 0;
    max-width: 50%;
  }
  .thumb {
    display: block;
    max-width: 220px;
    max-height: 140px;
    margin: 0 14px 6px 38px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    object-fit: contain;
    background: var(--surface-2);
  }
  .copies {
    list-style: none;
    margin: 0;
    padding: 0 14px 10px;
  }
  .copies li {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr) auto auto;
    gap: 10px;
    align-items: center;
    padding: 6px 4px 6px 24px;
    border-top: 1px solid var(--border);
  }
  .copies li.keep {
    background: var(--risk-safe-soft);
    border-radius: var(--radius-sm);
  }
  .keepbox,
  .rmbox {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    white-space: nowrap;
  }
  .where {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .flags {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
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
  .dialog footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
  .warn {
    margin: 10px 0;
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
  .warn li {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .warn label,
  .other {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  p.warn {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
</style>
