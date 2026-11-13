<script lang="ts">
  // The Recycle Bin card: what the bin holds (per drive), and "Empty the Recycle
  // Bin" through Windows' own API after a plain confirmation that this is
  // permanent. Self-contained (no props) so any view can mount it; hidden where
  // there is no Recycle Bin to manage (macOS).
  import { formatCount, formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { currentOs, hasFeature } from "../platform";
  import { recycleBinApi } from "../recyclebin/api";
  import type { BinStatus } from "../recyclebin/types";
  import { app, fail, toast } from "../state.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Icon from "./Icon.svelte";

  const loc = $derived(intlLocale());
  const shown = $derived(hasFeature(currentOs(app.info?.os), "recycle-bin"));
  let bin = $state<BinStatus | null>(null);
  let busy = $state(false);
  let asking = $state(false);

  async function load() {
    try {
      const s = await recycleBinApi.status();
      bin = s;
      // items this app moved to the bin are gone: the Activity log must say so
      if (s.settled > 0) app.version++;
    } catch (e) {
      fail(e);
    }
  }

  $effect(() => {
    void app.version;
    if (shown) void load();
  });

  async function empty() {
    asking = false;
    busy = true;
    try {
      const r = await recycleBinApi.empty();
      toast(t("rb-emptied", { count: r.items, size: formatSize(r.bytes, loc) }), r.failed.length ? "error" : "ok");
      for (const f of r.failed.slice(0, 3)) toast(t("rb-empty-failed", { error: f }), "error");
      if (r.journalError) toast(t("clean-journal-failed", { error: r.journalError }), "error");
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
      app.version++;
    }
  }

  const drivesText = $derived(
    (bin?.drives ?? []).map((d) => t("rb-drive-line", { drive: d.root, count: formatCount(d.items, loc), size: formatSize(d.bytes, loc) })).join("\n"),
  );
</script>

{#if shown}
  <section class="panel rb" class:empty={bin !== null && !bin.items} data-recyclebin aria-labelledby="rb-title">
    <div class="rb-head">
      <span class="rb-icon"><Icon name="trash" size={22} /></span>
      <div class="rb-what">
        <h2 id="rb-title">{t("rb-title")}</h2>
        <p class="muted">{t("rb-intro")}</p>
      </div>
      <div class="rb-actions">
        <button class="btn ghost icon" onclick={load} disabled={busy} title={t("rb-refresh")} aria-label={t("rb-refresh")}><Icon name="refresh" size={16} /></button>
        <button class="btn danger" disabled={!bin?.items || busy} onclick={() => (asking = true)} title={t("rb-empty-hint")} data-recyclebin-empty>
          <Icon name="trash" size={16} />{t("rb-empty")}
        </button>
      </div>
    </div>
    {#if bin === null}
      <p class="muted rb-line">{t("rb-loading")}</p>
    {:else if bin.items}
      <div class="rb-facts">
        <div class="fact"><b data-recyclebin-bytes>{formatSize(bin.bytes, loc)}</b><span>{t("rb-held")}</span></div>
        <div class="fact"><b data-recyclebin-items>{formatCount(bin.items, loc)}</b><span>{t("rb-items")}</span></div>
        {#each bin.drives.length > 1 ? bin.drives : [] as d (d.root)}
          <div class="fact"><b>{formatSize(d.bytes, loc)}</b><span>{t("rb-on-drive", { drive: d.root, count: formatCount(d.items, loc) })}</span></div>
        {/each}
      </div>
      <p class="muted small rb-line"><Icon name="info" size={14} />{t("rb-activity-note")}</p>
    {:else}
      <p class="muted rb-line"><Icon name="check" size={16} />{t("rb-none")}</p>
    {/if}
  </section>

  {#if asking && bin}
    <ConfirmDialog
      title={t("rb-confirm-title")}
      body={t("rb-confirm-body", { count: formatCount(bin.items, loc), size: formatSize(bin.bytes, loc) })}
      detail={drivesText || null}
      confirm={t("rb-empty")}
      onconfirm={empty}
      oncancel={() => (asking = false)}
    />
  {/if}
{/if}

<style>
  .rb {
    margin-bottom: 14px;
    border-left: 4px solid var(--risk-caution);
  }
  .rb.empty {
    border-left-color: var(--risk-safe);
  }
  .rb-head {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    flex-wrap: wrap;
  }
  .rb-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--risk-caution-ink);
  }
  .empty .rb-icon {
    color: var(--risk-safe-ink);
  }
  .rb-what {
    flex: 1;
    min-width: 240px;
  }
  .rb-what h2 {
    margin: 0 0 2px;
    font-size: 1.1em;
  }
  .rb-what p {
    margin: 0;
  }
  .rb-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: center;
  }
  .rb-facts {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    margin: 14px 0 6px;
  }
  .fact {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 150px;
    padding: 10px 14px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .fact b {
    font-size: 1.25em;
    font-variant-numeric: tabular-nums;
  }
  .fact span {
    font-size: 0.84em;
    color: var(--text-muted);
  }
  .rb-line {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 8px 0 0;
  }
  .small {
    font-size: 0.86em;
  }
</style>
