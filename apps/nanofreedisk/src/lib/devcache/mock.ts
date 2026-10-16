// Browser-only stand-in for the DevCache commands (`pnpm dev` without Tauri): a
// developer PC with AI tools, IDEs, package managers, a few repositories and
// toolchains. Cleaning "frees" the bytes; nothing touches a real disk.

import type { OperationResult, RuleSummary } from "../clean/types";
import type { DevCacheReport, DevRule, RepoScan, StoreInfo, Toolchain, UnknownFolder, VendorGroup } from "./types";

const MB = 2 ** 20;
const GB = 2 ** 30;
const DAY = 86_400;
const now = () => Math.floor(Date.now() / 1000);
const HOME = "C:\\Users\\nano";

function rule(
  ruleId: string,
  vendor: string,
  app: string,
  level: RuleSummary["level"],
  bytes: number,
  impact: [string, string],
  extra: Partial<DevRule> = {},
): DevRule {
  return {
    ruleId,
    packId: "dev",
    app,
    vendor,
    category: "dev",
    level,
    impact: impact[0],
    impactVi: impact[1],
    requiresClosed: [],
    running: [],
    bytes,
    files: Math.max(1, Math.round(bytes / (180 * 1024))),
    candidates: bytes ? Math.max(1, Math.round(bytes / (40 * MB))) : 0,
    skipped: 0,
    refused: 0,
    truncated: false,
    newest: now() - 3 * DAY,
    ...extra,
  };
}

const CACHE: [string, string] = [
  "Caches and logs; rebuilt automatically, settings are kept.",
  "Bộ đệm và nhật ký; được tạo lại tự động, giữ nguyên thiết lập.",
];

let rules: DevRule[] = [];
function reset(): void {
  rules = [
    rule("anthropic.claude-desktop.cache", "Anthropic", "Claude Desktop", "safe", 358 * MB, CACHE, {
      requiresClosed: ["Claude.exe"],
      running: ["Claude.exe"],
    }),
    rule("anthropic.claude-code.sessions", "Anthropic", "Claude Code", "caution", 1.4 * GB, [
      "Session transcripts used by `claude --resume`; your code is untouched.",
      "Bản ghi phiên dùng cho `claude --resume`; mã nguồn không bị ảnh hưởng.",
    ]),
    rule("anthropic.claude-code.config", "Anthropic", "Claude Code", "protected", 0, [
      "Global config, OAuth state and permissions.",
      "Cấu hình chung, trạng thái OAuth và quyền.",
    ]),
    rule("ollama.models", "Ollama", "Ollama", "caution", 61 * GB, [
      "Downloaded models; `ollama pull` again to get them back.",
      "Model đã tải; `ollama pull` lại để lấy về.",
    ]),
    rule("microsoft.vscode.cache", "Microsoft", "Visual Studio Code", "safe", 1.0 * GB, CACHE, { requiresClosed: ["Code.exe"] }),
    rule("microsoft.vscode.logs", "Microsoft", "Visual Studio Code", "safe", 231 * MB, CACHE),
    rule("microsoft.vscode.config", "Microsoft", "Visual Studio Code", "protected", 0, [
      "Settings, keybindings and installed extensions.",
      "Thiết lập, phím tắt và extension đã cài.",
    ]),
    rule("python.pip.cache", "Python", "pip", "safe", 3.7 * GB, [
      "pip's wheel cache; downloaded again on the next install.",
      "Bộ đệm wheel của pip; tải lại ở lần cài sau.",
    ]),
    rule("gradle.caches", "Gradle", "Gradle", "caution", 5.8 * GB, [
      "Dependencies and build caches; the next build downloads them again.",
      "Dependency và bộ đệm build; lần build sau tải lại.",
    ]),
    rule("unity.hub.cache", "Unity", "Unity Hub", "safe", 9.5 * MB, CACHE),
  ];
}
reset();

function report(): DevCacheReport {
  const vendors: VendorGroup[] = [];
  for (const r of rules) {
    let v = vendors.find((x) => x.vendor === r.vendor);
    if (!v) {
      v = { vendor: r.vendor, bytes: 0, files: 0, safeBytes: 0, cautionBytes: 0, advancedBytes: 0, newest: 0, running: [], threshold: r.vendor === "Ollama" ? 50 * GB : null, overThreshold: false, apps: [] };
      vendors.push(v);
    }
    let a = v.apps.find((x) => x.app === r.app);
    if (!a) {
      a = { app: r.app, bytes: 0, files: 0, safeBytes: 0, newest: 0, running: [], rules: [], kept: [] };
      v.apps.push(a);
    }
    if (r.level === "protected") {
      a.kept.push({ ...r });
      continue;
    }
    if (!r.candidates) continue;
    a.rules.push({ ...r });
    for (const g of [a, v]) {
      g.bytes += r.bytes;
      g.files += r.files;
      g.newest = Math.max(g.newest, r.newest);
      for (const x of r.running) if (!g.running.includes(x)) g.running.push(x);
    }
    if (r.level === "safe") {
      a.safeBytes += r.bytes;
      v.safeBytes += r.bytes;
    } else if (r.level === "caution") v.cautionBytes += r.bytes;
    else v.advancedBytes += r.bytes;
  }
  for (const v of vendors) v.overThreshold = v.threshold != null && v.bytes >= v.threshold;
  vendors.sort((a, b) => b.bytes - a.bytes);
  const sum = (l: string) => rules.filter((r) => r.level === l && r.candidates).reduce((s, r) => s + r.bytes, 0);
  return { vendors, safe: sum("safe"), caution: sum("caution"), advanced: sum("advanced"), kept: rules.filter((r) => r.level === "protected").length };
}

