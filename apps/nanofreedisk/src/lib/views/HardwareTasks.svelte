<script lang="ts">
  import { onMount } from "svelte";
  import { errorText } from "../api";
  import { formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, fail, toast } from "../state.svelte";
  import { hwApi } from "../hw/api";
  import { GROUP_ORDER, filterTasks, groupTasks, ioRate, sortTasks, treeRows, type Row, type SortKey } from "../hw/tasks";
  import type { EndMode, Task, TaskGroup, TaskSnapshot } from "../hw/types";
  import ContextMenu, { type MenuItem } from "../components/ContextMenu.svelte";
  import EndTaskDialog from "../components/EndTaskDialog.svelte";
  import Icon from "../components/Icon.svelte";

  let snap = $state<TaskSnapshot | null>(null);
  let error = $state<string | null>(null);
  let query = $state("");
  let sort = $state<{ key: SortKey; desc: boolean }>({ key: "cpu", desc: true });
  let tree = $state(false);
  let closedGroups = $state<Record<string, boolean>>({});
  let collapsed = $state<Set<number>>(new Set());
  let selected = $state<{ pid: number; ct: string } | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let ending = $state<{ task: Task; mode: EndMode; note?: string } | null>(null);
  let paused = $state(false);

  const loc = $derived(intlLocale());
  const isExpert = $derived(app.settings.mode === "expert");
  const visible = $derived(snap ? filterTasks(snap.tasks, query) : []);
  const groups = $derived(groupTasks(visible));
  const rowsFor = (list: Task[]): Row[] =>
    tree ? treeRows(list, sort, collapsed) : sortTasks(list, sort).map((task) => ({ task, depth: 0, parent: false }));

  async function tick() {
    if (document.hidden || paused) return;
    try {
      snap = await hwApi.tasks();
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

  function sortBy(key: SortKey) {
    sort = sort.key === key ? { key, desc: !sort.desc } : { key, desc: key !== "name" && key !== "pid" && key !== "publisher" };
  }

  const pct = (v: number) => `${v.toLocaleString(loc, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} %`;
  const rate = (v: number) => (v < 1 ? "0 MB/s" : `${formatSize(v, loc)}/s`);

  /** Heat for CPU / memory / disk / GPU cells, like Task Manager. */
  const heat = (ratio: number) => (ratio >= 0.5 ? "h3" : ratio >= 0.2 ? "h2" : ratio >= 0.05 ? "h1" : "");

  function isSel(task: Task) {
    return selected?.pid === task.pid && selected.ct === task.create_time;
  }

  function end(task: Task, mode: EndMode, note?: string) {
    ending = { task, mode, note };
  }

  /** After WM_CLOSE: if the app is still there after 5 s, offer to end it. */
  function afterClose(task: Task) {
    setTimeout(async () => {
      try {
        const now = await hwApi.tasks();
        const still = now.tasks.find((x) => x.pid === task.pid && x.create_time === task.create_time);
        if (still) end(still, "kill", t("task-close-timeout", { name: task.title ?? task.name }));
      } catch (e) {
        fail(e);
      }
    }, 5000);
  }

  async function copy(text: string | null | undefined) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      toast(t("task-copied"), "ok");
    } catch (e) {
      fail(e);
    }
  }

  function openMenu(e: MouseEvent, task: Task) {
    e.preventDefault();
    selected = { pid: task.pid, ct: task.create_time };
    const items: MenuItem[] = [];
    if (!task.blocked) {
      if (task.title) items.push({ icon: "x", label: t("task-menu-close"), run: () => end(task, "close") });
      items.push({ icon: "alert", label: t("task-menu-kill"), danger: true, run: () => end(task, "kill") });
      items.push({ icon: "alert", label: t("task-menu-kill-tree"), danger: true, run: () => end(task, "kill-tree") });
    } else {
      items.push({ icon: "shield", label: t("task-menu-blocked"), disabled: true, run: () => {} });
    }
    items.push(
      { icon: "reveal", label: t("task-menu-reveal"), disabled: !task.path, run: () => hwApi.reveal(task.pid, task.create_time).catch(fail) },
      { icon: "copy", label: t("task-menu-copy-path"), disabled: !task.path, run: () => copy(task.path) },
      {
        icon: "copy",
        label: t("task-menu-copy-cmd"),
        run: () =>
          hwApi
            .details(task.pid, task.create_time)
            .then((d) => (d.command_line ? copy(d.command_line) : toast(t("task-no-cmd"), "error")))
            .catch(fail),
      },
      { icon: "open", label: t("task-menu-taskmgr"), run: () => hwApi.openTaskManager().catch(fail) },
    );
    menu = { x: e.clientX, y: e.clientY, items };
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== "Delete" || !selected || !snap) return;
    const task = snap.tasks.find((x) => x.pid === selected!.pid && x.create_time === selected!.ct);
    if (task && !task.blocked) end(task, "kill");
  }

  function toggleTree(pid: number) {
    const next = new Set(collapsed);
    if (next.has(pid)) next.delete(pid);
    else next.add(pid);
    collapsed = next;
  }

  const COLS: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "name", label: "task-col-name" },
    { key: "pid", label: "task-col-pid", num: true },
    { key: "cpu", label: "task-col-cpu", num: true },
    { key: "memory", label: "task-col-memory", num: true },
    { key: "io", label: "task-col-disk", num: true },
    { key: "gpu", label: "task-col-gpu", num: true },
    { key: "publisher", label: "task-col-publisher" },
  ];
