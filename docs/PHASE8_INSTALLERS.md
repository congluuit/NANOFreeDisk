# Phase 8 — Installers: kế hoạch chi tiết

> Trạng thái: **xong trên máy dev** (2026-10-17) — lát 1 ✅ (gói Tauri MSI + NSIS, dọn khi gỡ), lát 2 ✅ (portable), lát 3 ✅ (bộ cài thương hiệu egui), lát 4 ✅ (updater), lát 5 ✅ (release.yml, ký số tùy chọn, smoke test). MSI và bộ cài "Mọi người" (cần admin) đã pass trong `release.yml` dry run trên `windows-latest`. Còn lại: VM Win10/11 sạch chưa kiểm; SignPath và secret updater do tác giả thiết lập (§2.5, §6). Bản chi tiết của hàng "8 – Installers" trong `docs/PLAN.md` §8, theo thiết kế §6.1–§6.3 và pipeline §7.2. Helper elevated / PawnIO theo `docs/adr/0002-hardware-sensors-pawnio.md`.

## 1. Mục tiêu & DoD

- Mỗi version phát hành **bốn gói** (PLAN §6.1), cùng một `nanofreedisk.exe` + `nfd.exe`:

  | Gói | Tên file | Công nghệ | Phạm vi cài |
  |-----|----------|-----------|-------------|
  | **Bộ cài thương hiệu** ★ | `NANOFreeDisk-Setup-x.y.z-x64.exe` | Rust + egui (`apps/nanofreedisk-setup`), payload zstd gắn cuối exe | Chỉ tôi (mặc định, không UAC) hoặc Mọi người (UAC) |
  | MSI | `NANOFreeDisk-x.y.z-x64.msi` | WiX 3 qua Tauri bundler | Mọi người (per-machine, cho IT/GPO/Intune) |
  | EXE (NSIS) | `NANOFreeDisk-x.y.z-x64-setup.exe` | NSIS qua Tauri bundler | Chỉ tôi (`%LOCALAPPDATA%\NANOFreeDisk`, không UAC) |
  | Portable | `NANOFreeDisk-x.y.z-x64-portable.zip` | zip + `portable.flag` | Không cài; dữ liệu nằm cạnh exe |

  Kèm: `NANOFreeDisk-x.y.z-x64-vi-VN.msi` (MSI giao diện tiếng Việt — WiX tạo một MSI cho mỗi ngôn ngữ), `SHA256SUMS.txt` (+ `.minisig` khi có khóa), `sbom-x.y.z.cdx.json`, `latest.json` + `*.sig` (updater, khi có khóa), release notes từ `git-cliff`. Cả bốn gói dựng bằng một script: `scripts/build-packages.ps1 -Out dist`.
- **DoD** (PLAN §8 hàng 8): cài → chạy → gỡ **sạch** (không còn tệp, registry, shortcut, menu Explorer) cho cả bốn gói; **silent** cài/gỡ chạy được (`/S`, `/qn`); release-check vẫn chặn; gói chưa ký số vẫn build được và bước ký **tự bỏ qua** khi chưa có secret.
- Mọi chuỗi giao diện của bộ cài (egui, NSIS, WiX, hộp thoại dọn dữ liệu) có **EN + VI**.
- Kiểm tra trên VM Win10/11 sạch: **chưa làm được trên máy dev** (không có VM) — ghi rõ ở §6, chạy trong CI (`windows-latest` = Windows Server 2022/2025) và người dùng tự kiểm trên VM.

## 2. Thành phần

### 2.1 Gói Tauri: MSI + NSIS

