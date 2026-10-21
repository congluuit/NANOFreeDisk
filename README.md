<p align="center">
  <img src="docs/assets/banner.svg" alt="NANOFreeDisk" width="100%">
</p>

<p align="center">
  <a href="https://github.com/congluuit/NANOFreeDisk/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/congluuit/NANOFreeDisk/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/congluuit/NANOFreeDisk/actions/workflows/release.yml"><img alt="Release" src="https://github.com/congluuit/NANOFreeDisk/actions/workflows/release.yml/badge.svg"></a>
  <a href="https://github.com/congluuit/NANOFreeDisk/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/congluuit/NANOFreeDisk?display_name=tag&sort=semver"></a>
  <a href="https://github.com/congluuit/NANOFreeDisk/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/congluuit/NANOFreeDisk/total"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="Rust" src="https://img.shields.io/badge/rust-stable%20%7C%20edition%202024-orange?logo=rust">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%2010%2F11%20%7C%20macOS%2011%2B-0078D4?logo=windows">
</p>

<p align="center">
  <b>NANOFreeDisk</b> is a fast, open-source disk analyzer and safe cleaner for Windows, written in Rust.<br>
  It shows where your space went, finds junk and duplicates, explains exactly what every AI/IDE cache folder is and what happens if you delete it,
  watches live disk activity per process, and keeps an eye on drive health — <b>without ever touching files Windows or your tools depend on</b>.<br>
  <sub>v1.0 is a disk analyzer; the roadmap grows it into a complete, safe <b>Windows workspace manager</b> for developers and gamers.</sub>
</p>

<p align="center">
  <a href="#-download">Download</a> ·
  <a href="#-features">Features</a> ·
  <a href="#-safety-first">Safety</a> ·
  <a href="#-devcache-manager">DevCache</a> ·
  <a href="#-cli">CLI</a> ·
  <a href="#-build-from-source">Build</a> ·
  <a href="#-roadmap">Roadmap</a> ·
  <a href="#-contributing">Contributing</a> ·
  <a href="#-support-the-author">☕ Donate</a> ·
  <a href="docs/PLAN.md">Development plan (vi)</a>
</p>

