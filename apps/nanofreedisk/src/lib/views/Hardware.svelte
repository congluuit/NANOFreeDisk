<script lang="ts">
  import { t } from "../i18n.svelte";
  import { hw, type HwTab } from "../hw/state.svelte";
  import Icon from "../components/Icon.svelte";
  import HardwareOverview from "./HardwareOverview.svelte";
  import HardwareSensors from "./HardwareSensors.svelte";
  import HardwareTasks from "./HardwareTasks.svelte";

  const TABS: { id: HwTab; icon: string; label: string }[] = [
    { id: "overview", icon: "cpu", label: "hw-tab-overview" },
    { id: "sensors", icon: "thermo", label: "hw-tab-sensors" },
    { id: "tasks", icon: "tasks", label: "hw-tab-tasks" },
  ];
</script>

<section class="page hardware">
  <header class="page-head">
    <div>
      <h1>{t("hw-title")}</h1>
      <p>{t("hw-subtitle")}</p>
    </div>
    <div class="segmented" role="tablist" aria-label={t("hw-title")}>
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
    {:else}
      <HardwareTasks />
    {/if}
  </div>
</section>

<style>
  .hardware {
    display: flex;
    flex-direction: column;
    padding-bottom: 0;
    min-height: 0;
    overflow: hidden;
  }
  .hardware > .page-head {
    margin-bottom: 14px;
  }
  .hwtab {
    padding: 5px 12px;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
</style>
