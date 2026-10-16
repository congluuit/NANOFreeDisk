<script lang="ts">
  import { onMount } from "svelte";
  import { errorText, inTauri, pickSaveFile } from "../api";
  import { formatCount, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app, expert, fail, toast } from "../state.svelte";
  import { hwApi } from "../hw/api";
  import { asText, sections, tempLevel } from "../hw/overview";
  import { hw } from "../hw/state.svelte";
  import Icon from "../components/Icon.svelte";

  let loading = $state(false);
  let error = $state<string | null>(null);

  const loc = $derived(intlLocale());
  const list = $derived(
    hw.inventory
      ? sections(hw.inventory, t, { size: (b) => formatSize(b, loc), number: (n) => formatCount(n, loc) })
      : [],
  );

  async function load() {
    loading = true;
    error = null;
    try {
      hw.inventory = await hwApi.inventory();
    } catch (e) {
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    if (!hw.inventory) void load();
  });

  async function copyText() {
    const date = new Date().toLocaleString(loc);
    try {
      await navigator.clipboard.writeText(asText(list, expert(), t("hw-text-header", { date })));
      toast(t("hw-copied"), "ok");
    } catch (e) {
      fail(e);
    }
  }

  async function exportJson() {
    const path = await pickSaveFile(t("hw-export-title"), "hardware.json", "json");
    if (!path) return;
    try {
      await hwApi.exportInventory(path);
      toast(t("hw-exported", { path }), "ok");
    } catch (e) {
      fail(e);
    }
  }
</script>

<div class="toolbar">
  <span class="muted small privacy"><Icon name="shield" size={14} />{t("hw-privacy")}</span>
  <span class="grow"></span>
  <button class="btn ghost" onclick={load} disabled={loading} title={t("hw-refresh")}><Icon name="refresh" /></button>
  <button class="btn" onclick={copyText} disabled={!hw.inventory}><Icon name="copy" />{t("hw-copy-text")}</button>
  {#if inTauri && app.settings.mode === "expert"}
    <button class="btn" onclick={exportJson} disabled={!hw.inventory}><Icon name="export" />{t("hw-export-json")}</button>
  {/if}
</div>

<div class="scroll">
  {#if error}
    <p class="err">{t("hw-error", { message: error })}</p>
  {:else if !hw.inventory}
    <p class="muted">{t("hw-loading")}</p>
  {:else}
    <div class="grid">
      {#each list as s (s.id)}
        <article class="card sec" data-sec={s.id}>
          <h2><span class="ic"><Icon name={s.icon} size={18} /></span>{s.title}</h2>
          {#each s.items as it, i (i)}
            <div class="item">
              {#if it.heading}
                <div class="head">
                  <strong class="ellipsis" title={it.heading}>{it.heading}</strong>
                  {#if it.tag}<span class="tag">{it.tag}</span>{/if}
                </div>
              {/if}
              <dl>
                {#each it.rows.filter((r) => app.settings.mode === "expert" || !r.expert) as r (r.label)}
                  <dt class="muted">{r.label}</dt>
                  <dd>
                    {#if r.temp}
                      <span class="temp {tempLevel(r.temp.c, r.temp.kind)}">{r.value}</span>
                    {:else}
                      {r.value}
                    {/if}
                  </dd>
                {/each}
              </dl>
            </div>
          {:else}
            <p class="muted small">{t("hw-none")}</p>
          {/each}
          {#if s.chips?.length}
            <div class="chips">
              {#each s.chips as c (c)}<span class="chip">{c}</span>{/each}
            </div>
          {/if}
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }
  .privacy {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .grow {
    flex: 1;
  }
  .small {
    font-size: 0.86em;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-bottom: 24px;
  }
  .err {
    color: var(--danger);
  }
  .grid {
    columns: 340px;
    column-gap: 14px;
  }
  .sec {
    padding: 14px 16px;
    min-width: 0;
    margin-bottom: 14px;
    break-inside: avoid;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 10px;
    font-size: 1.02em;
  }
  .ic {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .item + .item {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
    min-width: 0;
  }
  .tag {
    flex: none;
    padding: 0 6px;
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: 0.8em;
  }
  dl {
    display: grid;
    grid-template-columns: minmax(110px, auto) 1fr;
    gap: 3px 12px;
    margin: 0;
    font-size: 0.92em;
  }
  dt,
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .temp {
    font-weight: 600;
  }
  .temp.ok {
    color: var(--temp-ok);
  }
  .temp.warm {
    color: var(--temp-warm);
  }
  .temp.hot {
    color: var(--temp-hot);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 10px;
  }
  .chip {
    padding: 1px 7px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 0.78em;
    color: var(--text-muted);
  }
</style>
