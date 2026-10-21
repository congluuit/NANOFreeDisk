<script lang="ts">
  // A program's icon (read by the backend from its program file, its Windows
  // Installer icon or its Store logo), or its initial on a tinted tile when it has
  // none or the image cannot be shown.
  import { hue, initial } from "../../programs/model";
  import { pg } from "../../programs/state.svelte";

  let { id, name, size = 34 }: { id: string; name: string; size?: number } = $props();
  const icon = $derived(pg.icons[id]);
  let failed = $state<string | null>(null);
  const shown = $derived(icon && failed !== icon.src ? icon : null);
</script>

<span class="app-icon" class:letter={!shown} class:plate={shown?.plate} style="--s: {size}px; --h: {hue(name)}" aria-hidden="true" data-app-icon={shown ? "image" : "letter"}>
  {#if shown}
    <img src={shown.src} alt="" draggable="false" onerror={() => (failed = shown.src)} />
  {:else}
    {initial(name)}
  {/if}
</span>

<style>
  .app-icon {
    flex: none;
    width: var(--s);
    height: var(--s);
    display: grid;
    place-items: center;
    border-radius: calc(var(--s) * 0.26);
    overflow: hidden;
  }
  .letter {
    font-weight: 700;
    font-size: calc(var(--s) * 0.42);
    color: var(--text);
    background: color-mix(in srgb, hsl(var(--h) 60% 50%) 35%, var(--surface-2));
  }
  .plate {
    background: var(--accent);
    padding: calc(var(--s) * 0.12);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
</style>
