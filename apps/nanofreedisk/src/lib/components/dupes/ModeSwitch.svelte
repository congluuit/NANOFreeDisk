<script lang="ts">
  // The two halves of the Duplicates view: byte-identical files, or photos that
  // only look alike (SimilarPhotos.svelte).
  import { t } from "../../i18n.svelte";
  import { similar } from "../../similar/state.svelte";
  import type { DupesMode } from "../../similar/types";

  const MODES: DupesMode[] = ["identical", "similar"];
</script>

<div class="segmented mode" role="radiogroup" aria-label={t("similar-mode-label")} data-dupes-mode>
  {#each MODES as m (m)}
    <button
      role="radio"
      aria-checked={similar.mode === m}
      class:on={similar.mode === m}
      onclick={() => (similar.mode = m)}
      data-mode={m}
      title={t(`similar-mode-${m}-hint`)}
    >{t(`similar-mode-${m}`)}</button>
  {/each}
</div>

<style>
  .mode {
    align-self: flex-start;
  }
</style>
