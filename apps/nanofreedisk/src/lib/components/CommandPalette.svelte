<script lang="ts">
  // Command palette (Ctrl+K, PLAN §13 G3): one search box for views, actions, drives,
  // folders of the current scan and settings. Keyboard-first: ↑/↓, Enter, Esc.
  import { api, pickFolder } from "../api";
  import { exportHtmlReport } from "../actions";
  import { formatSize } from "../format";
  import { LOCALES, intlLocale, t } from "../i18n.svelte";
  import { rank } from "../palette";
  import { app, fail, navigate, startScan, type View } from "../state.svelte";
  import type { ThemeMode } from "../theme";
  import Icon from "./Icon.svelte";

  type Group = "go" | "action" | "drive" | "folder" | "setting";
  interface Item {
    key: string;
    group: Group;
    label: string;
    hint?: string;
    keywords?: string;
    icon: string;
    run: () => void;
  }

  let query = $state("");
  let active = $state(0);
  let folders = $state<Item[]>([]);
  let input: HTMLInputElement;
  let list: HTMLDivElement;
  const loc = $derived(intlLocale());

  const close = () => (app.palette = false);
  const go = (view: View) => () => (app.view = view);
  // the message id doubles as English keywords, so English words work in any locale
  const words = (id: string) => id.replace(/^(nav|palette|settings|analyze)-/, "").replace(/-/g, " ");

  const staticItems = $derived.by((): Item[] => {
    const items: Item[] = [];
    const nav: [View, string, string][] = [
      ["home", "nav-home", "home"],
      ["analyze", "nav-analyze", "analyze"],
      ["oldlarge", "nav-oldlarge", "drive"],
      ["tidy", "nav-tidy", "folder"],
      ["devcache", "nav-devcache", "devcache"],
      ["duplicates", "nav-duplicates", "duplicates"],
      ["monitor", "nav-monitor", "monitor"],
      ["settings", "nav-settings", "settings"],
    ];
    for (const [view, id, icon] of nav) {
      items.push({ key: `go-${view}`, group: "go", label: t(id), keywords: words(id), icon, run: go(view) });
    }
    items.push({
      key: "scan-folder",
      group: "action",
      label: t("home-scan-folder"),
      keywords: "scan folder pick",
      icon: "folder",
      run: async () => {
        const path = await pickFolder(t("home-pick-folder"));
        if (path) await startScan(path);
      },
    });
    if (app.summary) {
      const root = app.summary.root;
      items.push(
        { key: "rescan", group: "action", label: t("analyze-rescan"), hint: root, keywords: "rescan refresh", icon: "refresh", run: () => startScan(root) },
        { key: "report", group: "action", label: t("analyze-export-html"), keywords: "export html report", icon: "export", run: () => void exportHtmlReport() },
      );
    }
    const other = app.settings.mode === "expert" ? "simple" : "expert";
    items.push({
      key: "mode",
      group: "action",
      label: t(`palette-mode-${other}`),
      hint: t(`mode-${other}-hint`),
      keywords: `mode ${other} simple expert`,
      icon: "settings",
      run: () => (app.settings.mode = other),
    });
    for (const v of app.volumes.filter((v) => v.ready)) {
      items.push({
        key: `drive-${v.root}`,
        group: "drive",
        label: t("palette-scan-drive", { drive: `${v.label || t(`drive-kind-${v.kind}`)} (${v.root.replace(/\\$/, "")})` }),
        hint: v.total ? t("drive-free", { free: formatSize(v.free, loc) }) : undefined,
        keywords: `${v.root} scan drive analyze`,
        icon: v.kind === "removable" ? "usb" : v.kind === "network" ? "network" : "drive",
        run: () => startScan(v.root),
      });
    }
    for (const l of LOCALES) {
      items.push({
        key: `lang-${l.id}`,
        group: "setting",
        label: `${t("settings-language")}: ${l.label}`,
        keywords: "language ngon ngu",
        icon: "settings",
        run: () => (app.settings.locale = l.id),
      });
    }
    for (const m of ["light", "dark", "system", "custom"] as ThemeMode[]) {
      items.push({
        key: `theme-${m}`,
        group: "setting",
        label: `${t("settings-theme")}: ${t(`settings-theme-${m}`)}`,
        keywords: `theme ${m}`,
        icon: m === "light" ? "sun" : "disc",
        run: () => {
          app.settings.theme.mode = m;
          if (m === "custom") app.view = "settings";
        },
      });
    }
    const scale = (d: number) => () => (app.settings.fontScale = Math.min(1.4, Math.max(0.85, Math.round((app.settings.fontScale + d) * 100) / 100)));
    items.push(
      { key: "font-up", group: "setting", label: t("palette-font-larger"), keywords: "text size font larger zoom", icon: "zoom", run: scale(0.05) },
      { key: "font-down", group: "setting", label: t("palette-font-smaller"), keywords: "text size font smaller", icon: "zoom", run: scale(-0.05) },
    );
    return items;
  });

  const results = $derived([...rank(staticItems, query, 40), ...folders]);

  // folders of the current scan, searched on the in-memory index as you type
  $effect(() => {
    const q = query.trim();
    const summary = app.summary;
    if (!summary || q.length < 2) {
      folders = [];
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      api
        .search(summary.rootId, { name: q, kind: "dirs" }, 12)
        .then((r) => {
          if (cancelled) return;
          folders = r.rows.map((row) => ({
            key: `dir-${row.id}`,
            group: "folder" as const,
            label: row.name,
            hint: `${formatSize(row.onDisk, loc)} · ${row.path}`,
            icon: "folder",
            run: () => {
              navigate(row.id);
              app.view = "analyze";
            },
          }));
        })
        .catch(fail);
    }, 120);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  $effect(() => {
    void results.length;
    active = Math.min(active, Math.max(0, results.length - 1));
  });

  $effect(() => {
    input?.focus();
  });

  function run(item: Item | undefined) {
    if (!item) return;
    close();
    try {
      item.run();
    } catch (e) {
      fail(e);
    }
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = results.length;
      if (n) active = (active + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
      list?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(results[active]);
    }
  }
</script>

<div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && close()}>
  <div class="palette card" role="dialog" aria-modal="true" aria-label={t("palette-title")}>
    <label class="search">
      <Icon name="search" size={18} />
      <input
        bind:this={input}
        bind:value={query}
        oninput={() => (active = 0)}
        onkeydown={onkey}
        placeholder={t("palette-placeholder")}
        aria-label={t("palette-placeholder")}
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={results.length ? `palette-${active}` : undefined}
        spellcheck="false"
      />
      <kbd>Esc</kbd>
    </label>
    <div class="list" id="palette-list" role="listbox" bind:this={list}>
      {#each results as item, i (item.key)}
        {#if i === 0 || results[i - 1].group !== item.group}
          <div class="group">{t(`palette-group-${item.group}`)}</div>
        {/if}
        <div
          class="item"
          class:on={i === active}
          id={`palette-${i}`}
          data-i={i}
          role="option"
          tabindex="-1"
          aria-selected={i === active}
          onmousemove={() => (active = i)}
          onclick={() => run(item)}
          onkeydown={() => {}}
        >
          <Icon name={item.icon} size={16} />
          <span class="ellipsis label">{item.label}</span>
          {#if item.hint}<span class="ellipsis muted hint">{item.hint}</span>{/if}
        </div>
      {:else}
        <p class="muted nothing">{t("palette-nothing")}</p>
      {/each}
    </div>
    <div class="foot muted">
      <span><kbd>↑</kbd><kbd>↓</kbd> {t("palette-move")}</span>
      <span><kbd>Enter</kbd> {t("palette-run")}</span>
      {#if app.summary}<span>{t("palette-folders-hint")}</span>{/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    background: color-mix(in srgb, var(--bg) 55%, transparent);
  }
  .palette {
    width: min(640px, calc(100vw - 32px));
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    background: var(--surface-2);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
  }
  .search input {
    flex: 1;
    border: none;
    background: transparent;
    outline: none;
    font-size: 1.08em;
    color: var(--text);
    min-width: 0;
  }
  .list {
    overflow: auto;
    padding: 4px 6px 8px;
  }
  .group {
    padding: 8px 8px 4px;
    font-size: 0.75em;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-muted);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    min-width: 0;
  }
  .item.on {
    background: var(--accent-soft);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .label {
    flex: none;
    max-width: 60%;
  }
  .hint {
    flex: 1;
    text-align: right;
    font-size: 0.86em;
  }
  .nothing {
    margin: 14px;
  }
  .foot {
    display: flex;
    gap: 16px;
    padding: 6px 14px;
    border-top: 1px solid var(--border);
    font-size: 0.8em;
  }
  kbd {
    display: inline-block;
    padding: 0 5px;
    margin-right: 2px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 4px;
    font-family: inherit;
    font-size: 0.9em;
    background: var(--surface);
    color: var(--text-muted);
  }
</style>
