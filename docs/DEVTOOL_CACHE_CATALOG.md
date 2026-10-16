# DevCache Catalog — Thư mục tạm của AI / IDE / Agent / Package Manager / Game Engine

> Phiên bản v0 (2026-10-16). Đây là nguồn sự thật để sinh rule pack `rules/ai-tools.toml`, `rules/ide-editors.toml`, `rules/package-managers.toml`, `rules/game-engines.toml`.
> Mỗi dòng phải trả lời được 3 câu: **Xóa được không? Mất gì? Phải đóng app nào?**
> Đường dẫn dùng biến Windows; macOS/Linux ghi trong cột cuối khi khác biệt.

## Thang mức rủi ro

| Mức | Ý nghĩa | Hành vi app |
|-----|---------|-------------|
| 🟢 **Safe** | App tự tạo lại; không mất dữ liệu/cấu hình/lịch sử người dùng | Có trong "Dọn an toàn 1 chạm" |
| 🟡 **Caution** | Mất lịch sử/phiên làm việc, phải tải lại (model, package), chậm lần mở sau | Phải tick xác nhận từng mục; hiện dòng "Nếu xóa thì…" |
| 🟠 **Advanced** | Có thể làm hỏng cài đặt/chức năng nếu không hiểu | Ẩn mặc định, bật trong Settings → Expert |
| 🔴 **Protected / Keep** | Config, credential, token, dữ liệu người dùng | Không có nút xóa; hiện 🔒 để người dùng biết app đã cố tình bỏ qua |

Quy ước chung: mọi thư mục `Cache`, `Code Cache`, `GPUCache`, `DawnCache`, `DawnGraphiteCache`, `DawnWebGPUCache`, `ShaderCache`, `Service Worker\CacheStorage`, `blob_storage`, `Crashpad\completed`, `logs` của app Electron/Chromium đều 🟢 khi app đã đóng.

---

## 1. AI Assistants / Coding Agents

> **Lịch sử hội thoại không bao giờ bị xóa (PLAN §5.1).** Mọi thư mục lưu conversation / session của AI agent (Claude Code & Desktop, Codex, Gemini CLI, Antigravity, Copilot, Cursor, Windsurf, Devin, ZCode, Trae, Kiro, opencode, Continue, Cline, Roo Code, Aider, SpecStory, LM Studio, Jan…) nằm trong `AGENT_HISTORY_TEMPLATES` / `AGENT_HISTORY_NAMES` của SafetyGate (`crates/nfd-core/src/safety.rs`) — bị chặn với mọi nguồn xóa (rule, DevCache, Duplicates, chọn tay) — và là rule 🔴 Keep trong `rules/ai-tools.toml`.

### 1.1 Anthropic — Claude Code (CLI)

