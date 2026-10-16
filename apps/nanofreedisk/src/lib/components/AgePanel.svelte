<script lang="ts">
  // Age heatmap (PLAN §13 A9): bytes by last-modified age, one row per child of the
  // open folder. Darker = more bytes. Click a folder row to open it.
  import { api } from "../api";
  import { formatPercent, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, fail, navigate } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import type { AgeMatrix } from "../types";

  let data = $state<AgeMatrix | null>(null);
  const loc = $derived(intlLocale());
  const columns = $derived(data ? data.totals.length : 7);
  const max = $derived(
    Math.max(1, ...(data ? data.rows.flatMap((r) => r.bytes).concat(data.other) : [1])),
  );
  const grand = $derived(data ? data.totals.reduce((a, b) => a + b, 0) : 0);

  $effect(() => {
    const id = app.current;
    void app.version;
    if (!app.summary) return;
    let cancelled = false;
    api
      .ageMatrix(id, 14)
      .then((d) => !cancelled && (data = d))
      .catch(fail);
    return () => {
      cancelled = true;
    };
  });

  /** Share of the accent color for a cell (square root keeps small cells visible). */
  const heat = (bytes: number) => (bytes > 0 ? Math.round(12 + Math.sqrt(bytes / max) * 78) : 0);
</script>

{#if data}
  <div class="age">
    <p class="muted small hint">{t("age-hint")}</p>
    <table>
      <thead>
        <tr>
          <th>{t("col-name")}</th>
          {#each Array(columns) as _, c (c)}<th class="num">{t(`age-col-${c}`)}</th>{/each}
          <th class="num">{t("col-size")}</th>
        </tr>
      </thead>
      <tbody>
        {#each data.rows as r (r.id)}
          <tr class:link={!r.direct && r.kind === "dir"} onclick={() => !r.direct && r.kind === "dir" && navigate(r.id)}>
            <td>
              <div class="name">
                <Icon name={r.direct ? "file" : r.kind === "dir" ? "folder" : "file"} size={14} />
                <span class="ellipsis" title={r.name}>{r.direct ? t("age-direct") : r.name}</span>
              </div>
            </td>
            {#each r.bytes as b, c (c)}
              <td class="cell" style="background: color-mix(in srgb, var(--accent) {heat(b)}%, var(--surface))" title={`${formatSize(b, loc)} · ${r.files[c]} ${t("stat-files").toLowerCase()}`}>
                {b ? formatSize(b, loc) : ""}
              </td>
            {/each}
            <td class="num">{formatSize(r.total, loc)}</td>
          </tr>
        {/each}
        {#if data.other.some((x) => x > 0)}
          <tr>
            <td class="muted">{t("age-other")}</td>
            {#each data.other as b, c (c)}
              <td class="cell" style="background: color-mix(in srgb, var(--accent) {heat(b)}%, var(--surface))">{b ? formatSize(b, loc) : ""}</td>
            {/each}
            <td class="num muted">{formatSize(data.other.reduce((a, b) => a + b, 0), loc)}</td>
          </tr>
        {/if}
      </tbody>
      <tfoot>
        <tr>
          <td><strong>{t("age-total")}</strong></td>
          {#each data.totals as b, c (c)}
            <td class="num"><strong>{formatSize(b, loc)}</strong><br /><span class="muted small">{grand ? formatPercent(b / grand, loc) : ""}</span></td>
          {/each}
          <td class="num"><strong>{formatSize(grand, loc)}</strong></td>
        </tr>
      </tfoot>
    </table>
  </div>
{/if}

<style>
  .age {
    padding: 4px 8px 10px;
  }
  .hint {
    margin: 4px 2px 8px;
  }
  .small {
    font-size: 0.84em;
  }
  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 2px;
    font-size: 0.86em;
    table-layout: fixed;
  }
  th {
    font-weight: normal;
    color: var(--text-muted);
    font-size: 0.9em;
    padding: 2px 4px;
    text-align: left;
    white-space: normal;
  }
  th:first-child {
    width: 22%;
  }
  td {
    padding: 3px 4px;
  }
  .name {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .cell {
    text-align: right;
    border-radius: 3px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-variant-numeric: tabular-nums;
  }
  tr.link {
    cursor: pointer;
  }
  tr.link:hover .name {
    color: var(--accent);
  }
  tfoot td {
    border-top: 1px solid var(--border);
  }
</style>
