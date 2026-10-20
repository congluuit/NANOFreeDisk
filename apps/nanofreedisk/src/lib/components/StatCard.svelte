<script lang="ts">
  // One figure of a view's stat row: a small label (with a meaning dot), the number
  // big, its unit or count small next to it. Clickable when it leads somewhere.
  import type { Snippet } from "svelte";

  interface Props {
    label: string;
    /** Big number, already formatted ("27 GB"); null shows a dash. */
    value: string | null;
    /** Small text after the number ("of 926 GB", "6 items"). */
    unit?: string;
    /** Meaning dot before the label. */
    tone?: "safe" | "caution" | "advanced" | "protected" | "accent" | null;
    /** Tooltip. */
    hint?: string;
    onclick?: () => void;
    disabled?: boolean;
    /** Extra content under the number (a bar, a note). */
    children?: Snippet;
    [attr: `data-${string}`]: string | boolean | undefined;
  }
  let { label, value, unit = "", tone = null, hint, onclick, disabled = false, children, ...rest }: Props = $props();
</script>

{#if onclick}
  <button class="stat card click" {onclick} {disabled} title={hint} data-effect={hint ? true : undefined} {...rest}>
    {@render body()}
  </button>
{:else}
  <div class="stat card" title={hint} {...rest}>
    {@render body()}
  </div>
{/if}

{#snippet body()}
  <span class="label">
    {#if tone}<span class="dot {tone}"></span>{/if}
    <span class="ellipsis">{label}</span>
  </span>
  <span class="value">
    <b>{value ?? "—"}</b>{#if unit}<span class="unit">{unit}</span>{/if}
  </span>
  {@render children?.()}
{/snippet}

<style>
  .stat {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    padding: 13px 16px 14px;
    text-align: left;
    color: var(--text);
  }
  .click {
    font: inherit;
    transition: border-color 0.12s, background 0.12s;
  }
  .click:hover:not(:disabled) {
    border-color: var(--text-muted);
  }
  .click:disabled {
    cursor: default;
  }
  .label {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: 0.86em;
    color: var(--text-muted);
  }
  .value {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    column-gap: 6px;
    min-width: 0;
  }
  b {
    font-size: 1.85em;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.15;
    font-variant-numeric: tabular-nums;
  }
  .unit {
    font-size: 0.86em;
    color: var(--text-muted);
  }
</style>
