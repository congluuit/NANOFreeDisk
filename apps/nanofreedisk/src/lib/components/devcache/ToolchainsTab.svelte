<script lang="ts">
  // DevCache → Toolchains (C2): installed SDKs / runtimes / editors with size, last
  // write and the proper way to remove each. Read-only: the app never deletes a
  // toolchain folder, it shows the tool's own uninstall command to copy.
  import { formatDate, formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { fail } from "../../state.svelte";
  import { devApi } from "../../devcache/api";
  import { copyText, dev, loadTools } from "../../devcache/state.svelte";
  import type { Toolchain } from "../../devcache/types";
  import Icon from "../Icon.svelte";

  const loc = $derived(intlLocale());
  $effect(() => {
    if (!dev.tools && !dev.toolsLoading) void loadTools();
  });

  const families = $derived.by(() => {
    const out: { family: string; bytes: number; items: Toolchain[] }[] = [];
    for (const tc of dev.tools ?? []) {
      let f = out.find((x) => x.family === tc.family);
      if (!f) {
        f = { family: tc.family, bytes: 0, items: [] };
        out.push(f);
      }
      f.bytes += tc.bytes;
      f.items.push(tc);
    }
    for (const f of out) f.items.sort((a, b) => b.bytes - a.bytes);
    return out.sort((a, b) => b.bytes - a.bytes);
  });
</script>

<div class="card dc-note">
  <Icon name="shield" size={18} />
  <span class="dc-small">{t("dev-tools-note")}</span>
  <span class="dc-grow"></span>
  <button class="btn ghost" onclick={() => loadTools()} disabled={dev.toolsLoading} title={t("clean-rescan")}><Icon name="refresh" /></button>
</div>

{#if !dev.tools}
  <div class="card dc-loading"><Icon name="search" size={22} /><span>{t("dev-tools-loading")}</span></div>
{:else}
  {#each families as f (f.family)}
    <div class="card dc-card" data-family={f.family}>
      <h2 class="fam">{f.family}<span class="muted num">{formatSize(f.bytes, loc)}</span></h2>
      <ul class="dc-list">
        {#each f.items as tc (tc.path)}
          <li class="tool">
            <div class="name">
              {#if tc.default}<span class="dc-badge safe" title={t("dev-tool-default-hint")}>★ {t("dev-tool-default")}</span>{/if}
              <strong class="ellipsis" title={tc.path}>{tc.name}</strong>
              <span class="muted dc-small">{t("dev-last-write", { date: tc.newest ? formatDate(tc.newest, loc) : "—" })}</span>
            </div>
            <div class="remove">
              {#if tc.command}
                <span class="dc-code dc-small"><code class="mono">{tc.command}</code>
                  <button class="btn ghost icon" onclick={() => copyText(tc.command ?? "")} title={t("action-copy-command")} aria-label={t("action-copy-command")}><Icon name="copy" size={14} /></button>
                </span>
              {/if}
              {#if tc.guide}<span class="muted dc-small">{t(`dev-guide-${tc.guide}`)}</span>{/if}
            </div>
            <div class="dc-size">
              <b class="num">{formatSize(tc.bytes, loc)}{tc.truncated ? "+" : ""}</b>
              <button class="btn ghost icon" onclick={() => devApi.reveal(tc.path).catch(fail)} title={t("dev-reveal")} aria-label={t("dev-reveal")}><Icon name="reveal" size={14} /></button>
            </div>
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <div class="card dc-loading"><Icon name="check" size={22} /><span>{t("dev-tools-none")}</span></div>
  {/each}
{/if}

<style>
  .fam {
    display: flex;
    justify-content: space-between;
    margin: 0 0 8px;
    font-size: 1.05em;
  }
  .tool {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr) auto;
    gap: 12px;
    align-items: center;
    padding: 8px 0;
    border-top: 1px solid var(--border);
  }
  .tool:first-child {
    border-top: none;
  }
  .name {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .name .dc-badge {
    align-self: flex-start;
  }
  .remove {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .tool .dc-size {
    flex-direction: row;
    align-items: center;
  }
</style>
