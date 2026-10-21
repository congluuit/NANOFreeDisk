<script lang="ts">
  // Monitor — Disk Activity (docs/PHASE6_MONITOR.md §2.5). Read-only: the view
  // never ends or changes a process; "End task…" opens the Tasks tab's guarded
  // dialog (verdict, confirmation, hard blocks).
  import { onMount } from "svelte";
  import { errorText, pickFolder } from "../api";
  import { formatCount, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, fail, startScan, toast } from "../state.svelte";
  import { hwApi } from "../hw/api";
  import type { Task } from "../hw/types";
  import { monitorApi } from "../monitor/api";
  import { alertText } from "../monitor/sync.svelte";
  import type { MonitorHistory, MonitorView, Point, ProcRate, TouchRow } from "../monitor/types";
  import { diskLabel, filesOf, hourBars, iops, niceMax, parentDir, seriesPath, sortProcs, type SortKey } from "../monitor/view";
  import CliHint from "../components/CliHint.svelte";
  import EndTaskDialog from "../components/EndTaskDialog.svelte";
  import Icon from "../components/Icon.svelte";

  type Tab = "activity" | "history" | "who";
  let tab = $state<Tab>("activity");
  let view = $state<MonitorView | null>(null);
  let points = $state<Point[]>([]);
  let windowSecs = $state<60 | 600>(60);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let sort = $state<{ key: SortKey; desc: boolean }>({ key: "write", desc: true });
  let open = $state<{ pid: number; ct: string } | null>(null);
  let ending = $state<Task | null>(null);
  let history = $state<MonitorHistory | null>(null);
  let watchInput = $state("");

  const loc = $derived(intlLocale());
  const isExpert = $derived(app.settings.mode === "expert");
  const snap = $derived(view?.snapshot ?? null);
  const detail = $derived(!!view?.detail.running);
  const procs = $derived(snap ? sortProcs(snap.processes, sort.key, sort.desc) : []);
  const max = $derived(niceMax(Math.max(1, ...points.map((p) => Math.max(p.read, p.write)))));
  const end = $derived(points.at(-1)?.t ?? 0);
  const bars = $derived(history ? hourBars(history.minutes, Math.floor(Date.now() / 1000)) : []);
  const barMax = $derived(Math.max(1, ...bars.map((b) => Math.max(b.read, b.write))));

  const rate = (v: number) => (v < 1 ? "0 B/s" : `${formatSize(v, loc)}/s`);
  const num = (v: number) => v.toLocaleString(loc, { maximumFractionDigits: v < 10 ? 1 : 0 });
  const clock = (ms: number) => new Date(ms).toLocaleTimeString(loc);
  const hour = (secs: number) => new Date(secs * 1000).toLocaleTimeString(loc, { hour: "2-digit", minute: "2-digit" });

  async function tick() {
    if (document.hidden) return;
    try {
      view = await monitorApi.snapshot();
      if (tab === "activity") points = await monitorApi.chart(windowSecs);
      error = null;
    } catch (e) {
      error = errorText(e);
    }
  }

  onMount(() => {
    void tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    if (tab === "history") {
      monitorApi
        .history()
        .then((h) => (history = h))
        .catch(fail);
    }
  });

  $effect(() => {
    if (view?.watch && !watchInput) watchInput = view.watch;
  });

  function sortBy(key: SortKey) {
    sort = sort.key === key ? { key, desc: !sort.desc } : { key, desc: key !== "name" && key !== "pid" };
  }

  async function togglePause() {
    if (!view) return;
    try {
      await monitorApi.pause(!view.paused);
      await tick();
    } catch (e) {
      fail(e);
    }
  }

  async function setDetail(enabled: boolean) {
    busy = true;
    try {
      view = await monitorApi.detail(enabled);
    } catch (e) {
      const code = (e as { code?: string })?.code;
      toast(code === "uac-declined" ? t("mon-detail-declined") : t("mon-detail-failed", { message: errorText(e) }), "error");
    } finally {
      busy = false;
    }
  }

  async function watch(path: string | null) {
    try {
      view = await monitorApi.watch(path);
      if (!path) watchInput = "";
    } catch (e) {
      fail(e);
    }
  }

  async function pickWatch() {
    const path = await pickFolder(t("mon-who-pick"));
    if (path) {
      watchInput = path;
      await watch(path);
    }
  }

  function reveal(path: string) {
    monitorApi.reveal(path).catch(fail);
  }

  function analyze(path: string) {
    void startScan(parentDir(path));
  }

  /** The Tasks tab's guarded End-task dialog, for the live process. */
  async function endTask(p: ProcRate) {
    try {
      const now = await hwApi.tasks();
      const task = now.tasks.find((x) => x.pid === p.pid && x.create_time === p.createTime);
      if (task) ending = task;
      else toast(t("mon-process-gone", { name: p.name }), "error");
    } catch (e) {
      fail(e);
    }
  }

  const isOpen = (p: ProcRate) => open?.pid === p.pid && open.ct === p.createTime;

  const COLS: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "name", label: "mon-col-name" },
    { key: "pid", label: "mon-col-pid", num: true },
    { key: "read", label: "mon-col-read", num: true },
    { key: "write", label: "mon-col-write", num: true },
    { key: "iops", label: "mon-col-iops", num: true },
    { key: "files", label: "mon-col-files", num: true },
  ];

  const opLabel = (r: TouchRow) => t(`mon-op-${r.op}`);
  const cli = $derived(
    tab === "history"
      ? "nfd monitor --history"
      : tab === "who" && view?.watch
        ? `nfd monitor --watch "${view.watch}" --seconds 60`
        : `nfd monitor --source ${detail ? "etw" : "counters"} --seconds 10`,
  );