> **Status:** 🧪 preview (v0.1.x) — everything below works on Windows 10/11: scan & **suggestions** (what can go, what must stay, and why), safe cleaning with quarantine and undo, DevCache + AI leftovers, duplicates, an uninstaller with leftover cleanup and software junk, disk activity monitor, drive health, hardware sensors, EN/VI, themes, four installer packages. Still to come before 1.0: the NTFS MFT fast scan, code signing, profiles & onboarding. Star/watch the repo to follow releases — and [☕ buy the author a coffee](#-support-the-author) if it helps you.

---

## ✨ Features

| | Pillar | What you get |
|---|---|---|
| 📊 | **Analyze** | Right after a scan: **what you can free and what must stay** — every folder gets a verdict (Safe / Needs care / Guide only / Keep / Unknown), the bytes inside it by verdict, "if you delete this…", what is not touched and a one-click right way (rule → quarantine, `git worktree remove`, the package manager's own command…). Treemap, sunburst and sortable tree of any drive or folder. NTFS fast path reads the MFT (WizTree-class speed, ~1M files in seconds). Understands hardlinks, junctions, sparse/compressed files and OneDrive cloud placeholders, so numbers add up. |
| 🧹 | **Clean** | Rule-based junk detection: Windows temp & logs, crash dumps, thumbnail caches, browser caches, GPU shader caches, build artifacts (`target/`, `node_modules/`, `Library/`, `DerivedDataCache/`…). Every item carries a **risk level** and a plain-language **"if you delete this…"** note. |
| 🤖 | **DevCache** | A curated catalog of the temp/cache/session folders left behind by **Claude Code, Claude Desktop, OpenAI Codex, ChatGPT, Gemini CLI, GitHub Copilot, Cursor, Windsurf, Cline/Roo, Continue, Zed, Kiro, Trae, Ollama, LM Studio, Hugging Face, VS Code, JetBrains, Visual Studio, npm/pnpm/pip/uv/cargo/go/NuGet/Gradle, Unity, Unreal, Godot**… Shows what is safe, what costs you history or a re-download, and what is deliberately kept (configs, credentials, memory). |
| ⧉ | **Duplicates** | Size → partial BLAKE3 → full BLAKE3 pipeline with a hash cache, gentle on the PC (background I/O priority, one file at a time on hard disks), never downloads cloud placeholders. Identical files and identical folders; keep the oldest / newest / a preferred folder, send the rest to the Recycle Bin, or replace them with NTFS hard links / ReFS block clones — every copy re-checked byte for byte right before it moves, and everything restorable. |
| 🧩 | **Programs** | Every installed program (desktop, MSI and Store) with its size and install date; **uninstall with the vendor's own uninstaller** (one or many, one after the other), then **remove what it left behind** — its install folder, data folders in AppData/ProgramData, dead shortcuts, its registry keys and Run entries — each with the evidence that ties it to the program, never a folder another program uses. **Software junk**: app caches (Electron/Chromium), crash dumps, logs and old update packages of the programs you keep. Files go to the quarantine, registry keys to a `.reg` backup, everything restorable ([`docs/UNINSTALLER.md`](docs/UNINSTALLER.md)). |
| 📈 | **Monitor** | Real-time disk activity: which process reads and writes, how fast, IOPS, each disk's throughput, queue length, latency and busy time, a 60 s / 10 min chart and a 24-hour history. Without admin rights it uses each process's I/O counters; with one UAC prompt a private kernel trace session (ETW) adds the files being read and written and **"who touched this folder?"**. Spots runaway loggers, indexers and sync clients (optional alert) — and never stops anything by itself. |
| ♥ | **Health** | S.M.A.R.T. (ATA) and NVMe health log, temperature, wear, reallocated/pending sectors, TRIM status, dirty volumes, free-space trend with "disk full in N days" forecast, tray alerts, exportable report. |
| ⌨ | **CLI** | Everything above as `nfd` for scripts and CI: `nfd scan`, `nfd advise`, `nfd clean --dry-run`, `nfd devcache`, `nfd ai-leftovers`, `nfd dupes`, `nfd programs`, `nfd monitor`, `nfd health`. |

### Also in v1.0 — the "easy to use" layer

| | Feature | Why |
|---|---|---|
| 💯 | **Health Score home page** (0–100) with three big buttons: *Free X GB safely now*, *What is eating my disk*, *Is my drive healthy* — every point lost has a **Fix** button | One number, three clicks, no manual |
| ✋ | **You have the last word** — the app's "better keep" is a judgment and can be wrong, so **Delete anyway…** overrules it (to the quarantine, restorable for 7 days; a clean git worktree via `git worktree remove`). Only hard protections — Windows, programs, credentials, AI conversation history — can never be deleted, and the app says which one applies | Safe defaults without taking the decision away from you |
| 💡 | **Suggestions instead of hunting** — Analyze lists what can go across the whole scan ("80 GB AI agent worktrees in `NANOFreeDisk\.claude`, 35 GB Rust build output…") and the big things to keep with why (Windows, conversation history, your photos), so you never have to search Junk files or Dev caches yourself ([`docs/ANALYZE_ADVICE.md`](docs/ANALYZE_ADVICE.md)) | "Can I delete this, and what happens if I do?" answered for every folder |
| 🔀 | **"What changed?"** — diff two scans (today vs. last week): which folders grew, which files are new, how many GB | Answers "why did C: fill up by itself?" |
| 📖 | **Folder Encyclopedia** — click any system/app folder (WinSxS, `AppData\Local\Packages`, `Package Cache`, `.ollama`…) and get *What is this? Can I delete it? What happens if I do?* Community-editable | Nobody has to Google again |
| 🗂 | **Repo Finder** — every git repo on the drive with `.git` vs build-artifact size (`target/`, `node_modules/`, `Library/`, `DerivedDataCache/`, `bin/obj`, `.venv`…), last commit, uncommitted changes; clean artifacts to quarantine, `git gc`, archive | npkill + cargo-sweep + Unity cleaner for every language, one screen |
| 🧠 | **AI leftovers** — what AI coding agents leave behind *per session*: Claude Code worktrees (often 10+ GB of build output each), `%TEMP%\claude` scratchpads, `.playwright-mcp` snapshots, agent screenshots, test output, old Playwright browsers — each tied to the session that made it ("from session *X*, active 12 days ago"). Sessions still open or active this week are never pre-selected; worktrees are removed by `git worktree remove` only when clean and merged or pushed, branches stay; conversations are always kept ([`docs/AI_LEFTOVERS.md`](docs/AI_LEFTOVERS.md)) | In the AI era the clutter is per session and inside your projects — and nobody knows which part is safe |
| 🌐 | **Clean one website's browser cache** — Junk files → Web browser caches → *Advanced: clean one website*: every site in each Chrome / Edge / Brave / Vivaldi / Opera / Firefox profile with its size, searchable; only the ticked sites' Code Cache, Service Worker caches and Firefox cache entries go to the quarantine. Cookies, logins, site storage, history, passwords and extensions are never touched; Chromium's blockfile HTTP cache is listed, never edited ([`docs/BROWSER_SITE_CACHE.md`](docs/BROWSER_SITE_CACHE.md)) | "Clear the cache of this one site" without signing out of everything |
| ☑ | **One selection bar in every list** — "N selected · M files · X GB will be freed", Select all / Clear / only Safe / only Caution, a box per group, Ctrl+A / Esc; items an open app will keep are counted apart, and the confirmation repeats the totals per risk level | You always know exactly what one click will do |
| 🧰 | **Toolchain Inventory** — rustup toolchains, Node versions, Python/conda envs, JDKs, .NET SDKs, Android SDK images, Visual Studio workloads, Unity editors, Unreal versions — size, last used and the **proper uninstall command** | Dev machines carry 3–5 unused versions of everything |
| 📦 | **Package-store cleanup via vendor CLIs** — `pnpm store prune`, `npm cache verify`, `cargo-cache`, `uv/pip cache`, `nuget locals`, `go clean -modcache`, `conda clean`, `hf cache`, `ollama rm` | The store never breaks because the right command did the work |
| 👤 | **Profiles** — Developer / Gamer / Office / Minimal pick rule sets and thresholds during a 60-second onboarding | Usable before reading any docs |
| 🎚 | **Simple / Expert mode** — Simple hides Advanced rules and raw SMART; Expert shows everything plus the equivalent CLI command for each action | Grows with the user |
| ⌘ | **Command palette (Ctrl+K)**, keyboard-first, search across drives, folders, rules and settings | Fast for power users |
| ⏪ | **Time Machine** — an activity log of everything the app did, with one-click Restore (Recycle Bin, Quarantine, toggles) | Nothing is ever truly gone |
| 🌐 | **English & Vietnamese everywhere** — UI, CLI, installer, tray and every "if you delete this…" note; more languages via PR | Built in from day one, CI checks both locales stay complete |
| 🎨 | **Light / Dark / System / Custom theme** — Custom lets you pick your own colours (background, accent, risk colours, treemap palette) with live preview, presets and `theme.toml` import/export | Your tool, your look |
| 🔔 | Old & Large finder, empty folders & broken shortcuts, scheduled 🟢-only cleaning, Recycle Bin manager, folder quotas & alerts, free-space forecast, HTML report export, tray with per-drive free space, Explorer context menu, no telemetry | Small things that add up |

<p align="center">
  <img src="docs/assets/demo/nanofreedisk-demo.webp" alt="NANOFreeDisk demo: treemap, Ctrl+K palette, Folder Encyclopedia, Old &amp; large, What changed?, Vietnamese dark theme" width="90%"><br>
  <sub>The real app on a synthetic demo drive (recorded by <code>scripts/demo-video.mjs</code>; <a href="docs/assets/demo/nanofreedisk-demo.gif">GIF version</a>).</sub>
</p>

---

## 🛡 Safety first

NANOFreeDisk is built around one rule: **a cleaner must never be the reason Windows or your tools stop working.**

<p align="center">
  <img src="docs/assets/safety-pipeline.svg" alt="Safety pipeline" width="90%">
</p>

- **Hard-coded protected list that cannot be disabled** — `C:\Windows`, `Program Files`, `WinSxS`, `System32`, `pagefile.sys`, `hiberfil.sys`, `System Volume Information`, `$Recycle.Bin`, EFI/Recovery, your Documents/Desktop/Pictures, and the config/credential files of every tool in the catalog (`~/.claude.json`, `~/.codex/auth.json`, `~/.ssh`, `.env`, …).
- **Path canonicalization before every check** — junctions, symlinks, 8.3 names, `\\?\` prefixes and case tricks cannot route a delete into a protected folder.
- **Files in use are never force-deleted** — the Windows Restart Manager tells you *which* process holds a file; the app never kills anything.
- **Microsoft-signed and SYSTEM-attributed files** inside system folders are always protected, even if a rule matches.
- **Recycle Bin by default, Quarantine for big caches** — large cache folders are *moved* (instant rename) into a quarantine with a 7-day TTL and a one-click *Restore*. Permanent deletion is only offered for 🟢 Safe items and requires typing `DELETE`.
- **Dry-run first, always** — every plan shows the complete file list and total before anything happens; plans over 50 GB or 20 % of the drive get a red warning.
- **Undo journal + audit log** — every operation is recorded; `nfd quarantine restore <id>` brings it back.
- **No telemetry.** The only network calls are the optional update check and documentation links.

Risk levels shown on every item:

| Level | Meaning | Examples |
|---|---|---|
| 🟢 **Safe** | Recreated automatically; no user data or history lost | `%TEMP%`, GPU shader caches, Electron `Cache/` dirs, thumbnails, old logs |
| 🟡 **Caution** | Loses history, needs re-download, or slows the next start | Claude/Codex session transcripts, Ollama models, Unity `Library/`, Unreal `DerivedDataCache/` |
| 🟠 **Advanced** | Can break repair/uninstall or features; hidden unless Expert mode | `ProgramData\Package Cache`, `SoftwareDistribution\Download`, Docker `ext4.vhdx` |
| 🔴 **Protected / Keep** | Never offered for deletion; the app only *explains* the proper system tool | WinSxS → DISM, hiberfil → `powercfg /h off`, restore points → System Protection, configs & credentials |

---

## 🤖 DevCache Manager

Developer machines fill up with caches nobody dares to delete. NANOFreeDisk ships a maintained catalog (see [`docs/DEVTOOL_CACHE_CATALOG.md`](docs/DEVTOOL_CACHE_CATALOG.md) and the TOML rule packs in [`rules/`](rules/)) that answers three questions for each folder: **Can I delete it? What do I lose? Which app must be closed?**

<p align="center">
  <img src="docs/assets/screenshot-devcache.svg" alt="DevCache view (mockup)" width="90%">
</p>

A few examples from the catalog:

| Folder | Level | If you delete it… |
|---|---|---|
| `~\.claude\projects\`, `~\.codex\sessions\`, `~\.gemini\tmp\`, `~\.gemini\antigravity\`, VS Code / Cursor / Windsurf / Devin `workspaceStorage`, … | 🔴 | Kept — the conversation history of AI agents is **never** deleted, by any feature (rules, DevCache, Duplicates or by hand). |
| `~\.claude.json`, `settings.json`, `~\.codex\auth.json` | 🔴 | Kept — login, permissions and configuration. |
| `~\.claude\file-history\` | 🟡 | You can no longer revert old Claude Code edits. Your code is untouched. |
| `%APPDATA%\Cursor\Cache`, `CachedData`, `logs` | 🟢 | Rebuilt on next start. `User\globalStorage\state.vscdb` (chat history) is kept. |
| `~\.ollama\models\blobs` | 🟡 | Re-download required — the app lets you pick individual models instead. |
| `%LOCALAPPDATA%\JetBrains\<IDE>\caches`, `index` | 🟢 | IDE re-indexes the project once. |
| `<project>\Library\` (Unity) | 🟡 | Full asset re-import on next open (can take hours). Assets are not lost. |
| `<project>\DerivedDataCache\` (Unreal) | 🟡 | Shaders and derived data rebuild. |
| `%ProgramData%\Package Cache` | 🟠 | VC++/.NET/VS repair & uninstall may need the original installer again. |

Unknown dot-folders that *look* like caches are shown as 🟡 **Unverified** with a one-click "propose a rule" link — they are never part of one-click cleaning. Adding a vendor is a TOML entry plus a test; see [CONTRIBUTING.md](CONTRIBUTING.md).

---

## ⬇ Download

<p align="center">
  <img src="docs/assets/release-packages.svg" alt="Release packages" width="90%">
</p>

Grab the latest build from the [Releases page](https://github.com/congluuit/NANOFreeDisk/releases/latest):

| Package | File | Notes |
|---|---|---|
| ★ **Branded setup** (recommended) | `NANOFreeDisk-Setup-<ver>-x64.exe` | NANO-branded installer written in Rust (English/Vietnamese). For me (no admin, `%LOCALAPPDATA%\Programs\NANOFreeDisk`) or for everyone (UAC). Silent: `/S [/ALLUSERS] [/DESKTOP] [/SHELLMENU] [/D=C:\Apps\NANOFreeDisk]` (`/D=` last). Uninstall: Settings → Apps, or `uninstall.exe --uninstall /S [/REMOVEDATA]` |
| MSI | `NANOFreeDisk-<ver>-x64.msi` (`-vi-VN.msi` in Vietnamese) | Windows Installer for IT/GPO/Intune, installs for everyone (admin). Silent: `msiexec /i NANOFreeDisk-<ver>-x64.msi /qn`, `msiexec /x … /qn`. A quiet uninstall keeps user data. |
| Classic EXE | `NANOFreeDisk-<ver>-x64-setup.exe` | NSIS wizard, installs for the current user (`%LOCALAPPDATA%\NANOFreeDisk`, no admin). Silent: `/S`; uninstall `uninstall.exe /S [/REMOVEDATA]` |
| Portable | `NANOFreeDisk-<ver>-x64-portable.zip` | Unzip and run. Settings, history, quarantine and the WebView2 profile stay in `data\` next to the exe (`portable.flag`); nothing is written to your profile or the registry. Includes `nfd.exe`. |

Every package ships the app and the `nfd.exe` CLI. **Uninstalling never deletes files silently:** if NANOFreeDisk's quarantine still holds cleaned files you are asked whether to put them back or delete them; your data (scan history, settings, logs) is removed only when you tick "remove my data" (or pass `/REMOVEDATA`). The Explorer menu, shortcuts and registry entries always go. The optional PawnIO driver (Advanced sensors) is shared with other apps and stays installed.

**Updates:** Settings → Updates checks GitHub on request. The MSI and classic EXE update in place (the update is verified with the project's signing key); the branded setup and portable copies link to the new release.

Also (after 1.0): `winget install NANODev.NANOFreeDisk` · `scoop install nanofreedisk` · `cargo install nfd-cli`

**Verify a download:** the packages are not code-signed yet (SignPath Foundation is pending), so Windows SmartScreen may say "unknown publisher". Every release includes `SHA256SUMS.txt` (and a minisign signature `SHA256SUMS.txt.minisig` when available) — compare the hash before running an installer:

```powershell
Get-FileHash .\NANOFreeDisk-Setup-1.0.0-x64.exe -Algorithm SHA256
```

Requirements: Windows 10 1809+ / Windows 11, x64. The GUI uses the WebView2 runtime (preinstalled on Windows 11; the installer fetches it on Windows 10 if missing). Admin rights are only requested for the optional MFT fast scan, ETW monitoring, SMART over IOCTL and system-junk cleanup.

### 🍎 macOS (preview)

One app for **Intel and Apple silicon** Macs, **macOS 11 Big Sur or later** (keep Safari up to date: the app's window uses the system's WebKit, Safari 16.2+). Not yet tested by its author on a real Mac — please report what breaks.

- **Download:** `NANOFreeDisk-<ver>-macos-universal.dmg` from the [Releases page](https://github.com/congluuit/NANOFreeDisk/releases/latest) (from the next release on). Until then, the latest build is the `NANOFreeDisk-macos-universal` artifact of a manual [CI run](https://github.com/congluuit/NANOFreeDisk/actions/workflows/ci.yml) (open a run started with "Run workflow" → Artifacts; you need to be signed in to GitHub). Open the `.dmg` and drag NANOFreeDisk to Applications.
- **First launch:** the app is ad-hoc signed but **not notarized** by Apple yet, so Gatekeeper blocks it the first time. On macOS 11–14, right-click (or Control-click) the app → **Open** → **Open**. On macOS 15 and later, try to open it once, then go to System Settings → Privacy & Security and click **Open Anyway**. Or, in Terminal:

  ```bash
  xattr -dr com.apple.quarantine /Applications/NANOFreeDisk.app
  ```

- **Full Disk Access** (recommended): macOS hides Mail, Messages, Safari and other apps' data from every app, so without it a scan of your home folder or the disk leaves those folders out (the app shows a hint). System Settings → Privacy & Security → **Full Disk Access** → turn on NANOFreeDisk (or add it with **+**), then quit and reopen the app. The first scans also ask for Desktop, Documents and Downloads.
- **What works on macOS:** Analyze, Duplicates, Old & Large, Tidy up, DevCache, Activity (undo) and Settings. Deleting moves items to the **Trash** (Finder's Put Back works, and so does Activity → Restore) or to the app's quarantine — never a hard delete. Not on macOS yet: Clean (system rule packs), Programs, Drive health, Hardware, Monitor, the Finder menu and in-app updates.
- **Safety on macOS:** the system folders (`/System`, `/Library`, `/Applications`, `/usr`, `/private`…), iCloud Drive, Mail, Messages, keychains, app sandboxes, iPhone backups, Time Machine, Photos / Music / TV libraries and AI agents' chat history are never deleted, and nothing *inside* an app bundle or a document package is ever touched. Duplicates knows that **APFS clones** (Finder's Duplicate, `cp -c`) share their data — deleting one frees nothing, so they are flagged and never pre-selected — and never reads iCloud files that are not downloaded (reading would download them). Details: [docs/MACOS.md](docs/MACOS.md) (Vietnamese).

---

## ⌨ CLI

```text
nfd scan <path> [--fast] [--top N] [--json|--csv] [--min-size 10MB] [--advice]
nfd advise <path> [--json] [--lang vi] [--no-git]   # what can go, what must stay, and why
nfd clean [--rules ai-tools,browsers] [--level safe] [--dry-run] [--mode recycle|quarantine]
nfd devcache [list|clean --vendor anthropic --level safe|unknown]
nfd toolchains [--json]
nfd repos <root> [--keep-recent 7] [--json]   ·   nfd repos clean <root> [--apply]
nfd stores [--json]   ·   nfd stores run npm-cache-clean
nfd ai-leftovers [list|clean] [--root <dir>] [--keep-recent 7] [--apply] [--json]
nfd browsers sites [--browser chrome] [--search tube] [--json]   ·   nfd browsers clean-site <host> [--exact] [--apply]
nfd dupes <path...> [--min-size 1MB] [--folders] [--keep auto|oldest|newest|prefer:<dir>] [--action list|recycle|quarantine|hardlink] [--apply]
nfd programs [list|junk|uninstall <name>|leftovers <name>] [--remove-leftovers|--apply] [--json]
nfd monitor [--seconds 10] [--top 10] [--json] [--source auto|etw|counters] [--watch <dir>]   ·   nfd monitor --history
nfd health [--json] [--raw] [--disk N]          # exit code 1 = caution, 2 = bad
nfd quarantine [list|restore <id>|purge --older 7d]
nfd rules [list|validate <dir>]
```

```powershell
# what would a safe clean of AI-tool caches free up? (nothing is deleted)
nfd clean --rules ai-tools --level safe --dry-run
```

---

## 🏗 Architecture

<p align="center">
  <img src="docs/assets/architecture.svg" alt="Architecture" width="90%">
</p>

- **`nfd-core`** — entry arena, parallel scanner (+ NTFS MFT fast path), the `Planner → SafetyGate → Executor` pipeline, quarantine, insights (Old & Large, empty folders, shortcuts), Health Score. UI-agnostic; 100 % test-coverage target for `safety`.
- **`nfd-advice`** — the verdict of every path of a scan (safe / needs care / guide only / keep / unknown), bytes by verdict per folder, suggestions and the right action, combining everything below.
- **`nfd-rules`** — TOML rule-pack loader, glob matcher, risk levels, Folder Encyclopedia.
- **`nfd-devtools`** — DevCache grouping, unknown caches, toolchains, Repo Finder + build-artifact policy, package-store commands, AI leftovers (sessions, git worktrees).
- **`nfd-apps`** — installed programs, the vendors' uninstallers, leftovers after uninstalling (folders, shortcuts, registry with `.reg` backups, one elevated step for all-users items) and software junk.
- **`nfd-dupes`** · **`nfd-browsers`** · **`nfd-history`** — duplicate finder (BLAKE3, hash cache), per-website browser caches, scan history + undo journal (SQLite).
- **`nfd-monitor`** — ETW consumers, per-process I/O counters, PDH disk counters.
- **`nfd-health`** · **`nfd-hwinfo`** · **`nfd-sensors`** · **`nfd-tasks`** · **`nfd-helper`** — SMART/NVMe via IOCTL (NVMe without admin rights), hardware inventory, sensors, processes, and the small elevated helper (one UAC prompt).
- **`nfd-platform`** — Windows APIs on `windows-sys` (file info, Recycle Bin, Restart Manager, volumes, shell menu); on macOS the Trash, `getfsstat`, `st_flags`, APFS clones and Full Disk Access.
- **`apps/nanofreedisk`** — Tauri 2 desktop app (Svelte UI). **`apps/nfd-cli`** — the CLI. **`apps/nanofreedisk-setup`** — the branded installer (egui).

Full design, module specs, risk analysis and the phase-by-phase roadmap live in [`docs/PLAN.md`](docs/PLAN.md) (Vietnamese).

---

## 🔨 Build from source

```powershell
# prerequisites: Rust stable (1.85+), Git; for the desktop app: Node 20+, pnpm 9, WebView2
git clone https://github.com/congluuit/NANOFreeDisk.git
cd NANOFreeDisk
cargo build --release -p nfd-cli
.\target\release\nfd.exe --version
.\target\release\nfd.exe rules validate .\rules
```

Desktop app (Tauri 2 + Svelte 5):

```powershell
cd apps\nanofreedisk
pnpm install
pnpm tauri dev                     # run with hot reload
pnpm tauri build --no-bundle       # release exe: target\release\nanofreedisk.exe
pnpm dev                           # UI only, in a browser, against a built-in mock drive
```

Useful dev commands:

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace      # or: cargo test --workspace
cargo deny check
pnpm --dir apps/nanofreedisk check   # svelte-check (warnings fail)
pnpm --dir apps/nanofreedisk test    # UI unit tests (selection, advice, treemap, theme…)
node scripts/check-locales.mjs       # EN and VI must have the same keys
node scripts/gui-smoke.mjs           # end-to-end test of the built app on temporary fixtures
```

NANOFreeDisk targets Windows and macOS: CI lints and tests on Windows and macOS runners (formatting, licenses and the UI checks run on Linux runners because they do not depend on the OS). On a Mac (Xcode Command Line Tools, Rust targets `aarch64-apple-darwin` + `x86_64-apple-darwin`), `scripts/build-macos.sh dist <version>` builds the universal `.dmg`; a manual run of the CI workflow does the same and keeps it as an artifact.

---

## 🗺 Roadmap

### v1.0 — Windows (~21 weeks, solo developer)

| Phase | Scope | Status |
|---|---|---|
| 0 | Foundation: workspace, CI/CD, docs, DevCache catalog v0 | ✅ |
| 1 | Core scanner (standard + MFT), `nfd scan` | 🟡 standard mode done, MFT next |
| 2 | GUI: Health Score home, treemap/sunburst/list, instant search, "What changed?", Old & Large, Folder Encyclopedia, HTML report, Simple/Expert, Ctrl+K | ✅ done — plus age heatmap, empty folders / broken shortcuts / orphan junctions, Explorer menu, `nfd diff` / `nfd explain` ([demo](docs/assets/demo/nanofreedisk-demo.webp)) |
| 3 | SafetyGate, Planner/Executor, Quarantine, Time Machine undo log, scheduled 🟢 clean, Recycle Bin manager, junk & GPU rule packs | ✅ done — Clean view, Activity log with Restore, quarantine with 7-day purge, `nfd clean` / `nfd quarantine`; scheduled clean and the Recycle Bin manager remain for v1.x |
| 4 | DevCache Manager UI, Repo Finder, Toolchain Inventory, vendor-CLI package cleanup, build-artifact policy, full rule packs | ✅ done — 123 DevCache rules, unknown-folder detection, `nfd devcache` / `toolchains` / `repos` / `stores` |
| 5 | Duplicate finder | ✅ done — Duplicates view (files + folders), "Find duplicates here", replace with hard links / ReFS block clones, `nfd dupes` (size → partial → full BLAKE3, hash cache, keeper re-checked before every move) |
| 6 | Disk Activity monitor (ETW), log-spam detector | ✅ done — Disk activity view (live chart, disks, processes → files, files being written, 24 h history, "Who touched this?"), I/O counters without admin or a private ETW session through the elevated helper, log-spam alert, tray "Pause monitor", `nfd monitor` |
| 7 | Drive health (SMART / NVMe), free-space forecast, quotas & alerts, dump/log advisor | ✅ done — Health view + `nfd health` (NVMe health log without admin rights, ATA SMART / USB SAT through the elevated helper, CrystalDiskInfo-style verdict, volume checks, HTML/JSON report), Health Score, drive alerts and tray icon; quotas and the dump/log advisor remain for v1.x |
| 8 | Installers (branded, MSI, NSIS, portable), updater, signing, profiles & onboarding, tray | ✅ installers, updater and tray done — code signing (SignPath) and profiles & onboarding pending |
| — | **Suggestions in Analyze**: a verdict for every folder, "if you delete this…", one place that suggests what to clean | ✅ done — `nfd-advice`, `nfd advise` ([details](docs/ANALYZE_ADVICE.md)) |
| 9 | **v1.0** release for Windows | ⏳ preview builds (v0.1.x) on the Releases page |

### v1.x — one or two modules per minor release

- **NTFS Compact** (`compact.exe /EXE:LZX`) for games, SDKs and `node_modules`: 30–60 % back without deleting anything, measured before/after, reversible.
- **Privacy cleaner** — browser history/cookies/cache with a keep-list for signed-in sites, Recent items, Jump Lists, DNS cache, clipboard & activity history.
- **Docker & WSL** — `docker system df`, prune by type, orphaned volumes; WSL distro sizes and `ext4.vhdx` compaction; Hyper-V/VirtualBox/VMware disks and snapshots.
- **Game Library Manager** — Steam, Epic, GOG, Xbox, Battle.net, Riot, EA, Ubisoft: size, last played, shader cache per game, leftovers of uninstalled games, move games between libraries. Capture-folder cleanup (ShadowPlay, Game Bar, OBS).
- ~~Uninstaller + leftover scan~~ — ✅ done early: the **Programs** view. Still to come: winget-managed apps, "last used", a restore point before uninstalling.
- **Startup Manager** — Run keys, Startup folders, scheduled tasks, non-Microsoft auto-start services, UWP startup tasks, measured startup impact; enable/disable with undo, never delete.
- **App Mover & storage relocation** — move apps/games to another drive with junctions; relocate default save locations, libraries, temp, pagefile, hibernation, restore-point quota — each with a plain explanation.
- **Disk Benchmark** — sequential and random 4K like CrystalDiskMark, with a per-drive baseline over time to spot SSD degradation.
- **"Who touched this?"** — ETW watcher on a folder: which process created/wrote/deleted/renamed what, with a timeline and alerts.
- Also: cloud dehydrate (OneDrive/Google Drive), secure delete for HDDs, Downloads organizer, PATH & env doctor, IDE workspace hygiene, surface scan & `sfc`/`DISM` wizard, SSD/HDD optimize, backup status, lightweight resource overview, read-only/policy mode for IT, PowerShell module.

### v2.0

- **macOS & Linux** ports, media duplicates (perceptual hash), intentional hardlink dedupe for `node_modules`, context-menu manager, Store bloatware list, boot-time trend, signed community rule packs, compare two machines or snapshots.

### What we will never do

| Not doing | Why |
|---|---|
| Registry cleaner | Nearly useless on modern Windows, high risk, no reliable undo — the uninstaller only removes the keys of the program you just uninstalled, after a `.reg` backup |
| Driver updater | Untrusted driver sources cause blue screens; leave it to Windows Update and vendors |
| "RAM optimizer", "game booster", "internet booster" | Placebo features that would damage trust |
| Antivirus / malware removal | Out of scope; we only flag suspicious file-system behaviour and suggest a Defender scan |
| Killing processes or stopping services | The app shows who holds a file; the user decides |
| Deep Windows telemetry/privacy tweaks, removing Windows components | Dedicated tools exist; we only guide |
| Kernel driver / minifilter | Signing cost and stability risk; ETW is enough |

The full feature map with version tags, risk levels and rationale is in [`docs/PLAN.md`](docs/PLAN.md) §13.

---

## 🤝 Contributing

Issues and pull requests are welcome — especially **new or corrected cache rules** (one TOML entry + how you verified it). Please read [CONTRIBUTING.md](CONTRIBUTING.md), the [Code of Conduct](CODE_OF_CONDUCT.md) and [SECURITY.md](SECURITY.md) for reporting vulnerabilities.

Commits follow [Conventional Commits](https://www.conventionalcommits.org/); anything touching `nfd-core::safety` uses the `safety:` type and must come with tests.

---

## 👤 Author

**Lưu Đức Thành Công** — [nanodev.io.vn](https://nanodev.io.vn/) · [@congluuit](https://github.com/congluuit)

## ☕ Support the author

NANOFreeDisk is free, open source and made by one developer in Vietnam. If it saved you some gigabytes (or an evening of guessing which folder is safe to delete), a coffee keeps it going — thank you! The same QR code is in the app under **Settings → Support the author**.

*Ủng hộ tác giả: quét mã VietQR bằng bất kỳ ứng dụng ngân hàng nào.*

<p align="center">
  <img src="docs/assets/qr-bidv-donate.jpg" alt="VietQR BIDV — LUU DUC THANH CONG — 6503695458" width="280"><br>
  <b>LUU DUC THANH CONG</b> · BIDV (Ngân hàng TMCP Đầu tư và Phát triển Việt Nam) · <code>6503695458</code>
</p>

Starring the repo, reporting a bug or contributing a cache rule helps just as much. ⭐

---

## 📄 License

[MIT](LICENSE) © 2026 Lưu Đức Thành Công. Rule packs in `rules/` are data and are released under the same license.