function opResult(freed: number, done: number): OperationResult {
  return { id: `mock-dev-${now()}`, freed, done, partial: 0, skipped: 0, failed: 0, cancelled: false, problems: [], journalError: null };
}

const tot = (bytes: number, files: number, daysAgo: number) => ({ bytes, files, newest: now() - daysAgo * DAY, truncated: false });

const TOOLS: Toolchain[] = [
  { tool: "rustup", family: "Rust (rustup)", name: "stable-x86_64-pc-windows-msvc", path: `${HOME}\\.rustup\\toolchains\\stable-x86_64-pc-windows-msvc`, default: true, command: "rustup toolchain uninstall stable-x86_64-pc-windows-msvc", guide: null, ...tot(934 * MB, 2100, 1) },
  { tool: "rustup", family: "Rust (rustup)", name: "nightly-2025-01-10-x86_64-pc-windows-msvc", path: `${HOME}\\.rustup\\toolchains\\nightly-2025-01-10-x86_64-pc-windows-msvc`, default: false, command: "rustup toolchain uninstall nightly-2025-01-10-x86_64-pc-windows-msvc", guide: null, ...tot(1.1 * GB, 2300, 280) },
  { tool: "android-sdk", family: "Android SDK", name: "system-images;android-35;google_apis;x86_64", path: `${HOME}\\AppData\\Local\\Android\\Sdk\\system-images\\android-35\\google_apis\\x86_64`, default: false, command: 'sdkmanager --uninstall "system-images;android-35;google_apis;x86_64"', guide: null, ...tot(3.5 * GB, 40, 30) },
  { tool: "unity", family: "Unity Editors", name: "2022.3.21f1", path: "C:\\Program Files\\Unity\\Hub\\Editor\\2022.3.21f1", default: false, command: null, guide: "unity-hub", ...tot(7.9 * GB, 91000, 200) },
  { tool: "dotnet-sdk", family: ".NET SDKs", name: "8.0.406", path: "C:\\Program Files\\dotnet\\sdk\\8.0.406", default: false, command: "dotnet-core-uninstall remove --sdk 8.0.406", guide: "windows-apps", ...tot(384 * MB, 5000, 400) },
];

let repoScan: RepoScan | null = null;
function repos(root: string): RepoScan {
  const art = (repo: string, rel: string, kind: string, level: "safe" | "caution", bytes: number, extra = {}) => ({
    path: `${repo}\\${rel.replace(/\//g, "\\")}`,
    rel,
    kind,
    level,
    mtime: now() - 40 * DAY,
    selected: level === "safe",
    blocked: null as string | null,
    ...tot(bytes, Math.round(bytes / 20_000), 40),
    ...extra,
  });
  const a = `${root}\\game-server`;
  const b = `${root}\\web-shop`;
  const c = `${root}\\unity-demo`;
  repoScan ??= {
    root,
    git: true,
    folders: 812,
    cancelled: false,
    repos: [
      { path: a, name: "game-server", git: true, kinds: ["rust"], gitBytes: 48 * MB, lastCommit: now() - 120 * DAY, lastCommitSubject: "commit: release 0.9", lastActivity: now() - 120 * DAY, dirty: false, recent: false, stale: true, artifacts: [art(a, "target", "rust", "safe", 6.1 * GB)], artifactBytes: 6.1 * GB },
      { path: b, name: "web-shop", git: true, kinds: ["node"], gitBytes: 12 * MB, lastCommit: now() - DAY, lastCommitSubject: "commit: checkout page", lastActivity: now() - 3600, dirty: true, recent: true, stale: false, artifacts: [art(b, "node_modules", "node", "safe", 840 * MB, { selected: false }), art(b, "dist", "node", "safe", 3 * MB, { selected: false }), art(b, "build", "node", "safe", 1 * MB, { selected: false, blocked: "not-ignored" })], artifactBytes: 843 * MB },
      { path: c, name: "unity-demo", git: false, kinds: ["unity"], gitBytes: 0, lastCommit: null, lastCommitSubject: null, lastActivity: now() - 60 * DAY, dirty: null, recent: false, stale: false, artifacts: [art(c, "Library", "unity", "caution", 4.2 * GB), art(c, "Temp", "unity", "safe", 60 * MB)], artifactBytes: 4.26 * GB },
    ],
  };
  return repoScan;
}