- `tauri.conf.json` → `bundle.targets = ["msi", "nsis"]`, `webviewInstallMode = downloadBootstrapper` (Windows 10 thiếu WebView2 → tải bootstrapper chính chủ của Microsoft rồi cài Evergreen; Windows 11 có sẵn).
- `nfd.exe` (CLI) và `rules/` đi kèm app qua `bundle.resources` trong **cấu hình riêng** `src-tauri/tauri.bundle.conf.json` (chỉ ghép khi `tauri build --config …`), để `cargo build` / `clippy` không đòi `target/release/nfd.exe` có sẵn.
- **NSIS**: `installMode = "currentUser"` — gói "cài kiểu truyền thống" cho riêng người dùng, không UAC (chế độ `both` của Tauri dùng `RequestExecutionLevel highest`, nên tài khoản admin luôn bị hỏi UAC kể cả khi cài cho riêng mình; "Mọi người" đã có MSI và bộ cài thương hiệu). Hai ngôn ngữ English / Vietnamese, hook `installer/nsis/hooks.nsh`:
  - `NSIS_HOOK_PREUNINSTALL` → gỡ verb Explorer (HKCU, giống `nfd_platform::shell_menu`) và chạy `nanofreedisk.exe --uninstall-cleanup` (§2.3) **trước** khi template của Tauri xóa `%APPDATA%` / `%LOCALAPPDATA%\vn.io.nanodev.nanofreedisk` (ô "Delete the application data"), để Quarantine không bao giờ bị xóa âm thầm.
  - `/REMOVEDATA` trên dòng lệnh = tích ô xóa dữ liệu khi gỡ silent; không có → giữ dữ liệu.
  - `NSIS_HOOK_POSTUNINSTALL` → xóa khóa `HKCU\Software\<publisher>\NANOFreeDisk` (thư mục cài + ngôn ngữ bộ cài; template chỉ xóa khi tích xóa dữ liệu) khi không phải chế độ update.
- **MSI**: per-machine (template WiX của Tauri cố định `InstallScope="perMachine"`), fragment `installer/wix/shell-menu-cleanup.wxs`: `RemoveRegistryKey` cho verb Explorer + custom action chạy `nanofreedisk.exe --uninstall-cleanup` trong `InstallUISequence` khi `REMOVE="ALL"` (chỉ khi có UI — gỡ `/qn` giữ nguyên dữ liệu người dùng). MSI không xóa dữ liệu người dùng (với GPO, gỡ chạy dưới SYSTEM, không biết hồ sơ nào): ghi rõ trong README.

### 2.2 Portable

