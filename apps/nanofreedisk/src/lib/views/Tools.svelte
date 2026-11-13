<script lang="ts">
  // Tools: every tool of the app as a card (icon, name, when to use it), so Simple
  // mode can keep a short sidebar and still reach everything. Only what this
  // platform has is listed.
  import { t } from "../i18n.svelte";
  import { toolCards, type ToolCard } from "../nav";
  import { currentOs, viewAvailable } from "../platform";
  import { app } from "../state.svelte";
  import Icon from "../components/Icon.svelte";

  const os = $derived(currentOs(app.info?.os));
  const cards = $derived(toolCards((v) => viewAvailable(os, v)));
  const sections = $derived(
    (["space", "health"] as const).map((s) => ({ id: s, cards: cards.filter((c: ToolCard) => c.section === s) })).filter((s) => s.cards.length),
  );
</script>

<section class="page tools">
  <div class="wrap">
    <header class="page-head">
      <div>
        <h1>{t("tools-title")}</h1>
        <p>{t("tools-intro")}</p>
      </div>
    </header>

    <button class="quick card" data-view="quick" onclick={() => (app.view = "quick")}>
      <span class="ic"><Icon name="clean" size={22} /></span>
      <span class="text">
        <strong>{t("nav-quick")}</strong>
        <span class="muted small">{t("quick-intro")}</span>
      </span>
      <Icon name="chevron" size={16} />
    </button>

    {#each sections as s (s.id)}
      <section class="section" aria-labelledby={`tools-${s.id}`}>
        <h2 id={`tools-${s.id}`}>{t(`tools-section-${s.id}`)}</h2>
        <ul class="grid">
          {#each s.cards as c (c.id)}
            <li>
              <button class="tool card" data-view={c.id} onclick={() => (app.view = c.id)}>
                <span class="ic"><Icon name={c.icon} size={22} /></span>
                <span class="text">
                  <strong>{t(c.label)}</strong>
                  <span class="muted small">{t(c.when)}</span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  </div>
</section>

<style>
  .wrap {
    max-width: 1040px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .wrap > .page-head {
    margin-bottom: 0;
  }
  h2 {
    margin: 0 0 10px;
    font-size: 0.86em;
    font-weight: 700;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    gap: 12px;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .grid li {
    display: flex;
  }
  button.card {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    width: 100%;
    padding: 16px;
    color: var(--text);
    text-align: left;
    transition:
      border-color 0.12s,
      background 0.12s;
  }
  button.card:hover {
    border-color: var(--accent);
    background: var(--hover);
  }
  .quick {
    align-items: center;
    border-left: 4px solid var(--accent);
  }
  .quick > :global(svg:last-child) {
    color: var(--text-muted);
  }
  .ic {
    flex: none;
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--accent-ink);
  }
  .quick .ic {
    background: var(--accent-soft);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 1;
    min-width: 0;
  }
  .text strong {
    font-weight: 600;
  }
</style>
