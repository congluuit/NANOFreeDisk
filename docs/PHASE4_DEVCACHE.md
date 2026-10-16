# Phase 4 — DevCache Manager: kế hoạch chi tiết

> Trạng thái: **kế hoạch** (2026-10-16) — checklist ở `docs/PLAN.md` §12 "Phase 4". Bản chi tiết của hàng "4 – DevCache Manager" trong `docs/PLAN.md` §8, theo thiết kế §4.2 (định dạng rule, RiskLevel, marker build artifact), §4.3 (UI DevCache) và các mục ① của nhóm C trong §13.2: **C1** DevCache Manager, **C2** Toolchain Inventory (cơ bản), **C3** Repo Finder, **C4** dọn package store bằng lệnh chính chủ, **C9** chính sách build artifact. Theo §13.4, Phase 4 kéo dài **3 tuần**. Nguồn dữ liệu: `docs/DEVTOOL_CACHE_CATALOG.md` và `rules/ai-tools.toml`.

## 1. Mục tiêu & DoD

- Một view **DevCache** cho lập trình viên trả lời 3 câu cho mọi thư mục tạm của AI agent / IDE / package manager / game engine: **xóa được không, mất gì, phải đóng app nào** — nhóm theo **vendor → app → thư mục**.
- Không thêm đường xóa mới: mọi thao tác xóa đi qua `Planner → SafetyGate → Executor → UndoJournal` của Phase 3 (Quarantine / Recycle Bin, có trong Activity log, Khôi phục được). Lệnh dọn chính chủ (C4) chỉ chạy sau khi hiện **đúng dòng lệnh** và người dùng xác nhận; lệnh không có cách hoàn tác → chế độ *Hướng dẫn* (chỉ hiện + sao chép lệnh).
- **DoD (§8):**
  - Catalog **≥ 60 rule** trong 4 pack DevCache (`ai-tools`, `ide-editors`, `package-managers`, `game-engines`), mọi rule có `impact` EN + `impact_vi` + `verified_on`; `nfd rules validate rules/` không lỗi; test chặn pack DevCache tụt dưới 60 rule.
  - **Phát hiện thư mục lạ** trong `%USERPROFILE%` / `%APPDATA%` / `%LOCALAPPDATA%` (+ `.cache`, `.config`, `.local\share`, `%LOCALAPPDATA%\Programs`): chưa có rule + có dấu hiệu cache/session/logs → 🟡 "Chưa xác minh" + link đóng góp rule; không bao giờ vào "Dọn an toàn".
  - C2 / C3 / C4 / C9 mức ① chạy được trên máy dev; EN + VI đủ (`locales/{en,vi}/devcache.ftl`); `gui-smoke.mjs` phủ view DevCache (fixture riêng, không ghi vào history DB thật).

## 2. Thành phần

### 2.1 Rule pack (`rules/`, `nfd-rules`)

- Pack mới: **`ide-editors`** (VS Code + fork Cursor/Windsurf/Kiro/Trae/VSCodium, JetBrains, Visual Studio, Android Studio/SDK, Sublime, Zed, Obsidian, Postman…), **`package-managers`** (npm, pnpm, Yarn, Bun, Deno, pip, uv, Poetry, Conda, Cargo, rustup, Go, NuGet, .NET, Gradle, Maven, Composer, Scoop, Chocolatey, winget…), **`game-engines`** (cache toàn cục của Unity / Unity Hub / Unreal / Epic Launcher / Godot). Mở rộng **`ai-tools`** (Copilot CLI, Continue, Cline/Roo, Zed AI, PyTorch/Keras, Jan, GPT4All…).
- `[pack]` thêm cờ `devcache = true` → view DevCache lọc theo cờ này; Clean (Phase 3) vẫn quét mọi pack như cũ.
- Rule 🔴 Protected tiếp tục là **danh sách 🔒 Giữ lại** (config, auth, extension, memory…): matcher liệt kê nhưng không bao giờ đi vào.
- Build artifact **trong project** không làm bằng rule glob (gate cấm rule đụng Documents/Desktop… và glob `**/node_modules` trên cả ổ là quá rộng) mà bằng C9 (§2.4).
- Test: mỗi pack DevCache hợp lệ, ≥ 60 rule tổng, rule 🟢 phải `regenerates = true`, mọi rule có `requires_closed` hoặc là log/cache không bị khóa, không rule nào khớp `.git`, `extensions`, `settings.json`, `credentials`, `auth`, `token`.

### 2.2 Crate mới `nfd-devtools` (feature crate theo §13.5)

