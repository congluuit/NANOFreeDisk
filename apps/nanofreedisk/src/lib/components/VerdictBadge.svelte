<script lang="ts">
  // A verdict as a small pill (tree rows, suggestions, details): color from the theme
  // token of the verdict, icon + short label, the full meaning as tooltip.
  import { PROTECTED_TOKEN, VERDICT_ICON, VERDICT_TOKEN } from "../advice";
  import { t } from "../i18n.svelte";
  import type { Verdict } from "../types";
  import Icon from "./Icon.svelte";

  interface Props {
    verdict: Verdict;
    /** Long label ("Safe to delete") instead of the short one. */
    long?: boolean;
    /** Icon only (narrow columns). */
    compact?: boolean;
    /** A hard protection rather than the app's judgment. */
    hard?: boolean;
  }
  let { verdict, long = false, compact = false, hard = false }: Props = $props();
  const key = $derived(hard ? "protected" : verdict);
</script>

<span
  class="vb"
  class:compact
  data-verdict={verdict}
  data-hard={hard ? "" : undefined}
  style="--vc: var({hard ? PROTECTED_TOKEN : VERDICT_TOKEN[verdict]})"
  title={`${t(`advice-verdict-${key}`)} — ${t(`advice-verdict-hint-${key}`)}`}
>
  <Icon name={hard ? "lock" : VERDICT_ICON[verdict]} size={12} />
  {#if !compact}<span>{t(long ? `advice-verdict-${key}` : `advice-badge-${key}`)}</span>{/if}
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