</script>

<section class="page monitor">
  <header class="page-head">
    <div>
      <h1>{t("mon-title")}</h1>
      <p>{t("mon-subtitle")}</p>
    </div>
    <div class="page-actions">
      {#if view}
        <span class="badge" data-source={snap?.source ?? "counters"} title={t(`mon-source-${snap?.source ?? "counters"}-hint`)}>
          <Icon name={detail ? "file" : "monitor"} size={14} />{t(`mon-source-${snap?.source ?? "counters"}`)}
        </span>
        {#if detail}
          <button class="btn" onclick={() => setDetail(false)} disabled={busy} data-effect title={t("hint-mon-detail-off")}>{t("mon-detail-off")}</button>
        {:else}
          <button class="btn primary" data-detail onclick={() => setDetail(true)} disabled={busy || view.paused} data-effect title={t(view.elevated ? "hint-mon-detail-on-admin" : "hint-mon-detail-on")}>
            <Icon name="shield" size={16} />{t("mon-detail-on")}
          </button>
        {/if}
        <button class="btn" data-pause onclick={togglePause} data-effect title={t(view.paused ? "hint-mon-resume" : "hint-mon-pause")}>
          <Icon name={view.paused ? "refresh" : "clock"} size={16} />{t(view.paused ? "mon-resume" : "mon-pause")}
        </button>
      {/if}
    </div>
  </header>

  <div class="segmented seg" role="tablist" aria-label={t("mon-title")}>
    {#each [["activity", "monitor", "mon-tab-activity"], ["history", "clock", "mon-tab-history"], ["who", "search", "mon-tab-who"]] as [id, icon, label] (id)}
      <button role="tab" class="montab" data-montab={id} class:on={tab === id} aria-selected={tab === id} onclick={() => (tab = id as Tab)}>
        <Icon name={icon} size={16} />{t(label)}
      </button>
    {/each}
  </div>

  {#if view?.detail.error}
    <p class="note warn" data-detail-error>{t(`mon-detail-error-${["etw-silent", "helper-stopped", "session-stopped"].includes(view.detail.error) ? view.detail.error : "other"}`)}</p>
  {/if}
  {#if view?.paused}
    <p class="note">{t("mon-paused-note")}</p>
  {/if}
  {#if error}
    <p class="note err">{t("mon-error", { message: error })}</p>
  {/if}

  <div class="body">
    {#if tab === "activity"}
      <div class="top">
        <div class="card chart" data-chart>
          <div class="chart-head">
            <span class="legend"><i class="sw read"></i>{t("mon-read")} <b class="mono">{rate(snap?.read ?? 0)}</b></span>
            <span class="legend"><i class="sw write"></i>{t("mon-write")} <b class="mono">{rate(snap?.write ?? 0)}</b></span>
            <span class="muted small mono">{num(snap?.ops ?? 0)} {t("mon-ops")}</span>
            <span class="grow"></span>
            <div class="segmented mini-seg" role="radiogroup" aria-label={t("mon-window")}>
              {#each [60, 600] as const as w (w)}
                <button role="radio" aria-checked={windowSecs === w} class:on={windowSecs === w} onclick={() => ((windowSecs = w), void tick())}>
                  {t(w === 60 ? "mon-window-60" : "mon-window-600")}
                </button>
              {/each}
            </div>
          </div>
          <svg viewBox="0 0 600 140" preserveAspectRatio="none" role="img" aria-label={t("mon-chart-label")}>
            {#each [0.25, 0.5, 0.75] as g (g)}
              <line x1="0" x2="600" y1={140 * g} y2={140 * g} class="grid" />
            {/each}
            <path d={seriesPath(points, "read", end, windowSecs, 600, 140, max)} class="line read" />
            <path d={seriesPath(points, "write", end, windowSecs, 600, 140, max)} class="line write" />
          </svg>
          <div class="axis muted small"><span>{t(windowSecs === 60 ? "mon-ago-60" : "mon-ago-600")}</span><span class="mono">{rate(max)}</span></div>
        </div>
        <div class="disks">
          {#each snap?.disks ?? [] as d (d.name)}
            {@const l = diskLabel(d)}
            <div class="card disk" data-disk={d.name}>
              <div class="disk-head"><Icon name="drive" size={16} /><b>{t("mon-disk", { index: l.index })}</b><span class="muted">{l.letters}</span></div>
              <div class="kv"><span class="muted">{t("mon-read")}</span><span class="mono">{rate(d.read)}</span></div>
              <div class="kv"><span class="muted">{t("mon-write")}</span><span class="mono">{rate(d.write)}</span></div>
              <div class="kv"><span class="muted">{t("mon-queue")}</span><span class="mono">{num(d.queue)}</span></div>
              <div class="kv"><span class="muted">{t("mon-latency")}</span><span class="mono">{num(d.latencyMs)} ms</span></div>
              <div class="busy"><span style="width: {Math.max(0, Math.min(100, 100 - d.idle))}%"></span></div>
              <div class="muted small">{t("mon-busy", { pct: Math.round(100 - d.idle) })}</div>
            </div>
          {:else}
            <p class="muted small">{t("mon-no-disks")}</p>
          {/each}
        </div>
      </div>

      {#if view?.alerts.length}
        <div class="card alerts" data-alerts>
          {#each view.alerts.slice(0, 3) as a (a.at + a.kind + a.pid)}
            <div class="alert-row">
              <Icon name="alert" size={16} />
              <span class="grow">{alertText(a)} <span class="muted small">· {clock(a.at)}</span></span>
              {#if a.path}<button class="btn ghost" onclick={() => reveal(a.path!)} title={t("mon-reveal")}><Icon name="reveal" size={15} /></button>{/if}
            </div>
          {/each}
        </div>
      {/if}

      <div class="grid2">
        <div class="table card">
          <div class="row head" class:nofiles={!detail}>
            {#each COLS as c (c.key)}
              {#if c.key !== "files" || detail}
                <button class="th" class:num={c.num} onclick={() => sortBy(c.key)} aria-pressed={sort.key === c.key}>
                  {t(c.label)}{#if sort.key === c.key}<span class="arrow">{sort.desc ? "▾" : "▴"}</span>{/if}
                </button>
              {/if}
            {/each}
            <span class="th"></span>
          </div>
          <div class="rows" data-procs>
            {#if !snap}
              <p class="muted pad">{t("mon-loading")}</p>
            {:else}
              {#each procs as p (p.pid + ":" + p.createTime)}
                <div class="row" class:sel={isOpen(p)} class:nofiles={!detail} data-pid={p.pid} title={p.path ?? p.name}>
                  <button class="name" onclick={() => (open = isOpen(p) ? null : { pid: p.pid, ct: p.createTime })} aria-expanded={isOpen(p)}>
                    <span class="chev" class:open={isOpen(p)}><Icon name="chevron" size={12} /></span>
                    <span class="ellipsis">{p.name}</span>
                  </button>
                  <span class="num mono muted">{p.pid}</span>
                  <span class="num mono">{rate(p.read)}</span>
                  <span class="num mono">{rate(p.write)}</span>
                  <span class="num mono">{num(iops(p))}</span>
                  {#if detail}<span class="num mono">{p.files}</span>{/if}
                  <span class="row-actions">
                    {#if p.path}<button class="btn ghost" onclick={() => reveal(p.path!)} title={t("mon-reveal-exe")}><Icon name="reveal" size={14} /></button>{/if}
                    {#if p.createTime && p.pid > 4}
                      <button class="btn ghost" data-end onclick={() => endTask(p)} data-effect title={t("hint-mon-end-task")}><Icon name="x" size={14} /></button>
                    {/if}
                  </span>
                </div>
                {#if isOpen(p)}
                  <div class="sub">
                    {#if !detail}
                      <p class="muted small">{t("mon-files-need-detail")}</p>
                    {:else}
                      {#each filesOf(snap.files, p.pid).slice(0, 30) as f (f.path)}
                        <div class="file">
                          <Icon name="file" size={14} />
                          <span class="ellipsis mono small grow" title={f.path}>{f.path}</span>
                          <span class="mono small">{t("mon-r")} {rate(f.read)}</span>
                          <span class="mono small">{t("mon-w")} {rate(f.write)}</span>
                          <button class="btn ghost" onclick={() => reveal(f.path)} title={t("mon-reveal")}><Icon name="reveal" size={14} /></button>
                          <button class="btn ghost" onclick={() => analyze(f.path)} data-effect title={t("hint-mon-analyze")}><Icon name="analyze" size={14} /></button>
                        </div>
                      {:else}
                        <p class="muted small">{t("mon-no-files")}</p>
                      {/each}
                    {/if}
                  </div>
                {/if}
              {:else}
                <p class="muted pad">{t("mon-idle")}</p>
              {/each}
            {/if}
          </div>
        </div>

        <div class="card topfiles" data-topfiles>
          <h2>{t("mon-top-written")}</h2>
          {#if !detail}
            <p class="muted small">{t("mon-top-need-detail")}</p>
          {:else}
            {#each (snap?.files ?? []).filter((f) => f.write > 0).slice(0, 15) as f (f.pid + f.path)}
              <div class="tf">
                <div class="ellipsis mono small" title={f.path}>{f.path}</div>
                <div class="tf-meta">
                  <span class="muted small ellipsis">{f.process}</span>
                  <span class="mono small">{rate(f.write)}</span>
                  <button class="btn ghost" onclick={() => reveal(f.path)} title={t("mon-reveal")}><Icon name="reveal" size={14} /></button>
                  <button class="btn ghost" onclick={() => analyze(f.path)} data-effect title={t("hint-mon-analyze")}><Icon name="analyze" size={14} /></button>
                </div>
              </div>
            {:else}
              <p class="muted small">{t("mon-no-writes")}</p>
            {/each}
          {/if}
          {#if isExpert && snap && snap.source !== "counters"}
            <p class="muted small">{t("mon-lost", { lost: formatCount(snap.lost, loc), undecoded: formatCount(snap.undecoded, loc) })}</p>
          {/if}
        </div>
      </div>
    {:else if tab === "history"}
      <div class="card hist" data-history>
        <h2>{t("mon-history-title")}</h2>
        {#if !history}
          <p class="muted">{t("mon-loading")}</p>
        {:else if !history.minutes.length}
          <p class="muted">{t("mon-history-empty")}</p>
        {:else}
          <div class="bars" role="img" aria-label={t("mon-history-title")}>
            {#each bars as b (b.ts)}
              <div class="bar" title={`${hour(b.ts)} — ${t("mon-read")} ${formatSize(b.read, loc)}, ${t("mon-write")} ${formatSize(b.write, loc)}`}>
                <span class="r" style="height: {(b.read / barMax) * 100}%"></span>
                <span class="w" style="height: {(b.write / barMax) * 100}%"></span>
              </div>
            {/each}
          </div>
          <div class="axis muted small"><span>{hour(bars[0]?.ts ?? 0)}</span><span>{hour(bars.at(-1)?.ts ?? 0)}</span></div>
          <h2>{t("mon-top-writers")}</h2>
          <div class="writers">
            {#each history.top as w (w.name)}
              <div class="kv"><span class="ellipsis">{w.name}</span><span class="mono">{t("mon-w")} {formatSize(w.write, loc)} · {t("mon-r")} {formatSize(w.read, loc)}</span></div>
            {/each}
          </div>
        {/if}
        <p class="muted small">{t("mon-history-hint")}</p>
      </div>
    {:else}
      <div class="card who" data-who>
        <h2>{t("mon-who-title")}</h2>
        <p class="muted small">{t("mon-who-hint")}</p>
        <div class="who-bar">
          <input class="grow" bind:value={watchInput} placeholder={t("mon-who-placeholder")} aria-label={t("mon-who-placeholder")} />
          <button class="btn" onclick={pickWatch} title={t("mon-who-pick")}><Icon name="folder" size={16} /></button>
          {#if view?.watch}
            <button class="btn" onclick={() => watch(null)} data-effect title={t("hint-mon-unwatch")}>{t("mon-who-stop")}</button>
          {:else}
            <button class="btn primary" data-watch onclick={() => watch(watchInput.trim() || null)} disabled={!watchInput.trim()} data-effect title={t("hint-mon-watch")}>{t("mon-who-start")}</button>
          {/if}
        </div>
        {#if !detail}
          <p class="note">{t("mon-who-need-detail")}</p>
        {/if}
        <div class="timeline">
          {#each view?.touches ?? [] as r, i (i + ":" + r.at + r.path)}
            <div class="touch" data-op={r.op}>
              <span class="mono small muted">{clock(r.at)}</span>
              <span class="op op-{r.op}">{opLabel(r)}</span>
              <span class="ellipsis mono small grow" title={r.target ? `${r.path} → ${r.target}` : r.path}>{r.path}{#if r.target} → {r.target}{/if}</span>
              <span class="small ellipsis">{r.process} <span class="muted">({r.pid})</span></span>
              <button class="btn ghost" onclick={() => reveal(r.target ?? r.path)} title={t("mon-reveal")}><Icon name="reveal" size={14} /></button>
            </div>
          {:else}
            <p class="muted small">{t(view?.watch ? "mon-who-none" : "mon-who-idle")}</p>
          {/each}
        </div>
      </div>
    {/if}
    {#if isExpert}<CliHint command={cli} />{/if}
  </div>
</section>

{#if ending}
  <EndTaskDialog task={ending} mode="kill" onclose={() => (ending = null)} />
{/if}

<style>
  .monitor {
    display: flex;
    flex-direction: column;
    padding-bottom: 0;
    gap: 12px;
    min-height: 0;
    overflow: hidden;
  }
  .monitor > .page-head {
    margin-bottom: 2px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 1.06em;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: 0.84em;
    font-weight: 600;
  }
  .seg {
    align-self: flex-start;
    flex-shrink: 0;
  }
  .montab {
    padding: 5px 12px;
  }
  .note {
    margin: 0;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    font-size: 0.9em;
  }
  .note.warn {
    background: var(--risk-caution-soft);
    color: var(--risk-caution-ink);
  }
  .note.err {
    color: var(--danger);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 24px;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
  .chart {
    flex: 3 1 420px;
    min-width: 0;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .chart-head {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .legend {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .sw {
    width: 12px;
    height: 3px;
    border-radius: 2px;
    display: inline-block;
  }
  .sw.read {
    background: var(--io-read);
  }
  .sw.write {
    background: var(--io-write);
  }
  .mini-seg button {
    padding: 2px 10px;
    font-size: 0.82em;
  }
  svg {
    width: 100%;
    height: 150px;
    border-radius: var(--radius-sm);
  }
  .grid {
    stroke: var(--border);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .line {
    fill: none;
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
    stroke-linejoin: round;
  }
  .line.read {
    stroke: var(--io-read);
  }
  .line.write {
    stroke: var(--io-write);
  }
  .axis {
    display: flex;
    justify-content: space-between;
  }
  .disks {
    flex: 1 1 220px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .disk {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 0.9em;
  }
  .disk-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
  }
  .kv {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .busy {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-2);
    overflow: hidden;
    margin-top: 4px;
  }
  .busy span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .alerts {
    padding: 8px 12px;
    border-left: 3px solid var(--risk-advanced);
  }
  .alert-row {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }
  .grid2 {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: flex-start;
  }
  .table {
    flex: 2 1 580px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-height: 0;
  }
  .rows {
    flex: 1;
    min-height: 0;
    overflow: auto;
    max-height: 520px;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) 64px 100px 100px 70px 56px 64px;
    align-items: center;
    gap: 8px;
    min-height: var(--row);
    padding: 0 12px;
  }
  .row.nofiles {
    grid-template-columns: minmax(180px, 1fr) 64px 100px 100px 70px 64px;
  }
  .head {
    position: sticky;
    top: 0;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    font-size: 0.86em;
  }
  .row:not(.head):hover {
    background: var(--hover);
  }
  .row.sel {
    background: var(--accent-soft);
  }
  .th {
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-weight: 600;
    text-align: left;
    padding: 6px 0;
  }
  .th.num,
  .num {
    text-align: right;
  }
  .arrow {
    margin-left: 4px;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    border: none;
    background: transparent;
    color: var(--text);
    text-align: left;
    padding: 0;
  }
  .chev {
    display: inline-flex;
    transition: transform 0.12s;
    color: var(--text-muted);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .row-actions {
    display: flex;
    justify-content: flex-end;
  }
  .row-actions .btn,
  .file .btn,
  .tf .btn,
  .touch .btn,
  .alert-row .btn {
    padding: 2px 4px;
  }
  .sub {
    padding: 4px 12px 8px 32px;
    background: var(--surface-2);
  }
  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 26px;
  }
  .topfiles {
    flex: 1 1 260px;
    min-width: 0;
    padding: 10px 12px;
    overflow: auto;
    max-height: 560px;
  }
  .tf {
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .tf-meta {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tf-meta .ellipsis {
    flex: 1;
  }
  .hist,
  .who {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 160px;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .bar {
    flex: 1;
    height: 100%;
    display: flex;
    align-items: flex-end;
    gap: 1px;
  }
  .bar span {
    flex: 1;
    border-radius: 2px 2px 0 0;
    min-height: 1px;
  }
  .bar .r {
    background: var(--io-read);
  }
  .bar .w {
    background: var(--io-write);
  }
  .writers {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .who-bar {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .who-bar input {
    padding: 6px 8px;
  }
  .timeline {
    display: flex;
    flex-direction: column;
    max-height: 520px;
    overflow: auto;
  }
  .touch {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 28px;
    border-bottom: 1px solid var(--border);
  }
  .op {
    font-size: 0.8em;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--surface-2);
    min-width: 64px;
    text-align: center;
  }
  .op-create {
    color: var(--risk-safe);
  }
  .op-write {
    color: var(--io-write);
  }
  .op-delete {
    color: var(--danger);
  }
  .op-rename {
    color: var(--io-read);
  }
  .grow {
    flex: 1;
  }
  .small {
    font-size: 0.84em;
  }
  .pad {
    padding: 14px;
  }
</style>
