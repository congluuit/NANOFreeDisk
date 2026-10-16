<script lang="ts">
  // Search box + filters (PLAN §4.1 / A4). Results run on the in-memory scan.
  import { api } from "../api";
  import { parseSize } from "../format";
  import { t } from "../i18n.svelte";
  import { app, expert, fail, toast } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import { ATTR, type Filter } from "../types";

  let { onresults }: { onresults: () => void } = $props();

  let name = $state("");
  let open = $state(false);
  let kind = $state<"any" | "files" | "dirs">("any");
  let minSize = $state("");
  let maxSize = $state("");
  let exts = $state("");
  let modified = $state<"any" | "older-1y" | "older-3y" | "last-30d">("any");
  let hideHidden = $state(false);
  let cloudOnly = $state(false);
  let input: HTMLInputElement;

  const DAY = 86_400;

  function build(): Filter | null {
    const min = minSize.trim() ? parseSize(minSize) : null;
    const max = maxSize.trim() ? parseSize(maxSize) : null;
    if ((minSize.trim() && min == null) || (maxSize.trim() && max == null)) {
      toast(t("filter-invalid-size"), "error");
      return null;
    }
    const now = Math.floor(Date.now() / 1000);
    return {
      name: name.trim() || null,
      kind,
      min_size: min,
      max_size: max,
      exts: exts
        .split(/[\s,;]+/)
        .map((e) => e.replace(/^\*?\./, "").toLowerCase())
        .filter(Boolean),
      modified_before: modified === "older-1y" ? now - 365 * DAY : modified === "older-3y" ? now - 3 * 365 * DAY : null,
      modified_after: modified === "last-30d" ? now - 30 * DAY : null,
      attrs_none: hideHidden ? ATTR.hidden | ATTR.system : 0,
      attrs_all: cloudOnly ? ATTR.offline : 0,
    };
  }

  async function run(e?: Event) {
    e?.preventDefault();
    const filter = build();
    if (!filter) return;
    const empty =
      !filter.name && filter.kind === "any" && filter.min_size == null && filter.max_size == null &&
      !filter.exts?.length && filter.modified_before == null && filter.modified_after == null &&
      !filter.attrs_none && !filter.attrs_all;
    if (empty) {
      app.search = null;
      return;
    }
    try {
      app.search = { filter, result: await api.search(app.current, filter, 500) };
      onresults();
    } catch (err) {
      fail(err);
    }
  }

  function clear() {
    name = "";
    kind = "any";
    minSize = maxSize = exts = "";
    modified = "any";
    hideHidden = cloudOnly = false;
    app.search = null;
  }

  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        input?.focus();
        input?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

<form class="filters" onsubmit={run}>
  <div class="line">
    <label class="search">
      <Icon name="search" size={16} />
      <input
        bind:this={input}
        type="search"
        bind:value={name}
        placeholder={t("analyze-search-placeholder")}
        aria-label={t("analyze-search-placeholder")}
      />
    </label>
    {#if expert()}
      <button type="button" class="btn ghost" class:on={open} onclick={() => (open = !open)} aria-expanded={open}>
        <Icon name="filter" size={16} />{t("filter-title")}
      </button>
    {/if}
    <button type="submit" class="btn primary">{t("filter-apply")}</button>
    {#if app.search}
      <button type="button" class="btn ghost" onclick={clear}>{t("filter-clear")}</button>
    {/if}
  </div>
  {#if open && expert()}
    <div class="more">
      <label>
        <span>{t("filter-kind")}</span>
        <select bind:value={kind}>
          <option value="any">{t("filter-kind-any")}</option>
          <option value="files">{t("filter-kind-files")}</option>
          <option value="dirs">{t("filter-kind-dirs")}</option>
        </select>
      </label>
      <label>
        <span>{t("filter-min-size")}</span>
        <input type="text" bind:value={minSize} placeholder="500MB" size="7" />
      </label>
      <label>
        <span>{t("filter-max-size")}</span>
        <input type="text" bind:value={maxSize} placeholder="10GB" size="7" />
      </label>
      <label>
        <span>{t("filter-exts")}</span>
        <input type="text" bind:value={exts} placeholder={t("filter-exts-placeholder")} size="12" />
      </label>
      <label>
        <span>{t("filter-modified")}</span>
        <select bind:value={modified}>
          <option value="any">{t("filter-modified-any")}</option>
          <option value="older-1y">{t("filter-modified-older-1y")}</option>
          <option value="older-3y">{t("filter-modified-older-3y")}</option>
          <option value="last-30d">{t("filter-modified-last-30d")}</option>
        </select>
      </label>
      <label class="check"><input type="checkbox" bind:checked={hideHidden} />{t("filter-hide-hidden")}</label>
      <label class="check"><input type="checkbox" bind:checked={cloudOnly} />{t("filter-cloud-only")}</label>
    </div>
  {/if}
</form>

<style>
  .filters {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .line {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    flex: 1;
    border: none;
    background: transparent;
    padding: 0.45em 0;
    color: var(--text);
  }
  .on {
    background: var(--accent-soft);
  }
  .more {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 16px;
    align-items: flex-end;
    padding: 10px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .more label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.86em;
    color: var(--text-muted);
  }
  .more label.check {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    color: var(--text);
  }
</style>
