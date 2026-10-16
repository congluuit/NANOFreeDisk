<script lang="ts">
  // "What changed?" (PLAN §13 A1): compares the loaded scan with an earlier snapshot
  // of the same folder from the scan history. Click a row to jump to it.
  import { api } from "../api";
  import { daysBetween, formatCount, formatDateTime, formatDelta } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, fail, navigate, select } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import type { DirChange, Diff, FileChange, Snapshot } from "../types";

  let snapshots = $state<Snapshot[]>([]);
  /** The snapshot list has been loaded for the current scan. */
  let listed = $state(false);
  let older = $state<number | null>(null);
  let diff = $state<Diff | null>(null);
  let loading = $state(false);
  const loc = $derived(intlLocale());
  const choices = $derived(snapshots.filter((s) => s.id !== app.snapshot));

  $effect(() => {
    const root = app.summary?.root;
    void app.snapshot;
    if (!root) return;
    let cancelled = false;
    listed = false;
    api
      .historySnapshots(root)
      .then((list) => {
        if (cancelled) return;
        snapshots = list;
        listed = true;
        // default: the snapshot just before the loaded one
        const others = list.filter((s) => s.id !== app.snapshot);
        if (!others.some((s) => s.id === older)) older = others[0]?.id ?? null;
      })
      .catch(fail);
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const newer = app.snapshot;
    const from = older;
    if (newer == null || from == null) {
      diff = null;
      return;
    }
    let cancelled = false;
    loading = true;
    api
      .historyDiff(from, newer, 10 * 2 ** 20, 100)
      .then((d) => !cancelled && (diff = d))
      .catch(fail)
      .finally(() => !cancelled && (loading = false));
    return () => {
      cancelled = true;
    };
  });

  async function jump(path: string, isFile: boolean) {
    try {
      const id = await api.findPath(path);
      if (id == null) return;
      if (isFile) {
        const parent = path.replace(/[\\/][^\\/]*$/, "");
        const dir = path === parent ? null : await api.findPath(parent);
        navigate(dir ?? app.summary?.rootId ?? 0);
        select(id);
      } else {
        navigate(id);
      }
    } catch (e) {
      fail(e);
    }
  }

  const max = $derived(
    Math.max(1, ...(diff ? [...diff.grew, ...diff.shrank].map((c) => Math.abs(c.delta)) : [1])),
  );
  const exists = (c: DirChange | FileChange) => c.after > 0;
</script>

<div class="changes">
  {#if !app.settings.history && app.snapshot == null}
    <p class="muted note"><Icon name="alert" size={16} />{t("changes-history-off")}</p>
  {:else if app.snapshot == null}
    <p class="muted note"><Icon name="alert" size={16} />{t("changes-not-recorded")}</p>
  {:else if !listed}
    <p class="muted note">…</p>
  {:else if !choices.length}
    <p class="muted note"><Icon name="refresh" size={16} />{t("changes-first")}</p>
  {:else}
    <div class="head">
      <label>
        <span class="muted">{t("changes-compare")}</span>
        <select bind:value={older}>
          {#each choices as s (s.id)}
            <option value={s.id}>{formatDateTime(s.startedAt, loc)}</option>
          {/each}
        </select>
      </label>
      {#if diff}
        <span class="sum">
          {t("changes-summary", {
            days: daysBetween(diff.older.startedAt, diff.newer.startedAt),
            size: formatDelta(diff.onDiskDelta, loc),
            files: (diff.filesDelta > 0 ? "+" : "") + formatCount(diff.filesDelta, loc),
          })}
        </span>
        {#if diff.freeDelta != null}
          <span class="pill" class:bad={diff.freeDelta < 0}>
            {t("changes-free", { size: formatDelta(diff.freeDelta, loc) })}
          </span>
        {/if}
      {/if}
    </div>
    {#if diff}
      {#if !diff.grew.length && !diff.shrank.length && !diff.filesUp.length && !diff.filesDown.length}
        <p class="muted note"><Icon name="check" size={16} />{t("changes-none")}</p>
      {/if}
      {#each [{ key: "changes-grew", rows: diff.grew }, { key: "changes-shrank", rows: diff.shrank }] as group (group.key)}
        {#if group.rows.length}
          <h4>{t(group.key)}</h4>
          {#each group.rows as c (c.path)}
            <button class="row" disabled={!exists(c)} onclick={() => jump(c.path, false)} title={c.path}>
              <Icon name="folder" size={15} />
              <span class="ellipsis path">{c.path}</span>
              <span class="bar"><span class:down={c.delta < 0} style="width: {(Math.abs(c.delta) / max) * 100}%"></span></span>
              <span class="num delta" class:up={c.delta > 0}>{formatDelta(c.delta, loc)}</span>
            </button>
          {/each}
        {/if}
      {/each}
      {#each [{ key: "changes-files-up", rows: diff.filesUp }, { key: "changes-files-down", rows: diff.filesDown }] as group (group.key)}
        {#if group.rows.length}
          <h4>{t(group.key)}</h4>
          {#each group.rows as f (f.path)}
            <button class="row" disabled={!exists(f)} onclick={() => jump(f.path, true)} title={f.path}>
              <Icon name="file" size={15} />
              <span class="ellipsis path">{f.path}</span>
              <span class="muted small">{f.before === 0 ? t("changes-new") : f.after === 0 ? t("changes-gone") : ""}</span>
              <span class="num delta" class:up={f.delta > 0}>{formatDelta(f.delta, loc)}</span>
            </button>
          {/each}
        {/if}
      {/each}
      <p class="muted small foot">{t("changes-footnote")}</p>
    {:else if loading}
      <p class="muted note">…</p>
    {/if}
  {/if}
</div>

<style>
  .changes {
    padding: 6px 10px 10px;
  }
  .note {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 10px 4px;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    margin-bottom: 4px;
  }
  .head label {
    display: inline-flex;
    gap: 8px;
    align-items: center;
  }
  .sum {
    font-weight: 600;
  }
  .pill {
    padding: 1px 8px;
    border-radius: 10px;
    background: var(--accent-soft);
    font-size: 0.9em;
  }
  .pill.bad {
    background: var(--danger-soft);
  }
  h4 {
    margin: 10px 0 2px;
    font-size: 0.78em;
    font-weight: normal;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    color: var(--text-muted);
  }
  .row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) 110px 90px;
    gap: 8px;
    align-items: center;
    width: 100%;
    padding: 4px 6px;
    border: none;
    border-radius: 4px;
    background: none;
    text-align: left;
  }
  .row:hover:not(:disabled) {
    background: var(--hover);
  }
  .row:disabled {
    cursor: default;
    opacity: 0.75;
  }
  .path {
    font-family: "Cascadia Mono", Consolas, ui-monospace, monospace;
    font-size: 0.9em;
  }
  .bar {
    display: block;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--risk-caution);
  }
  .bar span.down {
    background: var(--risk-safe);
  }
  .delta {
    font-weight: 600;
  }
  .delta.up {
    color: var(--risk-advanced);
  }
  .small {
    font-size: 0.84em;
  }
  .foot {
    margin: 10px 4px 0;
  }
</style>
