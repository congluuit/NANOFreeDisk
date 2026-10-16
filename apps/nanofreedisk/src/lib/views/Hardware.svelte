<script lang="ts">
  import { t } from "../i18n.svelte";
  import { hw, type HwTab } from "../hw/state.svelte";
  import Icon from "../components/Icon.svelte";
  import HardwareOverview from "./HardwareOverview.svelte";
  import HardwareSensors from "./HardwareSensors.svelte";

  const TABS: { id: HwTab; icon: string; label: string }[] = [
    { id: "overview", icon: "cpu", label: "hw-tab-overview" },
    { id: "sensors", icon: "thermo", label: "hw-tab-sensors" },
    { id: "tasks", icon: "tasks", label: "hw-tab-tasks" },
  ];
</script>

<section class="page">
  <header>
    <div>
      <h1>{t("hw-title")}</h1>
      <p class="muted">{t("hw-subtitle")}</p>
    </div>
    <div class="seg" role="tablist" aria-label={t("hw-title")}>
      {#each TABS as tab (tab.id)}
        <button
          role="tab"
          class="hwtab"
          data-hwtab={tab.id}
          class:on={hw.tab === tab.id}
          aria-selected={hw.tab === tab.id}
          onclick={() => (hw.tab = tab.id)}
        >
          <Icon name={tab.icon} size={16} />{t(tab.label)}
        </button>
      {/each}
    </div>
  </header>
  <div class="body">
    {#if hw.tab === "overview"}
      <HardwareOverview />
    {:else if hw.tab === "sensors"}
      <HardwareSensors />
    {/if}
  </div>
</section>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 24px 28px 0;
    gap: 16px;
    min-height: 0;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 1.6em;
  }
  header p {
    margin: 0;
    max-width: 70ch;
  }
  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--surface);
  }
  .hwtab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border: none;
    background: transparent;
    color: var(--text-muted);
  }
  .hwtab + .hwtab {
    border-left: 1px solid var(--border);
  }
  .hwtab:hover:not(.on) {
    background: var(--hover);
    color: var(--text);
  }
  .hwtab.on {
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 600;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
</style>
