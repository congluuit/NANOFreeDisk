<script lang="ts" module>
  export interface MenuItem {
    icon: string;
    label: string;
    hint?: string;
    danger?: boolean;
    disabled?: boolean;
    run: () => void;
  }
</script>

<script lang="ts">
  import { t } from "../i18n.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    x: number;
    y: number;
    items: MenuItem[];
    onclose: () => void;
  }
  let { x, y, items, onclose }: Props = $props();
  let menu: HTMLDivElement;

  $effect(() => {
    menu?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== "Escape") return;
      if (e instanceof MouseEvent && menu?.contains(e.target as Node)) return;
      onclose();
    };
    window.addEventListener("mousedown", close, true);
    window.addEventListener("keydown", close, true);
    window.addEventListener("blur", onclose);
    return () => {
      window.removeEventListener("mousedown", close, true);
      window.removeEventListener("keydown", close, true);
      window.removeEventListener("blur", onclose);
    };
  });

  const pos = $derived({
    left: Math.min(x, window.innerWidth - 250),
    top: Math.min(y, window.innerHeight - items.length * 36 - 16),
  });
</script>

<div class="menu card" role="menu" bind:this={menu} style="left: {pos.left}px; top: {pos.top}px">
  {#each items as item (item.label)}
    <button
      role="menuitem"
      class:danger={item.danger}
      disabled={item.disabled}
      title={item.hint}
      onclick={() => {
        onclose();
        item.run();
      }}
    >
      <Icon name={item.icon} size={16} />
      <span>{t(item.label)}</span>
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 60;
    min-width: 220px;
    padding: 4px;
    box-shadow: var(--shadow);
    background: var(--surface-2);
  }
  button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    border: none;
    background: none;
    text-align: left;
    border-radius: 4px;
  }
  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--hover);
    outline: none;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .danger {
    color: var(--danger);
  }
</style>
