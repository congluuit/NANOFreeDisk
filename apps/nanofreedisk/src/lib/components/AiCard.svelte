<script lang="ts">
  // Home: what AI coding agents left behind in finished sessions (PLAN §13.2 C10).
  // Looked up once in the background a few seconds after Home first shows
  // (read-only); a click opens DevCache → AI leftovers.
  import { onMount } from "svelte";
  import { formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app } from "../state.svelte";
  import { dev, loadAi } from "../devcache/state.svelte";
  import { aiChecked, markAiChecked } from "../devcache/home";
  import Icon from "./Icon.svelte";

  const loc = $derived(intlLocale());

  onMount(() => {
    if (aiChecked()) return;
    const timer = setTimeout(() => {
      markAiChecked();
      if (!dev.ai && !dev.aiLoading) void loadAi(true);
    }, 4000);
    return () => clearTimeout(timer);
  });

  function open() {
    dev.tab = "ai";
    app.view = "devcache";
  }
</script>

{#if dev.ai && dev.ai.selectedBytes > 0}
  <button class="ai card" data-home-ai onclick={open} title={t("ail-finished-hint")}>
    <span class="ic"><Icon name="agent" size={22} /></span>
    <span class="txt">
      <strong>{t("home-ai-title")}</strong>
      <span class="muted">{t("home-ai-body", { size: formatSize(dev.ai.selectedBytes, loc) })}</span>
    </span>
    <span class="go">{t("home-ai-open")}<Icon name="chevron" size={14} /></span>
  </button>
{/if}

<style>
  .ai {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    padding: 14px 18px;
    margin-bottom: 22px;
    text-align: left;
    color: inherit;
    font: inherit;
    transition: border-color 0.12s;
  }
  .ai:hover {
    border-color: var(--text-muted);
  }
  .ic {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: var(--risk-safe-soft);
    color: var(--risk-safe-ink);
  }
  .txt .muted {
    font-size: 0.88em;
  }
  .txt {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .go {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-muted);
    font-size: 0.9em;
    white-space: nowrap;
  }
</style>
