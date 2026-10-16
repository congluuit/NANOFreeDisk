<script lang="ts">
  import { api } from "../api";
  import { formatDate, formatPercent, formatSize, formatCount } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import type { Node } from "../types";

  interface Props {
    rootId: number;
    total: number;
    version: number;
    selected: number[];
    onselect: (id: number, additive: boolean) => void;
    onopen: (id: number) => void;
    oncontext: (id: number, x: number, y: number) => void;
    ondelete: (ids: number[]) => void;
  }
  let { rootId, total, version, selected, onselect, onopen, oncontext, ondelete }: Props = $props();

  interface Row {
    node: Node;
    depth: number;
  }

  let cache = $state(new Map<number, Node[]>());
  let expanded = $state(new Set<number>());
  let scrollTop = $state(0);
  let viewH = $state(400);
  let list: HTMLDivElement;
  const loc = $derived(intlLocale());
  const rowH = $derived(Math.round(30 * app.settings.fontScale));

  async function load(id: number) {
    try {
      const kids = await api.children(id);
      cache.set(id, kids);
      cache = new Map(cache);
    } catch {
      /* removed meanwhile */
    }
  }

  // new root or new data: drop caches, keep expansion where still valid
  $effect(() => {
    void version;
    const id = rootId;
    cache = new Map();
    void load(id);
    for (const e of expanded) void load(e);
  });

  $effect(() => {
    void rootId;
    expanded = new Set();
    scrollTop = 0;
    if (list) list.scrollTop = 0;
  });

  const rows = $derived.by(() => {
    const out: Row[] = [];
    const walk = (id: number, depth: number) => {
      for (const n of cache.get(id) ?? []) {
        out.push({ node: n, depth });
        if (expanded.has(n.id)) walk(n.id, depth + 1);
      }
    };
    walk(rootId, 0);
    return out;
  });

  const first = $derived(Math.max(0, Math.floor(scrollTop / rowH) - 8));
  const visible = $derived(rows.slice(first, first + Math.ceil(viewH / rowH) + 16));

  function toggle(n: Node, e?: Event) {
    e?.stopPropagation();
    if (!n.hasChildren) return;
    const next = new Set(expanded);
    if (next.has(n.id)) next.delete(n.id);
    else {
      next.add(n.id);
      if (!cache.has(n.id)) void load(n.id);
    }
    expanded = next;
  }

  function key(e: KeyboardEvent) {
    const idx = rows.findIndex((r) => r.node.id === app.focus);
    const cur = rows[idx]?.node;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = rows[Math.min(rows.length - 1, Math.max(0, idx + (e.key === "ArrowDown" ? 1 : -1)))];
      if (n) {
        onselect(n.node.id, false);
        const top = rows.indexOf(n) * rowH;
        if (top < list.scrollTop) list.scrollTop = top;
        else if (top + rowH > list.scrollTop + viewH) list.scrollTop = top + rowH - viewH;
      }
    } else if (e.key === "ArrowRight" && cur && !expanded.has(cur.id)) {
      toggle(cur);
    } else if (e.key === "ArrowLeft" && cur && expanded.has(cur.id)) {
      toggle(cur);
    } else if (e.key === "Enter" && cur?.kind === "dir") {
      onopen(cur.id);
    } else if (e.key === "Delete" && selected.length) {
      ondelete(selected);
    }
  }

  function icon(n: Node) {
    if (n.kind === "dir") return "folder";
    if (n.kind === "file" || n.kind === "other") return n.attrs.includes("offline") ? "cloud" : "file";
    return "link";
  }
</script>

<div class="tree" role="tree" aria-label={t("col-name")} tabindex="0" onkeydown={key}>
  <div class="head">
    <span>{t("col-name")}</span>
    <span class="num">{t("col-size")}</span>
    <span>{t("col-share")}</span>
    <span class="num">{t("col-files")}</span>
    <span class="num">{t("col-modified")}</span>
  </div>
  <div
    class="body"
    bind:this={list}
    bind:clientHeight={viewH}
    onscroll={() => (scrollTop = list.scrollTop)}
  >
    <div style="height: {rows.length * rowH}px; position: relative">
      {#each visible as r, k (r.node.id)}
        {@const n = r.node}
        {@const share = total ? n.onDisk / total : 0}
        <div
          class="row"
          class:sel={selected.includes(n.id)}
          class:focus={app.focus === n.id}
          role="treeitem"
          aria-selected={selected.includes(n.id)}
          aria-expanded={n.hasChildren ? expanded.has(n.id) : undefined}
          tabindex="-1"
          style="top: {(first + k) * rowH}px; height: {rowH}px"
          onclick={(e) => onselect(n.id, e.ctrlKey || e.metaKey)}
          ondblclick={() => (n.kind === "dir" ? onopen(n.id) : undefined)}
          oncontextmenu={(e) => {
            e.preventDefault();
            oncontext(n.id, e.clientX, e.clientY);
          }}
          onkeydown={() => {}}
        >
          <span class="name" style="padding-left: {r.depth * 16 + 4}px">
            <button
              class="chev"
              class:open={expanded.has(n.id)}
              class:hidden={!n.hasChildren}
              tabindex="-1"
              aria-label="toggle"
              onclick={(e) => toggle(n, e)}
            >
              <Icon name="chevron" size={14} />
            </button>
            <span class="ic {n.kind}"><Icon name={icon(n)} size={16} /></span>
            <span class="ellipsis" title={n.name}>{n.name}</span>
            {#if n.hardlinkDup}<span class="badge" title={t("attr-hardlink")}>HL</span>{/if}
            {#if n.attrs.includes("system")}<span class="badge" title={t("attr-system")}>S</span>{/if}
          </span>
          <span class="num">{formatSize(n.onDisk, loc)}</span>
          <span class="share">
            <span class="bar"><span style="width: {Math.max(share * 100, share > 0 ? 0.5 : 0)}%"></span></span>
            <small class="muted">{formatPercent(share, loc)}</small>
          </span>
          <span class="num muted">{n.kind === "dir" ? formatCount(n.files, loc) : ""}</span>
          <span class="num muted">{formatDate(n.mtime, loc)}</span>
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .tree {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    outline: none;
    container-type: inline-size;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: minmax(140px, 1fr) 84px 110px 64px 96px;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
  }
  .head {
    height: 32px;
    font-size: 0.82em;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }
  .head > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .body {
    flex: 1;
    overflow: auto;
    min-height: 0;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    cursor: default;
    border-radius: 4px;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.sel {
    background: var(--accent-soft);
  }
  .row.focus {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }
  .chev {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-muted);
    border-radius: 4px;
    transition: transform 0.12s;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .chev.hidden {
    visibility: hidden;
  }
  .ic {
    color: var(--text-muted);
    display: grid;
  }
  .ic.dir {
    color: var(--risk-caution);
  }
  .badge {
    font-size: 0.7em;
    padding: 0 4px;
    border-radius: 3px;
    border: 1px solid var(--border);
    color: var(--text-muted);
  }
  .share {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .share .bar {
    flex: 1;
  }
  /* narrow panel: drop "modified", then "files" */
  @container (max-width: 600px) {
    .head,
    .row {
      grid-template-columns: minmax(140px, 1fr) 84px 110px 64px;
    }
    .head > :nth-child(5),
    .row > :nth-child(5) {
      display: none;
    }
  }
  @container (max-width: 480px) {
    .head,
    .row {
      grid-template-columns: minmax(120px, 1fr) 80px 96px;
    }
    .head > :nth-child(4),
    .row > :nth-child(4) {
      display: none;
    }
  }
  .share small {
    width: 38px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
