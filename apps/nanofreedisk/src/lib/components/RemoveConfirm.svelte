<script lang="ts">
  // The one confirmation of every removing tool (Junk files, Tidy up, Old & large,
  // Duplicates, Analyze): what goes and how much (the caller's list and totals),
  // where it goes and how to undo it (the safety note), then Cancel / confirm.
  // In Simple mode a large removal, or one from an almost full drive, may choose
  // the Recycle Bin instead of the quarantine (`choice`). Cancel has the focus;
  // Escape and a click outside cancel too.
  import type { Snippet } from "svelte";
  import { isBigRemoval, lowDrives, type ShownMode } from "../clean/destination";
  import { formatSize } from "../format";
  import { intlLocale, t } from "../i18n.svelte";
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import SafetyNote from "./SafetyNote.svelte";

  interface Props {
    title: string;
    icon?: string;
    /** Where the items go; bound so the big-removal choice can change it. */
    mode?: ShownMode;
    /** Bytes that go (the big-removal test and its sentence). */
    bytes: number;
    /** Paths that go (which drives are almost full). */
    paths?: string[];
    /** Offer the Recycle Bin for a big removal (Simple mode, tools whose backend
     *  takes a destination). */
    choice?: boolean;
    /** One line under the safety note (e.g. "checked again right before it moves"). */
    note?: string | null;
    confirm: string;
    disabled?: boolean;
    busy?: boolean;
    /** Extra marker attribute on the dialog for tests. */
    testid?: string;
    /** What goes and how much. */
    children: Snippet;
    /** Under "where": options and acknowledgements (Expert permanent delete…). */
    extra?: Snippet;
    onconfirm: () => void;
    oncancel: () => void;
  }
  let {
    title,
    icon = "trash",
    mode = $bindable("quarantine"),
    bytes,
    paths = [],
    choice = false,
    note = null,
    confirm,
    disabled = false,
    busy = false,
    testid = "",
    children,
    extra,
    onconfirm,
    oncancel,
  }: Props = $props();
  let dialog = $state<HTMLDivElement | null>(null);
  const loc = $derived(intlLocale());
  const big = $derived(choice && mode !== "permanent" && isBigRemoval(bytes, paths, app.volumes));
  const low = $derived(lowDrives(paths, app.volumes));

  $effect(() => {
    dialog?.querySelector<HTMLElement>(".cancel")?.focus();
  });

  const cancel = () => !busy && oncancel();
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && cancel()} onkeydown={(e) => e.key === "Escape" && cancel()}>
  <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="remove-title" data-remove-confirm={testid} bind:this={dialog}>
    <h2 id="remove-title"><Icon name={icon} size={20} />{title}</h2>
    <section class="what" aria-label={t("confirm-what")}>
      {@render children()}
    </section>
    <section class="where" data-confirm-where={mode}>
      <h3>{t("confirm-where")}</h3>
      {#if big}
        <p class="big" data-confirm-big>{t("confirm-big", { size: formatSize(bytes, loc) })}{#if low.length}<span class="muted"> ({low.map((v) => v.root.replace(/[\\/]$/, "")).join(", ")})</span>{/if}</p>
        <label class="choice">
          <input type="radio" name="remove-where" value="quarantine" bind:group={mode} disabled={busy} data-confirm-choice="quarantine" />
          <span>{t("confirm-choice-quarantine")}</span>
        </label>
        <label class="choice">
          <input type="radio" name="remove-where" value="recycle-bin" bind:group={mode} disabled={busy} data-confirm-choice="recycle-bin" />
          <span>{t("confirm-choice-recycle")}</span>
        </label>
      {:else}
        <SafetyNote {mode} plain />
      {/if}
      {#if mode === "quarantine"}
        <p class="muted small">{t("confirm-space-later")}</p>
      {/if}
      {#if note}<p class="muted small">{note}</p>{/if}
    </section>
    {@render extra?.()}
    <footer>
      <button class="btn cancel" onclick={cancel} disabled={busy}>{t("delete-cancel")}</button>
      <button class="btn danger" data-confirm data-confirm-go onclick={onconfirm} disabled={disabled || busy} data-effect title={note ?? confirm}>
        <Icon name={icon} size={16} />{confirm}
      </button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--bg) 65%, transparent);
    backdrop-filter: blur(2px);
  }
  .dialog {
    width: min(660px, 94vw);
    max-height: 86vh;
    overflow: auto;
    padding: 22px 24px;
    box-shadow: var(--shadow);
  }
  h2 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0 0 8px;
    font-size: 1.2em;
  }
  .what :global(p) {
    margin: 6px 0;
  }
  .where {
    margin-top: 12px;
    padding: 10px 12px;
    border-left: 4px solid var(--risk-safe);
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .where[data-confirm-where="permanent"] {
    border-left-color: var(--danger);
  }
  .where[data-confirm-where="recycle-bin"] {
    border-left-color: var(--accent);
  }
  .where h3 {
    margin: 0 0 6px;
    font-size: 0.86em;
    font-weight: 600;
    color: var(--text-muted);
  }
  .where p {
    margin: 6px 0 0;
  }
  .where .big {
    margin: 0 0 6px;
    font-weight: 500;
  }
  .choice {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 3px 0;
  }
  .small {
    font-size: 0.86em;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
</style>