- Zip gồm `nanofreedisk.exe`, `nfd.exe`, `rules/`, `LICENSE`, `README.md`, `portable.flag`.
- Khi `portable.flag` nằm cạnh exe đang chạy: mọi dữ liệu (lịch sử, settings, nhật ký, quarantine ổ hệ thống, hồ sơ WebView2 `data\EBWebView`) vào `<thư mục exe>\data\`. Một hàm duy nhất `nfd_history::portable_dir()` cho cả app và CLI; app gom mọi đường dẫn dữ liệu vào `paths.rs`. Cửa sổ chính được tạo trong `setup` (`"create": false` trong `tauri.conf.json`) với `data_directory`, vì Tauri luôn tạo `%LOCALAPPDATA%\<identifier>` cho WebView2 nếu không chỉ định — smoke test bắt được lỗi này.
- Quarantine của ổ khác vẫn là `<ổ>\.nfd-quarantine` (đổi tên không vượt volume — đã có trong `QuarantineRoots`): khi exe nằm trên USB, tệp ổ `C:` vào `C:\.nfd-quarantine`.
- Portable không ghi registry, không tự đăng ký menu Explorer (bật tay trong Settings thì tắt tay trước khi xóa thư mục), updater chỉ **thông báo** có bản mới.

### 2.3 Dọn dữ liệu khi gỡ: `nanofreedisk.exe --uninstall-cleanup`

Một đường duy nhất cho NSIS, MSI và bộ cài thương hiệu (không lặp logic trong NSIS script):

- Đọc journal (`history.db`) → các thao tác còn mục trong **Quarantine** (`quarantine`, `hardlink`, `clone` chưa khôi phục, chưa purge) → số mục + dung lượng.
- Có mục → hộp thoại rõ ràng (EN/VI theo `--lang`, settings, rồi ngôn ngữ Windows): **Khôi phục về chỗ cũ** / **Xóa vĩnh viễn** / **Để lại** (chỉ khi giữ dữ liệu). Khôi phục chỉ đụng mục Quarantine — mục trong Recycle Bin để Windows lo.
- `--remove-data` → sau đó xóa `%LOCALAPPDATA%\vn.io.nanodev.nanofreedisk` (lịch sử, cache hash, nhật ký, quarantine ổ hệ thống, WebView2), `%APPDATA%\vn.io.nanodev.nanofreedisk` (settings, nhiệt độ 24 h) và `%LOCALAPPDATA%\NANOFreeDisk\pawnio` (module PawnIO người dùng tải; driver thì không đụng); thư mục `<ổ>\.nfd-quarantine` rỗng bị xóa. Logic nằm ở `nfd_history::uninstall` (có test), hộp thoại ở `apps/nanofreedisk/src-tauri/src/uninstall.rs` (`MessageBoxW`).
- `--silent`: không hộp thoại; có `--remove-data` thì **khôi phục** Quarantine (an toàn hơn xóa vĩnh viễn), không thì để nguyên. `--quarantine restore|purge|keep` chọn tường minh.
- Luôn gỡ verb Explorer (`shell_menu::unregister`). App đang chạy → yêu cầu đóng (Thử lại / Bỏ qua).
- **Không đụng PawnIO**: driver dùng chung với app khác (ADR 0002), gỡ NANOFreeDisk để nguyên; app không bundle PawnIO cho tới khi tác giả xác nhận quyền phân phối lại — chỉ link `pawnio.eu`.
- Helper elevated (Advanced sensors) là chính `nanofreedisk.exe --nfd-helper`, không cài service / scheduled task → không có gì riêng để gỡ.

### 2.4 Bộ cài thương hiệu `apps/nanofreedisk-setup`

- Rust + **egui/eframe 0.32** (bản cuối hợp MSRV 1.85; backend wgpu: DirectX 12 — có WARP khi VM không có GPU — rồi OpenGL; không mở được cửa sổ → hộp thoại lỗi gợi ý `/S` hoặc gói NSIS/MSI), **không dùng WebView2**. Font lấy từ `C:\Windows\Fonts` (Segoe UI, Consolas) nên không bundle font nào. Icon `installer/icons/nanofreedisk.ico` (resource exe), màu theo `docs/BRAND.md` (navy `#0B1220` / `#111C33`, cyan `#22D3EE` → lime `#A3E635`), wordmark **NANO**FreeDisk. EN/VI, chọn ở góc trên, mặc định theo ngôn ngữ Windows.
- **Màn hình**: Splash → Chào mừng → Giấy phép (MIT) → Kiểu cài (Chỉ tôi `%LOCALAPPDATA%\Programs\NANOFreeDisk` / Mọi người `%ProgramFiles%\NANOFreeDisk` – UAC) → Thư mục & tùy chọn (Start Menu, Desktop, menu Explorer "Analyze with NANOFreeDisk") → Tiến trình (giải nén, kiểm tra WebView2, ghi registry) → Hoàn tất (Mở NANOFreeDisk, mở README).
- **Payload**: `nanofreedisk-setup.exe --pack <thư mục> --out <file>` ghép stub + zstd(`nanofreedisk.exe`, `nfd.exe`, `rules/`, `LICENSE`, `README.md`, `uninstall.exe` = chính stub) + footer. Exe đọc payload theo footer, bỏ qua bảng chứng chỉ Authenticode ở cuối file (đọc PE security directory) → ký số sau khi ghép vẫn đọc được.
- **Mọi người**: chọn → chạy lại chính nó với `runas` (UAC) và tham số đã chọn; tiến trình elevated cài rồi thoát.
- **WebView2**: đọc `pv` dưới `…\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` (HKLM WOW6432Node / HKLM / HKCU); thiếu → tải `MicrosoftEdgeWebview2Setup.exe` từ `go.microsoft.com/fwlink/p/?LinkId=2124703` (`URLDownloadToFileW`), **kiểm tra chữ ký Authenticode** (`WinVerifyTrust`, signer Microsoft) rồi chạy `/silent /install`. Không mạng → báo lỗi rõ, vẫn cài app (app báo thiếu WebView2 khi chạy).
- **Registry** `HKCU|HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\NANOFreeDisk`: DisplayName, DisplayVersion, Publisher, InstallLocation, UninstallString, QuietUninstallString, DisplayIcon, EstimatedSize, URLInfoAbout, NoModify, NoRepair, `NanoInstaller=setup` (để nhận ra bản của mình). Thấy bản cài bởi NSIS/MSI (không có dấu này) → báo và đề nghị gỡ bản đó trước.
- **Shortcut** `.lnk` qua `IShellLinkW` (crate `windows`, đã có trong lockfile qua Tauri).
- **Uninstaller** `uninstall.exe --uninstall`: trang xác nhận + ô "Xóa dữ liệu của tôi" (mặc định không) → `--uninstall-cleanup` (§2.3) → xóa tệp, shortcut, registry, verb Explorer; tự chép sang `%TEMP%` để xóa được thư mục cài, bản tạm tự xóa khi thoát.
- **Silent** tương thích NSIS: `/S`, `/D=<dir>` (cuối dòng), `/ALLUSERS`, `/NOSHORTCUTS`, `/NOSTARTMENU`, `/DESKTOP`, `/SHELLMENU`, `/LANG=en|vi`; gỡ: `uninstall.exe --uninstall /S [/REMOVEDATA]`. Mã thoát 0 = ok, 1 = lỗi, 2 = người dùng hủy, 3 = cần quyền admin, 4 = đã có bản cài bằng MSI/NSIS, 5 = app đang chạy từ thư mục cài.
- Xem lại thiết kế không cần cài: `NFD_SETUP_SHOTS=<thư mục>` chụp PNG từng trang (EN hoặc `/LANG=vi`) rồi thoát.