| Đường dẫn | Mức | Nội dung | Nếu xóa thì… | Cần đóng |
|-----------|-----|----------|--------------|----------|
| `%USERPROFILE%\.claude.json` | 🔴 | Config toàn cục, OAuth, trạng thái onboarding, MCP servers | Phải đăng nhập lại, mất cấu hình | — |
| `%USERPROFILE%\.claude\settings.json`, `settings.local.json` | 🔴 | Permissions, hooks, model | Mất cấu hình | — |
| `%USERPROFILE%\.claude\projects\<proj>\*.jsonl` | 🔴 | Transcript phiên làm việc (dùng cho `--resume`, `--continue`) | Lịch sử hội thoại — không bao giờ xóa (PLAN §5.1) | — |
| `%USERPROFILE%\.claude\projects\<proj>\memory\` | 🔴 | Auto-memory của Claude Code cho project | Mất "trí nhớ" tích lũy | — |
| `%USERPROFILE%\.claude\todos\` | 🟢 | Todo list tạm theo phiên | Không | `claude.exe` |
| `%USERPROFILE%\.claude\shell-snapshots\` | 🟢 | Snapshot shell env | Tự tạo lại | `claude.exe` |
| `%USERPROFILE%\.claude\statsig\`, `\telemetry\` | 🟢 | Feature-flag cache | Tự tạo lại | — |
| `%USERPROFILE%\.claude\debug\`, `\logs\` | 🟢 | Log debug | Mất log để báo lỗi | — |
| `%USERPROFILE%\.claude\file-history\` | 🟡 | Bản sao file trước khi sửa (undo) | Mất khả năng revert chỉnh sửa cũ | `claude.exe` |
| `%USERPROFILE%\.claude\plugins\`, `\skills\`, `\commands\`, `\agents\` | 🔴 | Plugin/skill người dùng cài | Mất plugin | — |
| `%USERPROFILE%\.claude\downloads\`, `\cache\` (nếu có) | 🟢 | Binary tải về / cache | Tải lại | `claude.exe` |
| `%USERPROFILE%\.local\share\claude\versions\` (native installer) | 🟠 | Các phiên bản binary cũ của Claude Code | Giữ phiên bản mới nhất, xóa cũ được; làm sai → phải cài lại | `claude.exe` |
| `%APPDATA%\npm\node_modules\@anthropic-ai\claude-code\` (cài qua npm) | 🔴 | Bản cài | Mất app | — |

### 1.2 Anthropic — Claude Desktop

| Đường dẫn | Mức | Nếu xóa thì… | Cần đóng |
|-----------|-----|--------------|----------|
| `%APPDATA%\Claude\claude_desktop_config.json` | 🔴 | Mất cấu hình MCP | — |
| `%APPDATA%\Claude\Cache\`, `Code Cache\`, `GPUCache\`, `DawnCache\`, `Service Worker\CacheStorage\` | 🟢 | Tải lại UI, mở chậm lần đầu | `Claude.exe` |
| `%APPDATA%\Claude\logs\`, `%APPDATA%\Claude\Crashpad\` | 🟢 | Mất log | `Claude.exe` |
| `%APPDATA%\Claude\Local Storage\`, `Session Storage\`, `IndexedDB\` | 🟡 | Đăng xuất, mất draft/cài đặt local | `Claude.exe` |
| `%LOCALAPPDATA%\AnthropicClaude\app-<ver>\` (các version cũ, Squirrel) | 🟠 | Chỉ xóa version cũ hơn version đang chạy | `Claude.exe` |
| `%LOCALAPPDATA%\AnthropicClaude\packages\*.nupkg` | 🟢 | Gói cập nhật đã áp dụng | `Claude.exe` |
| `%LOCALAPPDATA%\AnthropicClaude\SquirrelSetup.log`, `*.log` | 🟢 | Log | — |

### 1.3 OpenAI — Codex CLI / ChatGPT Desktop

| Đường dẫn | Mức | Nếu xóa thì… | Cần đóng |
|-----------|-----|--------------|----------|
| `%USERPROFILE%\.codex\auth.json` | 🔴 | Đăng nhập lại | — |
| `%USERPROFILE%\.codex\config.toml`, `instructions.md`, `AGENTS.md` | 🔴 | Mất cấu hình | — |
| `%USERPROFILE%\.codex\sessions\**\*.jsonl` | 🔴 | Lịch sử/resume phiên Codex — không bao giờ xóa (PLAN §5.1) | — |
| `%USERPROFILE%\.codex\log\`, `*.log` | 🟢 | Mất log | — |
| `%USERPROFILE%\.codex\history.jsonl`, `archived_sessions\`, `thread_history_*.sqlite` | 🔴 | Lịch sử prompt / thread — không bao giờ xóa | — |
| `%LOCALAPPDATA%\Packages\OpenAI.ChatGPT-Desktop_*\LocalCache\` | 🟢 | Cache app MSIX; tải lại | `ChatGPT.exe` |
| `%LOCALAPPDATA%\Packages\OpenAI.ChatGPT-Desktop_*\LocalState\` | 🟡 | Mất cài đặt/draft local | `ChatGPT.exe` |

### 1.4 Google — Gemini CLI / Antigravity

| Đường dẫn | Mức | Nếu xóa thì… | Cần đóng |
|-----------|-----|--------------|----------|
| `%USERPROFILE%\.gemini\oauth_creds.json`, `settings.json`, `GEMINI.md` | 🔴 | Đăng nhập/cấu hình lại | — |
| `%USERPROFILE%\.gemini\tmp\` | 🔴 | Chat đã lưu, checkpoint theo dự án — không bao giờ xóa | — |
| `%USERPROFILE%\.gemini\history\`, `*.jsonl` | 🔴 | Lịch sử phiên — không bao giờ xóa | — |
| `%USERPROFILE%\.gemini\installation_id` | 🔴 | — | — |

### 1.5 GitHub Copilot (CLI + extension)

| Đường dẫn | Mức | Nếu xóa thì… |
|-----------|-----|--------------|
| `%USERPROFILE%\.copilot\` (CLI: config, session-state) | 🟡 | `config.json` 🔴; `session-state\`, `logs\` 🟢/🟡 |
| `%APPDATA%\Code\User\globalStorage\github.copilot-chat\` | 🟡 | Mất lịch sử chat trong VS Code |
| `%LOCALAPPDATA%\github-copilot\` | 🔴 | Token/hosts → phải đăng nhập lại |

### 1.6 Cursor

| Đường dẫn | Mức | Nếu xóa thì… | Cần đóng |
|-----------|-----|--------------|----------|
| `%APPDATA%\Cursor\User\settings.json`, `keybindings.json`, `snippets\` | 🔴 | Mất cấu hình | — |
| `%APPDATA%\Cursor\User\globalStorage\state.vscdb` | 🔴 | Mất trạng thái, lịch sử chat Cursor (lưu trong DB này) | — |
| `%APPDATA%\Cursor\User\workspaceStorage\` | 🔴 | Chat theo dự án, trạng thái UI — không bao giờ xóa (PLAN §5.1) | — |
| `%APPDATA%\Cursor\User\History\` | 🟡 | Mất Local History (timeline) của file | `Cursor.exe` |
| `%APPDATA%\Cursor\Cache\`, `CachedData\`, `CachedExtensionVSIXs\`, `CachedProfilesData\`, `Code Cache\`, `GPUCache\`, `DawnCache\`, `Service Worker\CacheStorage\` | 🟢 | Tải lại, mở chậm lần đầu | `Cursor.exe` |
| `%APPDATA%\Cursor\logs\`, `Crashpad\` | 🟢 | Mất log | `Cursor.exe` |
| `%USERPROFILE%\.cursor\extensions\` | 🔴 | Mất extension đã cài | — |
| `%USERPROFILE%\.cursor\mcp.json`, `rules\` | 🔴 | Mất MCP/rules | — |
| `%LOCALAPPDATA%\cursor-updater\` | 🟢 | Gói update đã tải | `Cursor.exe` |
| `%LOCALAPPDATA%\Programs\cursor\` | 🔴 | App | — |

### 1.7 Windsurf (Codeium)

| Đường dẫn | Mức | Nếu xóa thì… |
|-----------|-----|--------------|
| `%APPDATA%\Windsurf\` | như VS Code (mục 2.1) | `User\` 🔴, `Cache*` 🟢, `logs` 🟢 |
| `%USERPROFILE%\.codeium\windsurf\` | 🟡 | `cascade\` lịch sử Cascade 🔴, `mcp_config.json` 🔴, `memories\` 🔴, `logs\` 🟢 |
| `%USERPROFILE%\.codeium\config.json`, `language_server_*` | 🔴/🟢 | config 🔴; binary language server cũ 🟢 (tải lại) |

### 1.8 Khác (Devin, Zed, Kiro, Trae, Cline/Roo, Continue, Aider, Amp, Zencoder, OpenCode)

| Công cụ | Đường dẫn | Mức | Ghi chú |
|---------|-----------|-----|---------|
| **Devin** (Cognition) | Chạy trên cloud; local chỉ có extension VS Code/JetBrains → `globalStorage\cognition.*` | 🟡 | Không có cache lớn trên máy |
| **Zed** | `%LOCALAPPDATA%\Zed\logs\` 🟢, `\db\` 🟡 (lịch sử), `\languages\`/`\node\` 🟢 (tải lại), `\extensions\` 🔴; `%APPDATA%\Zed\settings.json` 🔴 | — | Zed Windows ổn định từ 2025 |
| **Kiro** (AWS) | `%APPDATA%\Kiro\` (như VS Code), `%USERPROFILE%\.kiro\` (`settings\`, `specs\` 🔴; `logs\` 🟢) | — | — |
| **Trae** (ByteDance) | `%APPDATA%\Trae\` (như VS Code), `%USERPROFILE%\.trae\` | — | — |
| **Cline / Roo Code** | `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\tasks\` 🔴 (lịch sử task), `rooveterinaryinc.roo-cline\` tương tự | 🔴 | Có thể rất lớn (checkpoint + screenshot) |
| **Continue** | `%USERPROFILE%\.continue\index\` 🟢 (re-index), `sessions\` 🔴, `config.yaml`/`config.json` 🔴 | — | — |
| **Aider** | `<repo>\.aider.chat.history.md`, `.aider.input.history` 🔴, `.aider.tags.cache.v*\` 🟢 | — | Nằm trong repo → rule project-level |
| **Amp** (Sourcegraph) | `%LOCALAPPDATA%\amp\` / `%USERPROFILE%\.config\amp\` | 🟡 | Chưa xác minh → mặc định 🟡 |
| **Zencoder / OpenCode / Crush …** | `%USERPROFILE%\.<tool>\` | 🟡 | Chưa xác minh → app gắn nhãn "Chưa xác minh", mời đóng góp rule |

### 1.9 Local LLM runtimes

| Công cụ | Đường dẫn | Mức | Nếu xóa thì… |
|---------|-----------|-----|--------------|
| **Ollama** | `%USERPROFILE%\.ollama\models\blobs\` | 🟡 | Phải `ollama pull` lại (GB → TB); app hiện danh sách model để chọn từng cái |
| **Ollama** | `%USERPROFILE%\.ollama\id_ed25519*` | 🔴 | Key | 
| **Ollama** | `%LOCALAPPDATA%\Ollama\*.log` | 🟢 | Log |
| **LM Studio** | `%USERPROFILE%\.lmstudio\models\` (hoặc `%USERPROFILE%\.cache\lm-studio\models\`) | 🟡 | Tải lại model |
| **LM Studio** | `%USERPROFILE%\.lmstudio\conversations\` | 🔴 | Lịch sử chat người dùng |
| **Hugging Face** | `%USERPROFILE%\.cache\huggingface\hub\` | 🟡 | Tải lại model/dataset; `token` 🔴 |
| **PyTorch / TF / Keras** | `%USERPROFILE%\.cache\torch\`, `.keras\`, `%LOCALAPPDATA%\tensorflow_datasets\` | 🟡 | Tải lại |
| **Jan, GPT4All, Msty, AnythingLLM** | `%APPDATA%\Jan\models\`, `%LOCALAPPDATA%\nomic.ai\GPT4All\` … | 🟡 | Tải lại model; chat history 🔴 |

---

## 2. IDE / Editors

### 2.1 VS Code (và mọi fork: VSCodium, Cursor, Windsurf, Kiro, Trae, Antigravity)

Thay `Code` bằng tên fork tương ứng trong `%APPDATA%\<Code>\`.

| Thư mục | Mức | Nếu xóa thì… |
|---------|-----|--------------|
| `User\settings.json`, `keybindings.json`, `snippets\`, `profiles\`, `globalStorage\state.vscdb` | 🔴 | Mất cấu hình/trạng thái |
| `User\globalStorage\<ext>\` | 🔴 | Dữ liệu từng extension (chat history AI, token) — giữ lại (PLAN §5.1) |
| `User\workspaceStorage\<hash>\` | 🔴 | Trạng thái workspace, chat AI theo dự án — giữ lại (PLAN §5.1) |
| `User\History\` | 🟡 | Local History |
| `Cache\`, `CachedData\`, `CachedExtensions\`, `CachedExtensionVSIXs\`, `CachedProfilesData\`, `Code Cache\`, `GPUCache\`, `DawnCache\`, `DawnGraphiteCache\`, `DawnWebGPUCache\`, `Service Worker\CacheStorage\`, `blob_storage\`, `Crashpad\` | 🟢 | Tải lại |
| `logs\` | 🟢 | Mất log cũ |
| `Backups\` | 🟡 | Hot-exit backup file chưa lưu — chỉ xóa khi app đóng và thư mục cũ > 7 ngày |
| `%USERPROFILE%\.vscode\extensions\` | 🔴 | Extension; thư mục `*.obsolete` & phiên bản trùng cũ → 🟢 |
| `%USERPROFILE%\.vscode\cli\` | 🟢 | Tunnel/CLI cache (tải lại) |
| `%USERPROFILE%\.vscode-server\` (WSL/remote) | 🟡 | Server remote; `bin\<commit>` cũ → 🟢 |

### 2.2 JetBrains (IntelliJ, Rider, PyCharm, WebStorm, CLion, GoLand, Android Studio…)

| Thư mục | Mức | Nếu xóa thì… | Cần đóng |
|---------|-----|--------------|----------|
| `%APPDATA%\JetBrains\<Product><Ver>\` | 🔴 | Config, keymap, plugin settings | — |
| `%LOCALAPPDATA%\JetBrains\<Product><Ver>\caches\`, `index\` | 🟢 | Re-index project (vài phút với project lớn) | IDE |
| `%LOCALAPPDATA%\JetBrains\<Product><Ver>\log\`, `tmp\`, `jcef_cache\`, `compile-server\` | 🟢 | Mất log; build lại | IDE |
| `%LOCALAPPDATA%\JetBrains\<Product><Ver>\` với `<Ver>` **cũ hơn** bản đang cài | 🟢 | Rác sau upgrade | — |
| `%APPDATA%\JetBrains\<Product><Ver>\` với `<Ver>` cũ | 🟡 | Dùng để import settings khi cài lại | — |
| `%LOCALAPPDATA%\JetBrains\Toolbox\apps\<ide>\ch-0\<ver-cũ>\` | 🟠 | Phiên bản IDE cũ do Toolbox giữ — nên xóa qua Toolbox | Toolbox |
| `%LOCALAPPDATA%\JetBrains\Toolbox\logs\`, `cache\` | 🟢 | — | — |
| `%USERPROFILE%\.m2\repository\`, `.gradle\caches\` | → mục 3 | — | — |
| `%LOCALAPPDATA%\Google\AndroidStudio*\` | như trên | — | — |
| `%LOCALAPPDATA%\Android\Sdk\system-images\`, `\emulator\`, `.android\avd\*.avd\*.img` | 🟠 | Mất emulator image (tải lại GB); AVD snapshot 🟡 | Emulator |

### 2.3 Visual Studio

| Thư mục | Mức | Nếu xóa thì… |
|---------|-----|--------------|
| `%LOCALAPPDATA%\Microsoft\VisualStudio\<ver>\ComponentModelCache\`, `Extensions\` cũ, `ImageCache\` | 🟢 | Tạo lại |
| `%LOCALAPPDATA%\Microsoft\VisualStudio\Packages\` | 🟠 | Installer cache — dùng cho modify/repair; xóa phải tải lại khi sửa VS |
| `%LOCALAPPDATA%\Temp\VSFeedbackIntelliCodeLogs\`, `%TEMP%\VS*` | 🟢 | — |
| `<solution>\.vs\` | 🟢 | IntelliSense DB, UI state (mất layout cửa sổ) |
| `%LOCALAPPDATA%\Microsoft\VisualStudio\<ver>\Designer\ShadowCache\` | 🟢 | — |
| `%ProgramData%\Microsoft\VisualStudio\Packages\` | 🟠 | Như Packages ở trên (all-users) |

### 2.4 Khác

| Công cụ | Đường dẫn | Mức |
|---------|-----------|-----|
| Sublime Text | `%LOCALAPPDATA%\Sublime Text\Cache\`, `Index\` 🟢; `%APPDATA%\Sublime Text\` 🔴 |
| Notepad++ | `%APPDATA%\Notepad++\backup\` 🟡 (file chưa lưu) |
| Neovim | `%LOCALAPPDATA%\nvim-data\` (`lazy\` plugin 🔴, `shada\` 🟡, `swap\`/`undo\` 🟡, `log` 🟢) |
| Obsidian | `%APPDATA%\obsidian\Cache*\`, `GPUCache\` 🟢; vault 🔴 |
| Postman / Insomnia | `%APPDATA%\Postman\Cache\`, `Partitions\*\Cache\` 🟢; `storage\` 🔴 |
| Docker Desktop | `%LOCALAPPDATA%\Docker\wsl\data\ext4.vhdx` 🔴 (chỉ hướng dẫn `docker system prune` + `Optimize-VHD`); `%LOCALAPPDATA%\Docker\log\` 🟢 |
| WSL distros | `%LOCALAPPDATA%\Packages\<Distro>\LocalState\ext4.vhdx` 🔴 (hướng dẫn `wsl --manage --set-sparse`) |
| Git | `<repo>\.git\objects\` 🔴 — không bao giờ; gợi ý `git gc` |
| Terminal (Windows Terminal) | `%LOCALAPPDATA%\Packages\Microsoft.WindowsTerminal_*\LocalState\settings.json` 🔴 |

---

## 3. Package managers & toolchains

| Tool | Đường dẫn | Mức | Nếu xóa thì… | Cách "đúng" thay thế |
|------|-----------|-----|--------------|----------------------|
| npm | `%LOCALAPPDATA%\npm-cache\` | 🟢 | Tải lại package | `npm cache clean --force` |
| pnpm | `%LOCALAPPDATA%\pnpm\store\` | 🟡 | node_modules của project đang hard-link vào store → phải `pnpm install` lại | `pnpm store prune` (🟢) |
| Yarn | `%LOCALAPPDATA%\Yarn\Cache\`, `Berry\cache\` | 🟢 | Tải lại | `yarn cache clean` |
| Bun | `%USERPROFILE%\.bun\install\cache\` | 🟢 | Tải lại | — |
| Deno | `%LOCALAPPDATA%\deno\` | 🟢 | Tải lại | — |
| nvm-windows | `%APPDATA%\nvm\v*\` (phiên bản Node không dùng) | 🟠 | Mất bản Node | `nvm uninstall` |
| pip | `%LOCALAPPDATA%\pip\cache\` | 🟢 | Tải lại | `pip cache purge` |
| uv | `%LOCALAPPDATA%\uv\cache\` | 🟢 | Tải lại | `uv cache clean` |
| Poetry | `%LOCALAPPDATA%\pypoetry\Cache\` | 🟢 | — | `poetry cache clear` |
| Conda / Mamba | `<conda>\pkgs\` | 🟢 | Tải lại | `conda clean --all` |
| Conda envs | `<conda>\envs\` | 🔴 | Mất môi trường | — |
| Cargo | `%USERPROFILE%\.cargo\registry\cache\`, `registry\src\`, `git\checkouts\` | 🟢 | Tải/giải nén lại crate | `cargo cache -a` |
| Cargo | `%USERPROFILE%\.cargo\registry\index\` | 🟢 | Fetch lại index | — |
| Cargo | `%USERPROFILE%\.cargo\bin\`, `config.toml`, `credentials.toml` | 🔴 | Mất tool/credential | — |
| rustup | `%USERPROFILE%\.rustup\downloads\`, `tmp\` | 🟢 | — | — |
| rustup | `%USERPROFILE%\.rustup\toolchains\<old-nightly>\` | 🟠 | Mất toolchain | `rustup toolchain uninstall` |
| Go | `%LOCALAPPDATA%\go-build\` | 🟢 | Build lại | `go clean -cache` |
| Go | `%USERPROFILE%\go\pkg\mod\` | 🟡 | Tải lại module (chậm) | `go clean -modcache` |
| NuGet | `%USERPROFILE%\.nuget\packages\` | 🟡 | Restore lại | `dotnet nuget locals all --clear` |
| NuGet | `%LOCALAPPDATA%\NuGet\v3-cache\`, `plugins-cache\`, `%LOCALAPPDATA%\Temp\NuGetScratch\` | 🟢 | — | — |
| .NET | `%USERPROFILE%\.dotnet\TelemetryStorageService\`, `toolResolverCache\` | 🟢 | — | — |
| Gradle | `%USERPROFILE%\.gradle\caches\`, `daemon\*\*.log`, `wrapper\dists\<old>\` | 🟢/🟡 | Tải lại dependency (chậm) | — |
| Maven | `%USERPROFILE%\.m2\repository\` | 🟡 | Tải lại | — |
| Composer | `%LOCALAPPDATA%\Composer\` | 🟢 | — | `composer clear-cache` |
| Chocolatey | `%TEMP%\chocolatey\`, `C:\ProgramData\chocolatey\lib-bad\`, `lib-bkp\` | 🟢 | — | — |
| Scoop | `%USERPROFILE%\scoop\cache\` | 🟢 | — | `scoop cache rm *` |
| winget | `%LOCALAPPDATA%\Packages\Microsoft.DesktopAppInstaller_*\LocalCache\`, `%TEMP%\WinGet\` | 🟢 | — | — |
| Homebrew (macOS) | `~/Library/Caches/Homebrew` | 🟢 | — | `brew cleanup` |

---

## 4. Build artifacts trong project (quét theo marker)

| Marker | Thư mục xóa được | Mức | Nếu xóa thì… |
|--------|------------------|-----|--------------|
| `Cargo.toml` | `target\` | 🟢 | Build lại (có thể rất lâu với project lớn) → 🟡 nếu > 5 GB |
| `package.json` | `node_modules\`, `.next\`, `.nuxt\`, `.turbo\`, `.parcel-cache\`, `.svelte-kit\`, `dist\`, `build\`, `.vite\`, `.cache\` | 🟢 | `npm install`/build lại |
| `pyproject.toml` / `requirements.txt` | `__pycache__\`, `.pytest_cache\`, `.mypy_cache\`, `.ruff_cache\`, `.tox\`, `.nox\`, `*.egg-info\`, `build\`, `dist\` | 🟢 | — |
| `pyproject.toml` | `.venv\`, `venv\` | 🟡 | Tạo lại môi trường |
| `*.sln` / `*.csproj` | `bin\`, `obj\`, `.vs\` | 🟢 | Build lại |
| `go.mod` | — (dùng cache global) | — | — |
| `CMakeLists.txt` | `build\`, `cmake-build-*\`, `out\build\` | 🟢 | Configure lại |
| `build.gradle(.kts)` | `build\`, `.gradle\` | 🟢 | Build lại |
| `pom.xml` | `target\` | 🟢 | — |
| `Gemfile` | `vendor\bundle\`, `tmp\cache\` | 🟡 | `bundle install` |
| `.terraform.lock.hcl` | `.terraform\` | 🟡 | `terraform init` lại; **state 🔴** |
| `Dockerfile` | — | — | Gợi ý `docker builder prune` |

### 4.1 Game engines (Unity / Unreal / Godot)

| Engine | Marker | Thư mục | Mức | Nếu xóa thì… |
|--------|--------|---------|-----|--------------|
| **Unity** | `ProjectSettings\ProjectVersion.txt` | `Library\` | 🟡 | Re-import toàn bộ asset (có thể hàng giờ với project lớn); mất Library/ArtifactDB; **không mất asset** |
| Unity | — | `Temp\`, `Logs\`, `obj\`, `Build\` (nếu không phải output chính) | 🟢 | — |
| Unity | — | `Library\PackageCache\` | 🟢 | Resolve lại package |
| Unity | — | `Library\ShaderCache\`, `Library\Bee\`, `Library\Artifacts\` (chỉ xóa cùng Library) | 🟡 | Rebuild |
| Unity (global) | — | `%LOCALAPPDATA%\Unity\cache\`, `%LOCALAPPDATA%\Unity\Caches\`, `%APPDATA%\Unity\Asset Store-5.x\` | 🟡 | Tải lại package/asset store |
| Unity Hub | — | `%APPDATA%\UnityHub\logs\`, `Cache\` | 🟢 | — |
| Unity Editor | `%ProgramFiles%\Unity\Hub\Editor\<ver>\` | 🟠 | Gỡ qua Hub | — |
| Unity | `%LOCALAPPDATA%\Unity\Editor\Editor.log`, `Editor-prev.log`, `%LOCALAPPDATA%\Temp\Unity\` | 🟢 | — |
| **Unreal** | `*.uproject` | `Intermediate\`, `Saved\Logs\`, `Saved\Crashes\`, `Saved\Autosaves\` (cũ) | 🟢 | Build lại |
| Unreal | — | `DerivedDataCache\` (local) | 🟡 | Rebuild shader/DDC (rất lâu) |
| Unreal | — | `Binaries\` | 🟡 | Compile lại; **không xóa nếu project Blueprint-only dùng prebuilt** |
| Unreal | — | `Saved\` (toàn bộ) | 🟠 | Mất config local, autosave, collections |
| Unreal (global) | — | `%LOCALAPPDATA%\UnrealEngine\Common\DerivedDataCache\` | 🟡 | Rebuild DDC cho mọi project |
| Unreal | — | `%LOCALAPPDATA%\UnrealEngine\<ver>\Saved\Logs\`, `Crashes\`, `%LOCALAPPDATA%\UnrealEngine\Common\Zen\` | 🟢/🟡 | — |
| Epic Launcher | `%LOCALAPPDATA%\EpicGamesLauncher\Saved\webcache*\`, `Logs\` | 🟢 | — |
| **Godot** | `project.godot` | `.godot\` | 🟢 | Re-import |
| Godot | — | `%APPDATA%\Godot\app_userdata\`, `editor_data\` | 🔴/🟡 | user data 🔴 |
| Perforce | `.p4config` | `%LOCALAPPDATA%\Perforce\`… | 🔴 | — |
| **Shader/GPU** | — | `%LOCALAPPDATA%\NVIDIA\DXCache\`, `GLCache\`, `%LOCALAPPDATA%\NVIDIA Corporation\NV_Cache\`, `%LOCALAPPDATA%\D3DSCache\`, `%LOCALAPPDATA%\AMD\DxCache\`, `GLCache\`, `%LOCALAPPDATA%\Intel\ShaderCache\`, `<Steam>\steamapps\shadercache\` | 🟢 | Shader compile lại lần chạy đầu (stutter) |

---

## 5. Windows system (tham chiếu nhanh; chi tiết ở `rules/windows-system.toml`)

| Đường dẫn | Mức | Ghi chú |
|-----------|-----|---------|
| `%TEMP%`, `%SystemRoot%\Temp` | 🟢 | Bỏ qua file < 24 h và file đang khóa |
| `%LOCALAPPDATA%\Microsoft\Windows\Explorer\thumbcache_*.db`, `iconcache_*.db` | 🟢 | Explorer phải đóng (hoặc tự tạo lại sau reboot) |
| `%LOCALAPPDATA%\Microsoft\Windows\INetCache\`, `WebCache\` | 🟢 | — |
| `%LOCALAPPDATA%\CrashDumps\`, `%SystemRoot%\Minidump\`, `%SystemRoot%\MEMORY.DMP`, `%SystemRoot%\LiveKernelReports\` | 🟢 | Mất dump để debug BSOD |
| `%SystemRoot%\Logs\CBS\*.log` (cũ), `%SystemRoot%\Logs\DISM\` | 🟢 | — |
| `%SystemRoot%\SoftwareDistribution\Download\` | 🟠 | Dừng `wuauserv` trước; mất gói update đã tải |
| `%SystemRoot%\SoftwareDistribution\DeliveryOptimization\`, `%ProgramData%\Microsoft\Windows\DeliveryOptimization\Cache\` | 🟠 | Dùng Storage Sense tốt hơn |
| `%SystemRoot%\Prefetch\` | 🔴 | Xóa làm chậm boot/launch, không tiết kiệm đáng kể |
| `%ProgramData%\Package Cache\` | 🟠 | Cần cho repair/uninstall VC++/VS/.NET — chỉ Expert |
| `%ProgramData%\Microsoft\Windows Defender\Scans\History\` | 🟢 | Mất lịch sử quét Defender |
| `%ProgramData%\NVIDIA Corporation\Downloader\`, `%ProgramFiles%\NVIDIA Corporation\Installer2\` | 🟢/🟠 | Gói driver cũ |
| `C:\Windows.old` | 🔴 | Hướng dẫn Disk Cleanup/Storage Sense |
| `C:\$Recycle.Bin` | 🔴 | Chỉ qua API `SHEmptyRecycleBin` |
| `C:\hiberfil.sys`, `pagefile.sys`, `swapfile.sys` | 🔴 | Hướng dẫn `powercfg /h off`, cấu hình virtual memory |
| `C:\Windows\WinSxS`, `Installer`, `servicing`, `System32` | 🔴 | Hướng dẫn `DISM /Online /Cleanup-Image /StartComponentCleanup /ResetBase` |
| `System Volume Information` | 🔴 | Hướng dẫn `vssadmin delete shadows` / System Protection |
| `C:\ProgramData\Microsoft\Search\Data\` (Windows Search index) | 🟠 | Rebuild index — hướng dẫn qua Indexing Options |
| `%LOCALAPPDATA%\Packages\*\AC\INetCache`, `TempState\`, `LocalCache\` | 🟢 | App UWP/MSIX |
| `%LOCALAPPDATA%\Microsoft\Teams\`, `%LOCALAPPDATA%\Packages\MSTeams_*\LocalCache\Microsoft\MSTeams\EBWebView\*\Cache` | 🟢 | Teams cache |
| OneDrive `%LOCALAPPDATA%\Microsoft\OneDrive\logs\`, `setup\logs\` | 🟢 | — |
| Browsers (Chrome/Edge/Brave/Firefox) `Cache`, `Code Cache`, `GPUCache`, `Service Worker\CacheStorage`, Firefox `cache2` (`%LOCALAPPDATA%\Mozilla\Firefox\Profiles\*\cache2`) | 🟢 | Trình duyệt phải đóng; **không** đụng `Login Data`, `Cookies`, `History`, `Bookmarks` |

---

## 6. Quy tắc phát hiện "thư mục lạ" (unknown dev caches)

Trong `%USERPROFILE%`, `%APPDATA%`, `%LOCALAPPDATA%`, `%LOCALAPPDATA%\Programs`, `%USERPROFILE%\.cache`, `%USERPROFILE%\.config`, `%USERPROFILE%\.local\share`:

- Thư mục bắt đầu bằng `.` hoặc chứa thư mục con tên `cache|Cache|caches|tmp|temp|logs|log|sessions|history|index|blobs|models` → ứng viên.
- Chưa có rule → mức 🟡 "Chưa xác minh", hiển thị kích thước và ngày sửa cuối, kèm nút "Đề xuất rule" (mở issue template có sẵn thông tin).
- Không bao giờ tự động đưa vào "Dọn an toàn".

---

## 7. Đóng góp rule

1. Thêm `[[rule]]` vào file pack phù hợp trong `rules/`.
2. Bắt buộc: `id`, `vendor`, `app`, `paths`, `level`, `impact`, `verified_on`.
3. Chạy `nfd rules validate rules/` — CI sẽ chặn nếu rule khớp bất kỳ Protected path.
4. Mô tả cách bạn xác minh (phiên bản app, hành vi sau khi xóa) trong PR.
