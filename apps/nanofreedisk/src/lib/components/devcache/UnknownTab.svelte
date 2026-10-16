<script lang="ts">
  // DevCache → Unknown (PLAN §4.3): cache-like folders no rule knows yet. Always 🟡
  // "unverified", shown with their size and why they look like a cache; never cleaned
  // — the user can open them or propose a rule (pre-filled GitHub issue, the folder
  // written as a variable so no user name leaves the machine).
  import { formatCount, formatDate, formatSize } from "../../format";
  import { intlLocale, t } from "../../i18n.svelte";
  import { fail } from "../../state.svelte";
  import { devApi } from "../../devcache/api";
  import { dev, loadUnknown } from "../../devcache/state.svelte";
  import Icon from "../Icon.svelte";

  const loc = $derived(intlLocale());
  $effect(() => {
    if (!dev.unknown && !dev.unknownLoading) void loadUnknown();
  });
  const total = $derived((dev.unknown ?? []).reduce((s, f) => s + f.bytes, 0));
  const why = (s: string) => (s === "dot-folder" ? t("dev-unknown-dot") : s);
</script>

<div class="card dc-note">
  <Icon name="alert" size={18} />
  <span class="dc-small">{t("dev-unknown-note")}</span>
  <span class="dc-grow"></span>
  <button class="btn ghost" onclick={() => loadUnknown()} disabled={dev.unknownLoading} title={t("clean-rescan")}><Icon name="refresh" /></button>
</div>

{#if !dev.unknown}
  <div class="card dc-loading"><Icon name="search" size={22} /><span>{t("dev-unknown-loading")}</span></div>
{:else if !dev.unknown.length}
  <div class="card dc-loading"><Icon name="check" size={22} /><span>{t("dev-unknown-none")}</span></div>
{:else}
  <div class="card dc-card">
    <h2 class="sum">{t("dev-unknown-count", { count: dev.unknown.length })}<span class="muted num">{formatSize(total, loc)}</span></h2>
    <ul class="dc-list">
      {#each dev.unknown as f (f.path)}
        <li class="unk" data-unknown={f.name}>
          <div class="what">
            <div class="dc-row">
              <span class="dc-badge caution">{t("dev-unverified")}</span>
              <strong>{f.name}</strong>
              <span class="muted dc-small mono ellipsis" title={f.path}>{f.path}</span>
            </div>
            <p class="dc-small muted">{t("dev-unknown-why", { signals: f.signals.map(why).join(", ") })} · {t("dev-last-write", { date: f.newest ? formatDate(f.newest, loc) : "—" })}</p>
          </div>
          <div class="dc-size">
            <b class="num">{formatSize(f.bytes, loc)}{f.truncated ? "+" : ""}</b>
            <span class="muted dc-small">{t("clean-files", { count: formatCount(f.files, loc) })}</span>
            <div class="dc-row">
              <button class="btn ghost icon" onclick={() => devApi.reveal(f.path).catch(fail)} title={t("dev-reveal")} aria-label={t("dev-reveal")}><Icon name="reveal" size={14} /></button>
              <button class="btn ghost small" onclick={() => devApi.propose(f.proposeUrl).catch(fail)} data-effect title={t("dev-propose-hint")}><Icon name="open" size={14} />{t("dev-propose")}</button>
            </div>
          </div>
        </li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .sum {
    display: flex;
    justify-content: space-between;
    margin: 0 0 8px;
    font-size: 1.05em;
  }
  .unk {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }
  .unk:first-child {
    border-top: none;
  }
  .what {
    min-width: 0;
  }
  .what p {
    margin: 4px 0 0;
  }
</style>