### 2.5 Updater

- `tauri-plugin-updater` (bản hợp MSRV 1.85 / Tauri 2.11) kiểm tra `https://github.com/congluuit/NANOFreeDisk/releases/latest/download/latest.json`; chữ ký minisign với **public key trong `tauri.conf.json`** (`plugins.updater.pubkey`).
- Cài tự động chỉ cho bản NSIS / MSI (Tauri biết loại gói); bản thương hiệu và portable: chỉ thông báo + nút mở trang Release. Settings → "Kiểm tra cập nhật" (thủ công; không tự kiểm khi chưa bật).
- `bundle.createUpdaterArtifacts` bật trong `tauri.bundle.conf.json` **chỉ khi** CI có `TAURI_SIGNING_PRIVATE_KEY` (không có → build không tạo `.sig`, không có `latest.json`, release vẫn chạy).
- Khóa sinh **ngoài repo** (`%USERPROFILE%\.nanofreedisk-keys\updater.key`), không commit, không in. Tác giả tự đưa lên GitHub:

  Khóa đã sinh trên máy tác giả (2026-10-17, `pnpm tauri signer generate`): `updater.key` (khóa riêng, có mật khẩu), `updater.key.pub` (đã chép vào `tauri.conf.json`), `updater.password` (mật khẩu ngẫu nhiên). Đưa lên GitHub (PowerShell, trong thư mục repo):

  ```powershell
  Get-Content "$env:USERPROFILE\.nanofreedisk-keys\updater.key" -Raw | gh secret set TAURI_SIGNING_PRIVATE_KEY --repo congluuit/NANOFreeDisk
  Get-Content "$env:USERPROFILE\.nanofreedisk-keys\updater.password" -Raw | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo congluuit/NANOFreeDisk
  ```

  Ký số (khi SignPath Foundation duyệt): `gh secret set SIGNPATH_API_TOKEN --repo congluuit/NANOFreeDisk` và `gh variable set SIGNPATH_ORGANIZATION_ID --repo congluuit/NANOFreeDisk --body "<org id>"`; project slug `nanofreedisk`, artifact configuration `installers`, policy `release-signing` / `test-signing` (dry run).

  Mất khóa riêng = bản đã cài không nhận cập nhật nữa → sao lưu thư mục khóa ra ngoài máy (USB / password manager).