Phụ thuộc `nfd-core`, `nfd-platform`, `nfd-rules`. Không có hành động xóa riêng — chỉ sinh `PlanItem` cho Planner hoặc mô tả lệnh.

| Module | Việc |
|---|---|
| `devcache` | Gom `MatchReport` của các pack `devcache` thành cây **vendor → app → rule** (tổng kích thước, số tệp, lần sửa cuối, mức cao nhất, app đang chạy); danh sách 🔒 Keep; ngưỡng theo vendor (`thresholds`) → cờ "vượt ngưỡng" |
| `unknown` | Thư mục lạ: con trực tiếp của các gốc ở §1 không nằm dưới / chứa đường dẫn của rule nào, không thuộc danh sách bỏ qua (Microsoft, Packages, Temp, Programs của Windows, thư mục dữ liệu chính app…); tên bắt đầu bằng `.` **hoặc** có thư mục con tên `cache|caches|tmp|temp|logs|log|sessions|history|index|blobs|models`; tính kích thước có giới hạn thời gian/mục; kết quả 🟡 "Chưa xác minh" + URL issue GitHub điền sẵn (tên thư mục, kích thước, thư mục con — không có tên người dùng) |
| `toolchains` (C2 cơ bản) | rustup toolchains (+ mặc định), nvm-windows / fnm / Volta Node, pyenv-win, Python của py launcher, Conda envs, JDK (`~/.jdks`, Adoptium/Java), Gradle wrapper dists, Go SDK (`~/sdk`), Android SDK (platforms, system-images, NDK, build-tools), .NET SDK, Unity Editors (Hub), Unreal Engine, Flutter (fvm), Visual Studio (`vswhere`), WSL/Docker vhdx (kích thước). Mỗi mục: kích thước, lần sửa cuối, đang mặc định hay không, **lệnh gỡ đúng cách** (`rustup toolchain uninstall …`, `nvm uninstall …`, `sdkmanager --uninstall …`, Unity Hub / Visual Studio Installer…) — **chỉ Hướng dẫn**, không bao giờ xóa thư mục thô |
| `repos` (C3) | Tìm git repo dưới thư mục người dùng chọn (không follow link, không vượt volume, giới hạn độ sâu, không đi vào artifact / `.git`); mỗi repo: kích thước `.git`, artifact (C9), commit cuối (đọc `.git/logs/HEAD`, không cần git), "hoạt động cuối" (max mtime `.git/index`, `HEAD`, `FETCH_HEAD`, `logs/HEAD`), có thay đổi chưa commit (`git status --porcelain` nếu có git, có timeout), stale > N ngày |
| `artifacts` (C9) | Bảng marker → thư mục artifact + mức: `Cargo.toml`→`target` (🟡 nếu > 5 GB), `package.json`→`node_modules`, `.next`, `.nuxt`, `.turbo`, `.svelte-kit`, `.parcel-cache`, `.vite`, `.angular`; Python→`__pycache__`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.tox`, `.nox`, `.venv` 🟡; `*.sln`/`*.csproj`→`bin`, `obj`, `.vs`; `CMakeLists.txt`→`cmake-build-*`, `out/build`; Gradle→`build`, `.gradle`; `pom.xml`→`target`; Unity (`ProjectSettings/ProjectVersion.txt`)→`Library` 🟡, `Temp`, `Logs`, `obj`; Unreal (`*.uproject`)→`Intermediate`, `Saved/Logs`, `Saved/Crashes`, `DerivedDataCache` 🟡, `Binaries` 🟡; Godot (`project.godot`)→`.godot`. Tên chung chung (`build`, `dist`, `out`, `bin`, `obj`, `vendor`) **chỉ** được đề xuất khi git xác nhận là bị ignore (`git check-ignore --stdin`) — không có git → không đề xuất. Thư mục chứa tệp được git theo dõi → không bao giờ. **Giữ artifact của repo hoạt động trong 7 ngày gần nhất** (mặc định bật, chỉnh được) — vẫn liệt kê, không chọn sẵn |
| `stores` (C4) | Danh mục lệnh chính chủ: `npm cache clean --force`, `pnpm store prune`, `yarn cache clean`, `pip cache purge`, `uv cache clean`, `poetry cache clear --all .`, `conda clean --all -y`, `go clean -cache` / `-modcache`, `dotnet nuget locals all --clear`, `cargo cache --autoclean` (khi có `cargo-cache`), `composer clear-cache`, `scoop cache rm *`, `ollama rm <model>`, `hf cache delete`, `git gc` (repo). Mỗi lệnh: exe + đối số (không qua shell), thư mục store để đo trước/sau, `undo = redownload | none`. `redownload` → **Chạy** sau khi hiện đúng dòng lệnh + xác nhận; `none` → **Hướng dẫn**. Chỉ chạy exe tìm thấy trên `PATH` (đường dẫn tuyệt đối hiện trong dialog), timeout 10 phút, hủy được, ghi vào Activity log (kind `command`, không khôi phục được — ghi rõ) và audit log |

### 2.3 Đường xóa

- Rule DevCache → `Candidate::to_plan_item` (Quarantine) như Clean.
- Artifact repo (C3/C9) → `PlanItem` origin **Manual** (người dùng chọn repo/thư mục; gate cho phép bên trong Documents nhưng vẫn chặn Windows, Program Files, gốc ổ, link…) với mode **Quarantine** (thay vì Recycle Bin) — Restore trong Activity log, TTL 7 ngày; kind `devcache`.
- Kiểm tra lại trên đĩa ngay trước khi làm (Phase 3): artifact đổi loại / bị thay bằng link / nằm trên volume khác → bỏ qua có lý do.

### 2.4 CLI

- `nfd devcache [list] [--vendor <v>] [--level safe|caution|advanced] [--json]` — cây vendor → app → rule (schema `nanofreedisk.devcache.v1`).
- `nfd devcache clean [--vendor <v>] [--rule …] [--level safe] [--apply] [-y]` — dùng chung `nfd clean` (dry run mặc định).
- `nfd devcache unknown [--json]` — thư mục lạ.
- `nfd toolchains [--json]` — C2, in lệnh gỡ.
- `nfd repos <root> [--keep-recent 7] [--stale 90] [--json]`, `nfd repos clean <root> [--apply] [-y]` — C3/C9.
- `nfd stores [--json]`, `nfd stores run <id> [-y]` — C4 (lệnh *Hướng dẫn* chỉ in ra, không chạy).

### 2.5 GUI — view **DevCache** (thay mục "P4" trong sidebar)

Tab: **Cache** · **Toolchain** · **Repo** · **Package store** · **Thư mục lạ**.

- **Cache**: thẻ theo vendor (tổng, số app, badge "vượt ngưỡng"), mở ra app → thư mục: kích thước, số tệp, lần sửa cuối, mức rủi ro, **"Nếu xóa thì…"**, app cần đóng (đang chạy → cảnh báo vàng), nút *Mở thư mục* / *Thêm vào plan* (tick) / xem trước 50 mục lớn nhất; nút **"Dọn an toàn (chỉ 🟢)"** trên mỗi vendor và trên cùng (mọi vendor); khu **🔒 Giữ lại** (rule Protected) liệt kê cái app cố tình bỏ qua; dialog xác nhận tổng GB (dùng chung với Clean), kết quả + Hoàn tác → Activity.
- **Toolchain**: bảng theo công cụ (phiên bản, kích thước, lần sửa cuối, ★ mặc định), nút *Sao chép lệnh gỡ* / *Mở thư mục*.
- **Repo**: chọn thư mục gốc (nhớ trong Settings), bảng repo (tên, đường dẫn, `.git`, artifact, commit cuối, hoạt động cuối, có thay đổi chưa commit, stale), mở ra từng artifact (marker, mức, "gần đây — giữ"); chọn → Quarantine; *git gc* (C4 dialog) / *Mở thư mục*.
- **Package store**: mỗi công cụ tìm thấy: kích thước store, dòng lệnh chính xác, nút *Chạy* (dialog xác nhận hiện exe tuyệt đối + đối số, kết quả: giải phóng X, stdout/stderr rút gọn) hoặc *Sao chép* (Hướng dẫn).
- **Thư mục lạ**: danh sách 🟡 "Chưa xác minh" (kích thước, sửa cuối, gợi ý vì sao), *Mở thư mục*, *Đề xuất rule* (mở issue GitHub điền sẵn).
- **Ngưỡng / nhắc**: Settings → DevCache: ngưỡng GB theo vendor (mặc định Ollama 50 GB, Hugging Face 30 GB, JetBrains 10 GB), bật/tắt thông báo khi vượt (kiểm tra lúc quét DevCache).
- Simple mode: ẩn 🟠 Advanced, tab Toolchain/Package store chỉ đọc; Expert: hiện 🟠 + lệnh `nfd` tương đương (CliHint). Mọi nút có `data-effect` + tooltip "Nếu bấm thì…" (G4). EN + VI trong `locales/{en,vi}/devcache.ftl`; mọi màu là theme token.

## 3. Kiểm thử

- `nfd-rules`: pack DevCache hợp lệ, ≥ 60 rule, đã dịch, không rule nào chạm config/credential/extension; test catalog nhúng = thư mục `rules/`.
- `nfd-devtools` (thư mục tạm): gom vendor/app; thư mục lạ (được phát hiện / bị bỏ qua vì đã có rule / vì là thư mục hệ thống / không có dấu hiệu cache); toolchain (rustup với `settings.toml`, nvm, Android SDK giả); repo finder (repo lồng, worktree `.git` là tệp, link không bị follow, độ sâu); C9 (marker đúng/sai, tên chung chung không có git → không đề xuất, tệp được track → không đề xuất, repo gần đây → không chọn sẵn, Unity Library 🟡); C4 (dựng dòng lệnh, không qua shell, `none` → không chạy được, exe không có trên PATH → ẩn).
- Plan: artifact → Planner → Gate (thư mục trong Windows / Program Files bị chặn) → Executor → Quarantine → Restore khôi phục đúng.
- `gui-smoke.mjs`: fixture riêng (pack test + repo giả trong thư mục tạm, DB lịch sử tạm), mở DevCache, dọn an toàn 1 vendor, khôi phục từ Activity, tab Repo dọn artifact, tab Thư mục lạ.

## 4. Lát cắt (mỗi lát 1 commit)

1. Tài liệu này + checklist PLAN §12.
2. Rule pack `ide-editors`, `package-managers`, `game-engines` + mở rộng `ai-tools` (≥ 60 rule DevCache), cờ `devcache`, test pack.
3. `nfd-devtools`: `devcache` (vendor → app → rule, Keep, ngưỡng) + `unknown` + `nfd devcache [list|clean|unknown]`.
4. `toolchains` (C2) + `repos` (C3) + `artifacts` (C9) + `nfd toolchains`, `nfd repos [clean]`.
5. `stores` (C4) + runner (xác nhận, timeout, Activity/audit) + `nfd stores [run]`.
6. Tauri commands + view DevCache (5 tab) + Settings (ngưỡng, thư mục repo, giữ 7 ngày) + `devcache.ftl` EN/VI + mock + `gui-smoke`.
7. CHANGELOG, PLAN, README; đồng bộ `main`; fmt / clippy / test / check-locales / svelte-check / gui-smoke.

## 5. Rủi ro

- **Rule sai đường dẫn / app đổi cấu trúc** → `verified_on` mỗi rule, gate vẫn kiểm từng mục; thư mục chưa biết chỉ hiện, không dọn.
- **Artifact là output thật** (repo commit `dist/`, project Blueprint-only cần `Binaries/`) → tên chung chung cần git ignore; tệp được track → bỏ; `Binaries` / `Library` / `DerivedDataCache` 🟡 (không chọn sẵn); project hoạt động 7 ngày gần đây → không chọn sẵn.
- **pnpm store / Gradle cache bị hard-link / dùng chung** → dọn qua lệnh chính chủ (`pnpm store prune`) thay vì xóa thô; rule thô của pnpm store là 🟡.
- **Lệnh chính chủ treo / hỏi tương tác** → đối số không tương tác (`--force`, `-y`), stdin rỗng, timeout, hủy được; chỉ exe trên `PATH`, không bao giờ qua `cmd /c`.
- **Quét repo chậm trên ổ lớn** → người dùng chọn gốc, giới hạn độ sâu (mặc định 6), không đi vào artifact / `.git` / thư mục hệ thống, hủy được, `git status` chỉ khi bật.
- **Toolchain gỡ sai** → chỉ Hướng dẫn trong Phase 4.

## 6. Ngoài phạm vi Phase 4

- C2 đầy đủ (gỡ toolchain trực tiếp, Visual Studio workloads chi tiết) → v1.x (②); C5 Container & VM (Docker prune, compact `ext4.vhdx`), C6 PATH Doctor, C7 IDE hygiene (workspaceStorage của project đã xóa), C8 log-spam → v1.1+.
- Archive repo thành zip, *Mở trong IDE* theo từng IDE → v1.x; đặt **lịch** dọn tự động → v1.x (scheduled clean, §8 hàng 11) — Phase 4 chỉ nhắc khi vượt ngưỡng.
- Thư mục lạ chỉ hiện + đề xuất rule, không có nút xóa; pack `messaging-apps` và cộng đồng đóng góp rule pack từ xa → v1.x.
