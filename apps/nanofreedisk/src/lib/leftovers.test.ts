// Clean view: what a clean left behind (grouped by cause, one action each), the
// browser mock of "try again" and the elevated run, and a message in both languages
// for every key and reason code the leftovers UI can show.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { ACTION_KEYS, ELEVATED_REASONS, LEFTOVER_KEYS, TOOLS, leftoverAction, leftoverBody, leftoverGroups, leftoverTitle, toolKey } from "./clean/leftovers.ts";
import { cleanHandlers } from "./clean/mock.ts";
import type { CleanScan, LeftoverGroup, OperationResult } from "./clean/types.ts";

function keys(lang: string): Set<string> {
  const out = new Set<string>();
  for (const file of ["clean.ftl", "app.ftl", "programs.ftl"]) {
    const text = readFileSync(new URL(`../../../../locales/${lang}/${file}`, import.meta.url), "utf8");
    for (const line of text.split(/\r?\n/)) {
      const m = /^([a-z][\w-]*)\s*=/.exec(line);
      if (m) out.add(m[1]);
    }
  }
  return out;
}

const group = (cause: LeftoverGroup["cause"], programs: string[] = [], reasons: (string | null)[] = [null]): LeftoverGroup => ({
  cause,
  programs,
  bytes: 10,
  items: reasons.map((reason, n) => ({ n, path: `C:\\x\\${n}`, ruleId: "r", isDir: false, bytes: 10, status: "failed", reason, programs })),
});

test("titles, explanations and actions per cause", () => {
  assert.deepEqual(leftoverTitle(group("in-use", ["Google Chrome", "Slack"])), { key: "clean-left-in-use-title", args: { apps: "Google Chrome, Slack" } });
  assert.equal(leftoverTitle(group("in-use")).key, "clean-left-in-use-unknown");
  assert.equal(leftoverTitle(group("needs-admin")).key, "clean-left-admin-title");
  assert.equal(leftoverBody(group("needs-admin")), "clean-left-admin-body");
  assert.equal(leftoverAction(group("in-use")), "retry");
  assert.equal(leftoverAction(group("needs-admin")), "admin");
  // looking again only helps when something changed since the scan
  assert.equal(leftoverAction(group("other", [], ["changed", "protected"])), "retry-other");
  assert.equal(leftoverAction(group("other", [], ["protected"])), null);
  assert.deepEqual(leftoverGroups({ leftovers: [group("other", [], [])] } as unknown as OperationResult), []);
  assert.deepEqual(leftoverGroups(null), []);
});

test("every leftovers message exists in English and Vietnamese", () => {
  for (const lang of ["en", "vi"]) {
    const have = keys(lang);
    const missing = [...LEFTOVER_KEYS, ...Object.values(ACTION_KEYS), ...TOOLS.map(toolKey), ...ELEVATED_REASONS.map((r) => `reason-${r}`), "reason-in-use", "reason-permissiondenied", "reason-uac-declined"].filter(
      (k) => !have.has(k),
    );
    assert.deepEqual(missing, [], lang);
  }
});

test("mock: a clean leaves in-use and admin items, retry and the elevated run take them", () => {
  const scan = cleanHandlers.clean_scan({}) as CleanScan;
  const ids = scan.rules.filter((r) => r.level === "safe" && r.candidates > 0).map((r) => r.ruleId);
  assert.ok(scan.rules.some((r) => r.admin) && scan.rules.some((r) => r.tool), "the mock has admin and tool rules");
  const first = cleanHandlers.clean_run({ ruleIds: ids, expert: false, permanent: false }) as OperationResult;
  const causes = leftoverGroups(first).map((g) => g.cause);
  assert.deepEqual(causes, ["in-use", "needs-admin"]);
  const inUse = leftoverGroups(first)[0];
  assert.ok(inUse.programs.length > 0, "names of the programs holding files");

  const retried = cleanHandlers.clean_retry({ cause: "in-use" }) as OperationResult;
  assert.equal(retried.failed, 0);
  assert.deepEqual(leftoverGroups(retried).map((g) => g.cause), ["needs-admin"], "the admin items are still offered");
  assert.throws(() => cleanHandlers.clean_retry({ cause: "in-use" }), (e: { code: string }) => e.code === "nothing-to-clean");

  const admin = cleanHandlers.clean_elevated({}) as OperationResult;
  assert.ok(admin.freed > 0);
  assert.deepEqual(leftoverGroups(admin), []);
  const row = (cleanHandlers.activity_list({ limit: 1 }) as { kind: string }[])[0];
  assert.equal(row.kind, "clean-admin");
  assert.throws(() => cleanHandlers.clean_elevated({}), (e: { code: string }) => e.code === "nothing-to-clean");

  // tools only for rules that name one
  assert.doesNotThrow(() => cleanHandlers.clean_open_tool({ ruleId: "windows.winsxs" }));
  assert.throws(() => cleanHandlers.clean_open_tool({ ruleId: "windows.temp.user" }));
});