### 2.6 `release.yml`

1. **release-check** (chặn, giữ nguyên): version Cargo / `tauri.conf.json` / `package.json` == tag, CHANGELOG có mục, fmt/clippy/test, rule packs, `cargo deny`.
2. **build-windows**: CLI → app (`tauri build --bundles msi,nsis --config tauri.bundle.conf.json`) → setup stub → `--pack` → zip portable → **ký số** (SignPath, chỉ khi `SIGNPATH_API_TOKEN` tồn tại; không có → bước bị bỏ qua, ghi notice) → `scripts/smoke-test.ps1` (cài/gỡ silent cả 3 bộ cài + kiểm không sót tệp/registry/shortcut/menu) → upload.
3. **publish**: SHA256SUMS, minisign (tùy chọn), SBOM, `latest.json` (từ `.sig` của NSIS nếu có), release notes, GitHub Release.
4. `workflow_dispatch` có input `dry_run` (mặc định **true**): build + smoke test từ một ref bất kỳ, version lấy từ `Cargo.toml`, **không** tạo tag / Release / winget.

## 3. Kiểm thử

- Unit: đọc payload (footer, bỏ qua bảng chứng chỉ), round-trip pack → unpack, tham số silent (`/D=` có khoảng trắng, `/ALLUSERS`), đường dẫn cài mặc định, phát hiện portable, `--uninstall-cleanup` trên journal tạm (khôi phục / xóa / để lại; mục Recycle Bin không bị đụng), sinh `latest.json`.
- Tích hợp trên máy dev (người dùng thường, không admin): build cả bốn gói; NSIS per-user và bộ cài thương hiệu per-user: cài có UI → mở app → gỡ; cài/gỡ silent; portable: chạy từ thư mục tạm, kiểm dữ liệu chỉ nằm trong `data\`. Sau mỗi vòng: không còn thư mục cài, khóa Uninstall, shortcut Start Menu/Desktop, verb Explorer, `%LOCALAPPDATA%`/`%APPDATA%` (khi chọn xóa dữ liệu). MSI cần admin (per-machine) → kiểm trong CI.
- CI: `smoke-test.ps1` cài/gỡ silent MSI + NSIS + thương hiệu trên `windows-latest` (runner có admin).

## 4. Lát cắt

1. **Gói Tauri + dọn khi gỡ** — `tauri.bundle.conf.json` (nfd.exe, rules), `--uninstall-cleanup` + `nfd_history::uninstall`, NSIS hooks (`/REMOVEDATA`, khóa ngôn ngữ), WiX custom action.
2. **Portable** — `portable_dir()`, `paths.rs` trong app, WebView2 data, CLI dùng chung, zip.
3. **Bộ cài thương hiệu** — `apps/nanofreedisk-setup` (wizard egui, payload, WebView2, registry, shortcut, uninstaller, silent).
4. **Updater** — plugin, pubkey, Settings → Kiểm tra cập nhật, `latest.json`.
5. **Release** — `release.yml` (dry-run, SignPath tùy chọn, latest.json), `smoke-test.ps1` kiểm sót, README mục cài đặt, CHANGELOG.

## 5. Ngoài phạm vi Phase 8

- Bundle PawnIO (chờ tác giả xác nhận quyền phân phối lại, ADR 0002).
- Tray monitor khởi động cùng Windows, file association `.nfdreport` (PLAN §6.2) — chưa có tính năng tương ứng trong app → v1.x.
- MSI per-user / dual-purpose (cần template WiX riêng) — v1.x nếu có người dùng doanh nghiệp yêu cầu.
- winget / Scoop: job winget có sẵn, chạy sau v1.0 khi có `WINGET_TOKEN`.
- Delta update, kênh beta riêng.

## 6. Rủi ro & việc còn lại

- **Chưa ký số** → SmartScreen cảnh báo "Unknown publisher". Đăng ký SignPath Foundation (cần repo public + 1 release); trong lúc chờ, README hướng dẫn kiểm SHA256 (+ minisign).
- **VM Win10/11 sạch chưa kiểm** (máy dev không có VM): người dùng chạy `scripts/smoke-test.ps1` trên VM, hoặc dựa vào CI `windows-latest`. Riêng WebView2 bootstrap chỉ kiểm được trên máy *thiếu* WebView2 (Win10 LTSC / VM sạch).
- egui/wgpu trên VM không GPU: DirectX 12 WARP; nếu vẫn lỗi → bộ cài in lỗi và gợi ý dùng gói NSIS.
- Antivirus heuristics với exe tự giải nén chưa ký: payload zstd chuẩn, không tự sửa mã, không ghi vào thư mục hệ thống ngoài thư mục cài.

## 7. Kết quả kiểm tra (2026-10-17, laptop tham chiếu, Windows 10 Pro 19045, người dùng thường)

- `scripts/build-packages.ps1` dựng đủ 4 gói (+ MSI tiếng Việt): Setup ≈ 18 MB (stub egui ≈ 8 MB + payload zstd gồm chính stub làm `uninstall.exe`), MSI ≈ 9,7 MB, NSIS ≈ 6,8 MB, portable ≈ 9,4 MB; có khóa updater thì thêm `.sig` cho NSIS và MSI.
- `scripts/smoke-test.ps1` (dữ liệu và menu Explorer thật của người dùng được cất sang chỗ khác rồi trả lại): portable (CLI, `nfd scan --save` ghi vào `data\`, app mở được, không ghi gì vào hồ sơ người dùng), NSIS per-user (cài silent → `nfd --version` → mở app → gỡ `/S /REMOVEDATA`), bộ cài thương hiệu per-user (cài `/S /DESKTOP /SHELLMENU` → kiểm Apps & features, shortcut, menu Explorer → mở app → gỡ `--uninstall /S /REMOVEDATA`): **pass**, không sót tệp / registry / shortcut / menu / thư mục dữ liệu. Lỗi tìm ra và đã sửa nhờ smoke test: portable tạo thư mục rỗng `%LOCALAPPDATA%\<identifier>` (WebView2), NSIS để lại `HKCU\Software\NANO (nanodev.io.vn)\NANOFreeDisk`.
- Wizard egui: xem từng trang EN/VI qua `NFD_SETUP_SHOTS` (splash, chào mừng, giấy phép, kiểu cài, tùy chọn, tiến trình, hoàn tất, lỗi "đã có bản MSI/NSIS", gỡ cài đặt). Bấm tay qua wizard và hộp thoại khu cách ly khi gỡ có UI: chưa tự động hóa được (không điều khiển được cửa sổ native) — người dùng kiểm khi thử bản dry run.
- MSI và bộ cài thương hiệu "Mọi người" (cần admin, không chạy được cục bộ): `release.yml` dry run (`workflow_dispatch`, run 38949675585, `windows-latest` = Windows Server) — portable, NSIS, MSI, Setup per-user và "Mọi người" đều cài → chạy → gỡ silent, không sót gì. Lần chạy đầu bắt được lỗi: tiến trình `msedgewebview2.exe` còn giữ hồ sơ WebView2 vài giây sau khi app bị tắt → smoke test chờ chúng, `--uninstall-cleanup` thử xóa lại tới 5 s.
