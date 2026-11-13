// Startup view state (docs/STARTUP.md): the list, the entry waiting for a "yes"
// before it is turned off, and the last change that can be undone.

import { t } from "../i18n.svelte";
import { pg } from "../programs/state.svelte";
import { fail, toast } from "../state.svelte";
import { startupApi } from "./api";
import type { StartupFilter, StartupSort } from "./model";
import type { StartupEntry } from "./types";

export const su = $state({
  list: null as StartupEntry[] | null,
  loading: false,
  query: "",
  filter: "all" as StartupFilter,
  sort: "name" as StartupSort,
  /** An entry being changed. */
  busy: null as string | null,
  /** Turning this one off waits for the user's yes (Windows, security software). */
  confirm: null as StartupEntry | null,
  /** The last change, to undo it. */
  undo: null as null | { id: string; name: string; enabled: boolean },
});

let listing: Promise<void> | null = null;
const iconsAsked = new Set<string>();

/** List the startup entries (concurrent calls share one listing). */
export function loadStartup(): Promise<void> {
  if (listing) return listing;
  su.loading = true;
  listing = startupApi
    .list()
    .then((l) => {
      su.list = l;
      void loadIcons(l.map((e) => e.id));
    })
    .catch(fail)
    .finally(() => {
      su.loading = false;
      listing = null;
    });
  return listing;
}

/** Icons go into the Programs view's icon map (ids never collide: `user-run|…`
 *  against `hkcu:…`), so the same AppIcon shows them. */
async function loadIcons(ids: string[]): Promise<void> {
  const todo = ids.filter((id) => !iconsAsked.has(id));
  todo.forEach((id) => iconsAsked.add(id));
  if (!todo.length) return;
  try {
    const got = await startupApi.icons(todo);
    if (Object.keys(got).length) pg.icons = { ...pg.icons, ...got };
  } catch {
    // no icon: the letter stays
  }
}

function replace(e: StartupEntry): void {
  if (su.list) su.list = su.list.map((x) => (x.id === e.id ? e : x));
}

/** Turn `e` on or off. `undoable`: offer to undo it afterwards. */
export async function setEnabled(e: StartupEntry, enabled: boolean, undoable = true): Promise<void> {
  su.confirm = null;
  if (su.busy) return;
  su.busy = e.id;
  try {
    const now = await startupApi.set(e.id, enabled);
    replace(now);
    su.undo = undoable ? { id: e.id, name: e.name, enabled } : null;
    toast(t(enabled ? "su-enabled" : "su-disabled", { name: e.name }), "ok");
  } catch (err) {
    const code = err && typeof err === "object" && "code" in err ? String((err as { code: unknown }).code) : "";
    if (code === "needs-admin") toast(t("su-needs-admin-error", { name: e.name }), "error");
    else if (code === "not-found") {
      toast(t("su-gone", { name: e.name }), "error");
      void loadStartup();
    } else fail(err);
  } finally {
    su.busy = null;
  }
}

/** Put the last change back. */
export async function undoLast(): Promise<void> {
  const u = su.undo;
  const e = u && su.list?.find((x) => x.id === u.id);
  su.undo = null;
  if (u && e) await setEnabled(e, !u.enabled, false);
}

export const openTaskManager = (): void => {
  startupApi.openTaskManager().catch(fail);
};
