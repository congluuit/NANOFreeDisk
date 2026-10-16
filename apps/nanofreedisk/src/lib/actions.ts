// Actions shared by several views: the per-item context menu and the HTML report
// export. Deleting always goes through `askDelete` → DeleteDialog → the Rust safety
// checks; nothing here touches files directly.

import { api, inTauri, pickSaveFile } from "./api";
import type { MenuItem } from "./components/ContextMenu.svelte";
import { minBytesFor } from "./treemap";
import { intlLocale, t } from "./i18n.svelte";
import { buildReport } from "./report";
import { app, askDelete, fail, navigate, oldLargeOptions, select, toast } from "./state.svelte";
import { openDupesFor } from "./dupes/state.svelte";
import { paletteVars, resolvePalette } from "./theme";

export interface ItemMenuOptions {
  /** Offer "Move to Recycle Bin" (default true). Read-only lists pass false. */
  recycle?: boolean;
  /** Ids the delete acts on when the item is part of a multi-selection. */
  selection?: number[];
}

/** Context menu for one entry of the current scan. */
export function itemMenu(id: number, opts: ItemMenuOptions = {}): MenuItem[] {
  const rootId = app.summary?.rootId ?? 0;
  const items: MenuItem[] = [
    {
      icon: "zoom",
      label: "action-zoom",
      run: () =>
        api
          .detail(id)
          .then((d) => {
            const dir = d.kind === "dir" ? id : d.ancestors[d.ancestors.length - 2]?.id ?? rootId;
            navigate(dir);
            if (dir !== id) select(id);
            app.view = "analyze";
          })
          .catch(fail),
    },
    { icon: "reveal", label: "action-reveal", run: () => api.reveal(id).catch(fail) },
    { icon: "open", label: "action-open", run: () => api.openItem(id).catch(fail) },
    {
      icon: "copy",
      label: "action-copy-path",
      run: () =>
        api
          .detail(id)
          .then((d) => navigator.clipboard.writeText(d.path))
          .then(() => toast(t("copied"), "ok"))
          .catch(fail),
    },
    {
      icon: "duplicates",
      label: "action-find-dupes",
      hint: t("action-find-dupes-hint"),
      run: () =>
        api
          .detail(id)
          .then((d) => {
            // a file: look in its folder
            const folder = d.kind === "dir" ? d.path : d.path.replace(/[\\/][^\\/]*$/, "");
            openDupesFor(folder);
          })
          .catch(fail),
    },
  ];
  if (opts.recycle !== false) {
    items.push({
      icon: "trash",
      label: "action-recycle",
      hint: t("action-recycle-hint"),
      danger: true,
      disabled: id === rootId,
      run: () => askDelete(opts.selection?.length ? opts.selection : [id]),
    });
  }
  return items;
}

export function download(name: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: "text/html" }));
  const a = Object.assign(document.createElement("a"), { href: url, download: name });
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** Build the self-contained HTML report of the folder open in Analyze and save it. */
export async function exportHtmlReport(): Promise<void> {
  const summary = app.summary;
  if (!summary) return;
  const id = app.current;
  try {
    const folder = await api.detail(id);
    const [slice, children, topFiles, extensions, oldLarge] = await Promise.all([
      api.slice(id, 4, minBytesFor(folder.onDisk, 1000, 560, 24), 4000),
      api.children(id),
      api.topFiles(id, 50),
      api.extensions(id, 25),
      api.oldAndLarge(id, { ...oldLargeOptions(), limit: 1 }).catch(() => null),
    ]);
    // "What changed?" against the previous saved scan of the scanned folder
    let changes = null;
    if (app.snapshot != null) {
      const newer = app.snapshot;
      const older = (await api.historySnapshots(summary.root).catch(() => [])).find((s) => s.id !== newer);
      if (older) changes = await api.historyDiff(older.id, newer, 10 * 2 ** 20, 15).catch(() => null);
    }
    const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    const palette = resolvePalette(app.settings.theme, dark, app.info?.accent ?? null);
    const html = buildReport(
      {
        folder,
        summary,
        slice,
        children,
        topFiles,
        extensions,
        oldLarge,
        changes,
        vars: paletteVars(palette.colors, palette.dark),
        dark: palette.dark,
        version: app.info?.version ?? "",
        generatedAt: Date.now(),
        locale: intlLocale(),
      },
      t,
    );
    const name = "nanofreedisk-report.html";
    if (!inTauri) {
      download(name, html);
      return;
    }
    const path = await pickSaveFile(t("analyze-export-html"), name, "html");
    if (!path) return;
    await api.saveHtmlReport(path, html);
    toast(t("analyze-exported", { path }), "ok");
  } catch (e) {
    fail(e);
  }
}
