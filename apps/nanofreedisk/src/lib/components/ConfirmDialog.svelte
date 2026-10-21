<script lang="ts">
  // A yes/no question before something that cannot be undone. Cancel has the
  // focus, Escape and a click outside cancel too.
  import { t } from "../i18n.svelte";
  import Icon from "./Icon.svelte";

  let {
    title,
    body,
    detail = null,
    confirm,
    icon = "trash",
    danger = true,
    onconfirm,
    oncancel,
  }: {
    title: string;
    body: string;
    /** A path or a list shown in a mono box under the body. */
    detail?: string | null;
    confirm: string;
    icon?: string;
    danger?: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  } = $props();
  let dialog = $state<HTMLDivElement | null>(null);

  $effect(() => {
    dialog?.querySelector<HTMLElement>(".cancel")?.focus();
  });
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && oncancel()} onkeydown={(e) => e.key === "Escape" && oncancel()}>
  <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-body" data-confirm bind:this={dialog}>
    <h2 id="confirm-title"><Icon name={icon} size={20} />{title}</h2>
    <p id="confirm-body">{body}</p>
    {#if detail}<p class="detail mono">{detail}</p>{/if}
    <footer>
      <button class="btn cancel" onclick={oncancel}>{t("delete-cancel")}</button>
      <button class="btn" class:danger data-confirm-go onclick={onconfirm} data-effect>
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
    width: min(520px, 92vw);
    padding: 22px 24px;
    box-shadow: var(--shadow);
  }
  h2 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0 0 8px;
    font-size: 1.15em;
  }
  p {
    margin: 0 0 8px;
  }
  .detail {
    padding: 6px 8px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    font-size: 0.86em;
    word-break: break-all;
    max-height: 140px;
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
