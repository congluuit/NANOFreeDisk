<script lang="ts">
  // A verdict as a small pill (tree rows, suggestions, details): color from the theme
  // token of the verdict, icon + short label, the full meaning as tooltip.
  import { VERDICT_ICON, VERDICT_TOKEN } from "../advice";
  import { t } from "../i18n.svelte";
  import type { Verdict } from "../types";
  import Icon from "./Icon.svelte";

  interface Props {
    verdict: Verdict;
    /** Long label ("Safe to delete") instead of the short one. */
    long?: boolean;
    /** Icon only (narrow columns). */
    compact?: boolean;
  }
  let { verdict, long = false, compact = false }: Props = $props();
</script>

<span
  class="vb"
  class:compact
  data-verdict={verdict}
  style="--vc: var({VERDICT_TOKEN[verdict]})"
  title={`${t(`advice-verdict-${verdict}`)} — ${t(`advice-verdict-hint-${verdict}`)}`}
>
  <Icon name={VERDICT_ICON[verdict]} size={12} />
  {#if !compact}<span>{t(long ? `advice-verdict-${verdict}` : `advice-badge-${verdict}`)}</span>{/if}
</span>

<style>
  .vb {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 0.76em;
    line-height: 1.5;
    padding: 0 6px;
    border-radius: 9px;
    white-space: nowrap;
    color: var(--vc);
    background: color-mix(in srgb, var(--vc) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--vc) 38%, transparent);
  }
  .vb.compact {
    padding: 0 3px;
  }
</style>
