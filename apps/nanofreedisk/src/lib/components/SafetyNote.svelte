<script lang="ts">
  // The one sentence every removing tool shows about where removed items go and how
  // to get them back (docs/PHASE3_CLEAN.md, "Simple mode: one destination"). Simple
  // mode always says the quarantine; Expert mode says what the tool really uses.
  import { safetyNoteKey, type ShownMode } from "../clean/destination";
  import { t } from "../i18n.svelte";
  import Icon from "./Icon.svelte";

  let { mode = "quarantine", plain = false }: { mode?: ShownMode; plain?: boolean } = $props();
</script>

<span class="safety-note" class:tag={!plain} class:safe={!plain && mode !== "permanent"} class:plain data-safety-note={mode}>
  <Icon name={mode === "permanent" ? "alert" : "shield"} size={13} />{t(safetyNoteKey(mode))}
</span>

<style>
  .safety-note {
    display: inline-flex;
    gap: 6px;
    align-items: flex-start;
    font-weight: 500;
    white-space: normal;
    line-height: 1.35;
    max-width: 72ch;
    width: fit-content;
  }
  .safety-note.tag {
    border-radius: var(--radius-sm);
    padding: 3px 9px;
    margin-top: 6px;
  }
  .safety-note :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .plain {
    max-width: none;
    font-weight: 400;
  }
</style>