const STORES: StoreInfo[] = [
  { id: "npm-cache-clean", tool: "npm", program: "C:\\Program Files\\nodejs\\npm.cmd", args: ["cache", "clean", "--force"], commandLine: '"C:\\Program Files\\nodejs\\npm.cmd" cache clean --force', stores: [`${HOME}\\AppData\\Local\\npm-cache`], undo: "redownload", level: "safe", runnable: true, ...tot(2.2 * GB, 9000, 2) },
  { id: "pnpm-store-prune", tool: "pnpm", program: `${HOME}\\AppData\\Roaming\\npm\\pnpm.cmd`, args: ["store", "prune"], commandLine: `${HOME}\\AppData\\Roaming\\npm\\pnpm.cmd store prune`, stores: [`${HOME}\\AppData\\Local\\pnpm\\store`], undo: "redownload", level: "safe", runnable: true, ...tot(70 * MB, 7900, 9) },
  { id: "docker-system-prune", tool: "Docker", program: "C:\\Program Files\\Docker\\Docker\\resources\\bin\\docker.exe", args: ["system", "prune"], commandLine: '"C:\\Program Files\\Docker\\Docker\\resources\\bin\\docker.exe" system prune', stores: [`${HOME}\\AppData\\Local\\Docker\\wsl`], undo: "none", level: "advanced", runnable: false, ...tot(34.7 * GB, 3, 1) },
];

const UNKNOWN: UnknownFolder[] = [
  { path: `${HOME}\\AppData\\Roaming\\Antigravity`, name: "Antigravity", root: "appData", template: "${APPDATA}/Antigravity", level: "caution", signals: ["Cache", "Code Cache", "logs"], proposeUrl: "https://github.com/congluuit/NANOFreeDisk/issues/new?template=rule_proposal.yml&title=%5Brule%5D%20Antigravity", ...tot(1.3 * GB, 5320, 2) },
  { path: `${HOME}\\.zcode`, name: ".zcode", root: "home", template: "${HOME}/.zcode", level: "caution", signals: ["dot-folder", "v2/cache"], proposeUrl: "https://github.com/congluuit/NANOFreeDisk/issues/new?template=rule_proposal.yml&title=%5Brule%5D%20.zcode", ...tot(902 * MB, 7382, 1) },
];

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const devHandlers: Record<string, (a: Record<string, any>) => unknown> = {
  devcache_scan: () => report(),
  devcache_items: ({ ruleId, limit }) => {
    const r = rules.find((x) => x.ruleId === ruleId);
    if (!r) return [];
    return Array.from({ length: Math.min(limit, r.candidates) }, (_, i) => ({
      path: `${HOME}\\AppData\\Roaming\\${r.app.replace(/\W+/g, "")}\\Cache\\item-${i}`,
      isDir: i % 3 === 0,
      bytes: Math.round(r.bytes / (i + 2)),
      files: 1 + (i % 7),
      newest: now() - DAY * (i + 2),
    }));
  },
  devcache_run: ({ ruleIds }) => {
    const picked = rules.filter((r) => ruleIds.includes(r.ruleId) && r.level !== "protected");
    const freed = picked.reduce((s, r) => s + r.bytes, 0);
    for (const r of picked) {
      r.bytes = 0;
      r.candidates = 0;
    }
    return opResult(freed, picked.length);
  },
  devcache_cancel: () => undefined,
  devcache_unknown: () => UNKNOWN,
  devcache_toolchains: () => TOOLS,
  devcache_repos: ({ root }) => repos(root),
  devcache_repos_clean: ({ paths }) => {
    let freed = 0;
    for (const r of repoScan?.repos ?? []) {
      r.artifacts = r.artifacts.filter((a) => {
        if (!paths.includes(a.path)) return true;
        freed += a.bytes;
        return false;
      });
      r.artifactBytes = r.artifacts.filter((a) => !a.blocked).reduce((s, a) => s + a.bytes, 0);
    }
    return opResult(freed, paths.length);
  },
  devcache_stores: () => STORES,
  devcache_store_run: ({ id, commandLine }) => {
    const s = STORES.find((x) => x.id === id);
    if (!s || !s.runnable || s.commandLine !== commandLine) throw { code: "command", message: "not runnable" };
    const before = s.bytes;
    s.bytes = Math.round(before * 0.05);
    return { id, opId: `mock-cmd-${now()}`, before, after: s.bytes, freed: before - s.bytes, output: { code: 0, stdout: "npm warn using --force Recommended protections disabled.", stderr: "", timedOut: false, cancelled: false, millis: 1800 }, journalError: null };
  },
  devcache_reveal: () => undefined,
  devcache_propose: () => undefined,
  devcache_notify: () => undefined,
};