</script>

<svelte:window onkeydown={onKey} />

{#if snap}
  {@const s = snap.summary}
  <div class="summary">
    <div class="stat card"><span class="muted">{t("task-sum-cpu")}</span><b class="mono">{pct(s.cpu)}</b></div>
    <div class="stat card">
      <span class="muted">{t("task-sum-memory")}</span>
      <b class="mono">{formatSize(s.memory_used, loc)} / {formatSize(s.memory_total, loc)}</b>
    </div>
    <div class="stat card"><span class="muted">{t("task-sum-disk")}</span><b class="mono">{rate(s.disk)}</b></div>
    <div class="stat card"><span class="muted">{t("task-sum-gpu")}</span><b class="mono">{pct(s.gpu)}</b></div>
    <div class="stat card"><span class="muted">{t("task-sum-processes")}</span><b class="mono">{s.processes}</b></div>
  </div>
{/if}

<div class="toolbar">
  <span class="search">
    <Icon name="search" size={15} />
    <input type="search" bind:value={query} placeholder={t("task-search")} aria-label={t("task-search")} />
  </span>
  <label class="toggle"><input type="checkbox" bind:checked={tree} />{t("task-tree")}</label>
  <label class="toggle" title={t("task-pause-hint")}><input type="checkbox" bind:checked={paused} />{t("task-pause")}</label>
  <span class="grow"></span>
  <span class="muted small">{t("task-hint")}</span>
  <button class="btn ghost" onclick={() => hwApi.openTaskManager().catch(fail)} title={t("task-menu-taskmgr")}><Icon name="open" /></button>
</div>

<div class="table card" class:expert={isExpert}>
  <div class="row head">
    {#each COLS as c (c.key)}
      <button class="th" class:num={c.num} onclick={() => sortBy(c.key)} aria-pressed={sort.key === c.key}>
        {t(c.label)}{#if sort.key === c.key}<span class="arrow">{sort.desc ? "▾" : "▴"}</span>{/if}
      </button>
    {/each}
    {#if isExpert}
      <span class="th num">{t("task-col-threads")}</span>
      <span class="th num">{t("task-col-handles")}</span>
      <span class="th">{t("task-col-path")}</span>
    {/if}
  </div>
  <div class="rows">
    {#if error}
      <p class="err pad">{t("hw-error", { message: error })}</p>
    {:else if !snap}
      <p class="muted pad">{t("hw-loading")}</p>
    {:else}
      {#each GROUP_ORDER as g (g)}
        {@const list = groups[g as TaskGroup]}
        {#if list.length}
          <button class="grp" onclick={() => (closedGroups[g] = !closedGroups[g])} aria-expanded={!closedGroups[g]}>
            <span class="chev" class:open={!closedGroups[g]}><Icon name="chevron" size={14} /></span>
            <strong>{t(`task-group-${g}`)}</strong><span class="muted">({list.length})</span>
          </button>
          {#if !closedGroups[g]}
            {#each rowsFor(list) as r (r.task.pid + ":" + r.task.create_time)}
              {@const x = r.task}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                class="row"
                class:sel={isSel(x)}
                role="row"
                tabindex="-1"
                aria-selected={isSel(x)}
                data-pid={x.pid}
                onclick={() => (selected = { pid: x.pid, ct: x.create_time })}
                oncontextmenu={(e) => openMenu(e, x)}
                title={x.path ?? x.name}
              >
                <span class="name" style="padding-left: {r.depth * 18}px">
                  {#if r.parent}
                    <button class="twisty" onclick={(e) => (e.stopPropagation(), toggleTree(x.pid))} aria-label={t("task-expand")}>
                      <span class="chev" class:open={!collapsed.has(x.pid)}><Icon name="chevron" size={12} /></span>
                    </button>
                  {/if}
                  {#if x.blocked}<span class="lock" title={t("task-menu-blocked")}><Icon name="lock" size={12} /></span>{/if}
                  <span class="ellipsis">{x.title && g === "app" ? x.title : x.name}</span>
                  {#if x.title && g === "app"}<span class="muted ellipsis small">{x.name}</span>{/if}
                </span>
                <span class="num mono muted">{x.pid}</span>
                <span class="num mono cell {heat(x.cpu / 100)}">{pct(x.cpu)}</span>
                <span class="num mono cell {heat(snap.summary.memory_total ? x.memory / snap.summary.memory_total : 0)}">{formatSize(x.memory, loc)}</span>
                <span class="num mono cell {heat(ioRate(x) / 50e6)}">{rate(ioRate(x))}</span>
                <span class="num mono cell {heat(x.gpu / 100)}">{pct(x.gpu)}</span>
                <span class="ellipsis muted">{x.publisher ?? ""}</span>
                {#if isExpert}
                  <span class="num mono muted">{x.threads}</span>
                  <span class="num mono muted">{x.handles}</span>
                  <span class="ellipsis mono muted small">{x.path ?? ""}</span>
                {/if}
              </div>
            {/each}
          {/if}
        {/if}
      {:else}
        <p class="muted pad">{t("hw-none")}</p>
      {/each}
    {/if}
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}
{#if ending}
  <EndTaskDialog task={ending.task} mode={ending.mode} note={ending.note} onrequested={afterClose} onclose={() => ((ending = null), void tick())} />
{/if}

<style>
  .summary {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 10px;
    margin-bottom: 10px;
  }
  .stat {
    display: flex;
    flex-direction: column;
    padding: 8px 12px;
    font-size: 0.9em;
  }
  .stat b {
    font-size: 1.15em;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 10px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-muted);
  }
  .search input {
    border: none;
    background: transparent;
    width: 240px;
    padding: 5px 0;
  }
  .toggle {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    font-size: 0.9em;
  }
  .grow {
    flex: 1;
  }
  .small {
    font-size: 0.84em;
  }
  .table {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    margin-bottom: 24px;
  }
  .rows {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(220px, 1fr) 70px 80px 100px 100px 70px minmax(120px, 0.6fr);
    align-items: center;
    gap: 8px;
    min-height: var(--row);
    padding: 0 12px;
    cursor: default;
  }
  .expert .row {
    grid-template-columns: minmax(220px, 1fr) 70px 80px 100px 100px 70px minmax(120px, 0.6fr) 60px 70px minmax(160px, 0.8fr);
  }
  .row:not(.head):hover {
    background: var(--hover);
  }
  .row.sel {
    background: var(--accent-soft);
  }
  .head {
    position: sticky;
    top: 0;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    font-size: 0.86em;
  }
  .th {
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-weight: 600;
    text-align: left;
    padding: 6px 0;
  }
  .th.num {
    text-align: right;
  }
  .arrow {
    margin-left: 4px;
  }
  .grp {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px 4px;
    border: none;
    background: transparent;
    color: var(--text);
    text-align: left;
  }
  .chev {
    display: inline-flex;
    transition: transform 0.12s;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .twisty {
    border: none;
    background: transparent;
    padding: 0;
    color: var(--text-muted);
    display: inline-flex;
  }
  .lock {
    color: var(--text-muted);
    display: inline-flex;
  }
  .cell {
    padding: 0 4px;
    border-radius: 3px;
  }
  .h1 {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .h2 {
    background: color-mix(in srgb, var(--risk-caution) 25%, transparent);
  }
  .h3 {
    background: color-mix(in srgb, var(--risk-advanced) 35%, transparent);
  }
  .pad {
    padding: 14px;
  }
  .err {
    color: var(--danger);
  }
</style>
