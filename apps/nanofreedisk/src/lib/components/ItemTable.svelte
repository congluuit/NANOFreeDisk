<script lang="ts">
  // Flat list of located items (largest files, search results). With `checks`, every
  // row has a tick box that adds or removes it without Ctrl; clicking the row itself
  // still selects only it, Ctrl+click still adds it.
  import { formatDate, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import Icon from "./Icon.svelte";
  import type { Located } from "../types";

  interface Props {
    rows: Located[];
    selected: number[];
    onselect: (id: number, additive: boolean) => void;
    oncontext: (id: number, x: number, y: number) => void;
    /** Omitted for read-only lists (the Delete key does nothing). */
    ondelete?: (ids: number[]) => void;
    onreveal: (id: number) => void;
    /** Replaces the size column (e.g. a shortcut's target). */
    aux?: { label: string; value: (r: Located) => string; wide?: boolean };
    /** A tick box per row (lists the user picks from before acting). */
    checks?: boolean;
  }
  let { rows, selected, onselect, oncontext, ondelete, onreveal, aux, checks = false }: Props = $props();
  const loc = $derived(intlLocale());
</script>

<div
  class="table"
  role="listbox"
  aria-multiselectable="true"
  tabindex="0"
  class:aux={aux && !aux.wide}
  class:wide={aux?.wide}
  class:checks
  onkeydown={(e) => e.key === "Delete" && selected.length && ondelete?.(selected)}
>
  <div class="row head">
    {#if checks}<span aria-hidden="true"></span>{/if}
    <span>{t("col-name")}</span>
    <span class:num={!aux?.wide}>{aux ? aux.label : t("col-size")}</span>
    <span class="num">{t("col-modified")}</span>
    <span>{t("col-path")}</span>
  </div>
  {#each rows as r (r.id)}
    {@const on = selected.includes(r.id)}
    <div
      class="row"
      class:sel={on}
      role="option"
      aria-selected={on}
      tabindex="-1"
      onclick={(e) => onselect(r.id, e.ctrlKey || e.metaKey)}
      ondblclick={() => onreveal(r.id)}
      oncontextmenu={(e) => {
        e.preventDefault();
        oncontext(r.id, e.clientX, e.clientY);
      }}
      onkeydown={() => {}}
    >
      {#if checks}
        <!-- the box toggles on its own: its clicks never reach the row -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
        <label class="tick" onclick={(e) => e.stopPropagation()} ondblclick={(e) => e.stopPropagation()}>
          <input type="checkbox" data-row-check={r.id} checked={on} aria-label={t("sel-row", { name: r.name })} onchange={() => onselect(r.id, true)} />
        </label>
      {/if}
      <span class="name">
        <Icon name={r.kind === "dir" ? "folder" : "file"} size={15} />
        <span class="ellipsis" title={r.name}>{r.name}</span>
      </span>
      {#if aux}
        <span class="ellipsis" class:num={!aux.wide} class:mono={aux.wide} title={aux.value(r)}>{aux.value(r)}</span>
      {:else}
        <span class="num">{formatSize(r.onDisk, loc)}</span>
      {/if}
      <span class="num muted">{formatDate(r.mtime, loc)}</span>
      <span class="ellipsis muted mono" title={r.path}>{r.path}</span>
    </div>
  {/each}
</div>

<style>
  .table {
    outline: none;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(140px, 1.1fr) 90px 100px minmax(160px, 2fr);
    gap: 10px;
    align-items: center;
    padding: 5px 10px;
    border-radius: 4px;
    cursor: default;
  }
  .aux .row {
    grid-template-columns: minmax(140px, 1.1fr) 140px 100px minmax(160px, 2fr);
  }
  .wide .row {
    grid-template-columns: minmax(140px, 1fr) minmax(160px, 1.4fr) 100px minmax(160px, 1.6fr);
  }
  .checks .row {
    grid-template-columns: 18px minmax(140px, 1.1fr) 90px 100px minmax(160px, 2fr);
  }
  .checks.aux .row {
    grid-template-columns: 18px minmax(140px, 1.1fr) 140px 100px minmax(160px, 2fr);
  }
  .checks.wide .row {
    grid-template-columns: 18px minmax(140px, 1fr) minmax(160px, 1.4fr) 100px minmax(160px, 1.6fr);
  }
  .tick {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin: -5px 0;
    padding: 5px 0;
    cursor: pointer;
  }
  .tick input {
    margin: 0;
    cursor: pointer;
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
    background: var(--surface);
    font-size: 0.82em;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
  }
  .name {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
    color: var(--text);
  }
</style>
