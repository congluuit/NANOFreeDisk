# Phase 3 — Safety + Clean: kế hoạch chi tiết

> Trạng thái: **đã làm xong 4 lát cắt** (2026-10-16) — checklist ở `docs/PLAN.md` §12 "Phase 3". Khác kế hoạch: quarantine trên ổ hệ thống nằm trong thư mục dữ liệu của app (vốn riêng tư cho user) thay vì `C:\.nfd-quarantine`; kiểm tra Authenticode được thay bằng quy tắc chặt hơn (mọi file chương trình dưới `%SystemRoot%` đều bị chặn); `cargo-fuzz` chưa làm. Bản chi tiết của hàng "3 – Safety + Clean" trong `docs/PLAN.md` §8, theo thiết kế §3.4, §4.2, §5. Chạy song song với Hardware Monitor (`docs/HARDWARE_MONITOR.md`) — xem mục 7 để hai bên không đụng file của nhau.

## 1. Mục tiêu & DoD

- Mọi thao tác xóa (Clean, Tidy up, Old & Large, xóa thủ công ở Analyze) đi qua **một** đường: `Planner → SafetyGate → Executor → UndoJournal`.
- Dọn junk theo rule pack: `windows-system` (đã có 21 rule), **`browsers`**, **`gpu-shader-cache`** (mới).
- **DoD:** ≥ 200 test safety xanh (kể cả property test); `nfd clean --dry-run` chạy trên toàn hệ thống VM sạch không đề xuất mục nào trong Protected; mọi thao tác có Restore (Recycle Bin / Quarantine); EN + VI đủ; `gui-smoke.mjs` phủ Clean + Restore.

## 2. Thành phần

### 2.1 `nfd-core::safety` → `SafetyGate`

- Tổng quát hóa `check_manual_delete` thành `SafetyGate::check(&CleanPlan) -> GateReport` (mỗi mục: `Allowed` / `Warn(reason)` / `Blocked(reason)`); `check_manual_delete` giữ lại làm lớp bọc để Analyze không đổi hành vi.
- Bổ sung so với hiện tại (§3.4): canonicalize (`GetFinalPathNameByHandleW`, 8.3 → tên dài, `\\?\`, trailing dot/space, ADS `:stream`), reparse trỏ sang volume khác, Authenticode Microsoft trong thư mục hệ thống, thuộc tính SYSTEM dưới `%SystemRoot%` / Program Files.
- **Restart Manager** (`nfd-platform::locks`: `RmStartSession / RmRegisterResources / RmGetList`) → mục đang bị khóa hiện tên tiến trình; app không bao giờ đóng tiến trình (người dùng có thể dùng tab Tasks).
- Kiểm tra lại trên đĩa ngay trước khi thực thi (giống Recycle hiện nay): mục đổi kích thước/thời gian/loại kể từ lúc plan → bỏ qua, báo lại.

### 2.2 Rule engine (`nfd-rules`)

- `Matcher`: mở rộng biến (`${TEMP}`…, đã có `expand`), glob `**`/`*`/`{a,b}` (`globset`), `min_age_days`, `requires_closed` (ánh xạ tên exe → tiến trình đang chạy → mục `Warn`), `exclude`, `level`. Walk dùng scanner sẵn có (không follow link, không vượt volume).
- Đầu ra `Candidate { rule_id, path, bytes, files, newest_mtime, level }`, tổng theo rule / pack / level.
- Pack mới: `rules/browsers.toml` (Chrome/Edge/Brave/Opera/Vivaldi/Firefox: Cache, Code Cache, GPUCache, Service Worker CacheStorage — **không** đụng Cookies, History, Login Data, Bookmarks, Local Storage, IndexedDB), `rules/gpu-shader-cache.toml` (NVIDIA DXCache/GLCache, AMD DxCache/VkCache, Intel ShaderCache, `D3DSCache`). Mỗi rule có `impact` EN + VI, `verified_on`; `nfd rules validate` + test cho từng pack.

### 2.3 Planner / Executor / Quarantine

- `Planner::build(candidates | manual items, mode) -> CleanPlan` — gộp mục lồng nhau, tính tổng "sẽ giải phóng", mặc định chỉ chọn 🟢 Safe; 🟡 Caution cần tick riêng; 🟠 Advanced chỉ hiện ở Expert; 🔴 Protected chỉ có *Hướng dẫn*.
- `Executor::run(plan, dry_run)` — lần đầu mặc định dry-run; chế độ:
  - **Recycle Bin** (`nfd-platform::trash`, đã có) cho mục người dùng chọn tay;
  - **Quarantine** cho thư mục cache lớn: `rename` cùng volume vào `<volume>\.nfd-quarantine\<op-id>\` (ẩn, ACL chỉ user) + `manifest.json`; khác volume → Recycle Bin; TTL 7 ngày, tự purge khi app chạy, nút *Restore*;
  - **Xóa vĩnh viễn**: chỉ Safe + xác nhận 2 lớp + gõ `DELETE` (Expert).
  - Tiến trình theo mục, hủy được, lỗi từng mục không dừng cả plan.
- **UndoJournal** trong `nfd-history` (cùng SQLite): bảng `operations`, `operation_items` (đường dẫn gốc, đích quarantine, kích thước, kết quả); `restore(op_id)`; audit log `%LOCALAPPDATA%\…\logs\audit-*.log` giữ 90 ngày.

### 2.4 CLI

- `nfd clean [--pack windows-system,browsers] [--level safe|caution] [--dry-run (mặc định)] [--apply] [--json]`
- `nfd quarantine list | restore <id> | purge [--older-than 7d]`
- `nfd plan explain <path>` — vì sao một đường dẫn được phép / bị chặn (dùng cho test và hỗ trợ).

### 2.5 GUI

- View **Clean**: nhóm theo pack → rule, mỗi dòng: kích thước, số tệp, mức rủi ro, "Nếu xóa thì…", app cần đóng (đang chạy hay không); nút lớn **"Dọn an toàn (chỉ 🟢)"**; xem trước danh sách tệp; dialog xác nhận có tổng GB.
- Home: nút G1 "Giải phóng *X* GB an toàn ngay" dùng kết quả quét rule; Health Score thêm `HealthContribution` junk.
- **Activity log (G6)**: mọi thao tác theo ngày, nút Restore.
- Tidy up: thêm hành động cho thư mục rỗng / shortcut hỏng (qua Quarantine); Old & Large và Analyze chuyển sang Planner.

## 3. Kiểm thử

- `tests/safety_gate.rs`: ≥ 200 case (junction vào `C:\Windows`, symlink ra ổ khác, `..\..`, `PROGRA~1`, UNC, `\\?\`, khác hoa/thường, trailing dot/space, ADS, mount point, OneDrive placeholder).
- `proptest`: mọi đường dẫn sinh dưới Protected root → bị chặn; mọi plan → không có hai mục lồng nhau.
- Test tích hợp trên thư mục tạm: plan → quarantine → restore khôi phục đúng byte/thời gian; tệp bị khóa → bỏ qua có lý do.
- `cargo-fuzz` cho rule loader. CI `windows-latest`: `nfd clean --level safe --dry-run --json` toàn hệ thống, assert không có mục Protected.

## 4. Lát cắt

1. SafetyGate + canonicalize + Restart Manager + 200 test.
2. Matcher + pack `browsers`, `gpu-shader-cache` + `nfd clean --dry-run`.
3. Planner / Executor / Quarantine / UndoJournal + `nfd clean --apply`, `nfd quarantine`.
4. View Clean + Activity log + nút Home + Health junk; chuyển Analyze/Old & Large/Tidy up sang Planner.

## 5. Ngoài phạm vi Phase 3

DevCache UI (Phase 4), Duplicates (Phase 5), System Restore Point tự động (cần helper elevated, làm cùng helper của Hardware Monitor). Dọn system junk cần admin: làm sau, xem mục 8.

## 6. Rủi ro

- Quarantine trên ổ FAT/exFAT/mạng → luôn Recycle Bin.
- Trình duyệt đang chạy giữ khóa cache → mục `Warn`, đề nghị đóng; không bao giờ xóa profile.
- Glob quá rộng trong rule → `nfd rules validate` chặn rule khớp Protected template (đã có) + giới hạn độ sâu.

## 7. Chạy song song với Hardware Monitor — phân chia file

| Phase 3 Clean | Hardware Monitor |
|---|---|
| `crates/nfd-core` (safety, planner), `crates/nfd-rules`, `crates/nfd-history` (journal), `nfd-platform::locks` | crate mới `nfd-hwinfo`, `nfd-sensors`, `nfd-tasks`; `nfd-platform` chỉ thêm module mới (`process.rs`, `firmware.rs`) |
| `src-tauri/src/commands/clean.rs`, `src/lib/views/Clean*.svelte` | `src-tauri/src/commands/hwmon.rs`, `src/lib/views/Hardware*.svelte` |
| `locales/{en,vi}/clean.ftl` | `locales/{en,vi}/hwmon.ftl` |
| `apps/nfd-cli/src/clean.rs` | `apps/nfd-cli/src/hw.rs` |

File dùng chung (chỉ thêm dòng đăng ký, merge tay khi gộp): `Cargo.toml` workspace members, `src-tauri/src/lib.rs` (`invoke_handler`), `App.svelte` / sidebar, loader i18n + `scripts/check-locales.mjs` (hỗ trợ nhiều tệp `.ftl` — phase nào làm trước thì làm), `CHANGELOG.md`, `README.md`, `docs/PLAN.md`. Mỗi bên làm trên nhánh / worktree riêng, bắt đầu **sau khi** session Phase 2 commit xong.

## 8. Những gì còn lại sau khi dọn, và lần dọn với quyền quản trị (2026-11)

Người dùng báo một số rác "không bao giờ hết": tệp đang bị chương trình giữ, và tệp chỉ quản trị viên mới xóa được (`%SystemRoot%\Temp`, nhật ký CBS, dump của kernel, bộ đệm tải về của Windows Update).

**Kết quả "còn gì và vì sao".** Executor phân biệt lý do: lỗi chia sẻ / khóa (32, 33) thành `in-use`, bị từ chối truy cập thành `permissiondenied` (cả khi di chuyển từng tệp của một thư mục; thư mục con không đọc được không còn làm hỏng cả mục mà chỉ được để lại). `exec::group_leftovers` gom các mục còn lại theo nguyên nhân: **đang được dùng** (tên chương trình từ Restart Manager, hỏi lại sau khi dọn), **cần quyền quản trị** (bị từ chối và quy tắc có `admin = true`), **khác** (SafetyGate chặn, đã đổi từ lúc quét, lỗi khác). Mỗi nhóm có lời giải thích và một hành động trong view Dọn dẹp.

**"Đóng rồi thử lại"** (`clean_retry`). Backend giữ lần dọn cuối; nút chỉ gửi nguyên nhân, không gửi đường dẫn. Các quy tắc liên quan được quét lại, chỉ ứng viên mới nằm tại hoặc bên dưới một mục còn lại (cùng quy tắc) được đưa vào Planner, rồi SafetyGate và Executor như mọi lần dọn. Ứng dụng không bao giờ tự đóng chương trình.

**Lần dọn với quyền quản trị** (`nanofreedisk.exe --elevated-clean <request.json> <response.json>`, theo mẫu `--elevated-leftovers`). Chỉ chạy sau khi người dùng bấm nút ghi rõ cần quyền quản trị và xác nhận trong dialog; một lời nhắc UAC cho cả lô; không có tiến trình elevated thường trú; `NFD_NO_AUTO_ELEVATE` tắt hẳn. Ranh giới tin cậy:

- Tệp request nằm trong thư mục dữ liệu của app, chương trình nào của người dùng cũng ghi được, nên là **dữ liệu không tin cậy**: nó chỉ có thể thu hẹp những gì tiến trình elevated tự quyết dọn, không bao giờ mở rộng. Dòng lệnh phải đúng dạng `[exe, cờ, req, resp]`, hai tệp `.json` tuyệt đối cùng thư mục; request tối đa 8 MiB, không qua link, trường lạ bị từ chối (`deny_unknown_fields`), mã thao tác đúng dạng `YYYYMMDD-HHMMSS-xxxx`.
- Tiến trình elevated bật **Redirection Guard** (`ProcessRedirectionTrustPolicy`: không đi theo junction do người không phải quản trị tạo), tự nạp **gói quy tắc nhúng trong binary** và lấy các thư mục máy (`SYSTEMROOT`, `SYSTEMDRIVE`, `PROGRAMDATA`, `PROGRAMFILES*`) từ `SHGetKnownFolderPath`, không từ biến môi trường (biến người dùng như `SystemRoot` trong `HKCU\Environment` là đường vòng UAC kinh điển).
- Mỗi mục bị từ chối nếu: không phải đường dẫn ổ đĩa tuyệt đối thông thường (`relative-path`: tương đối, `.`/`..`, `\\?\`, UNC, thiết bị, tên kết thúc bằng dấu chấm/khoảng trắng); quy tắc không có (`unknown-rule`), không có `admin = true` (`not-admin-rule`), là Được bảo vệ (`protected-level`) hoặc Nâng cao khi không ở Expert (`expert-only`); quy tắc đó không khớp đường dẫn (`not-matched`); một quy tắc Giữ lại bao gồm nó (`kept`); SafetyGate từ vựng chặn.
- Các quy tắc được chấp nhận được **quét lại ngay trong tiến trình elevated**; chỉ ứng viên mới tại hoặc dưới một mục được yêu cầu mới được dọn (`not-found-now` cho phần còn lại). Bản quét không bao giờ đi theo hay đề xuất link; kích thước, thời gian lấy từ bản quét này, không từ request.
- Executor kiểm tra lại từng mục ngay trước khi làm (SafetyGate trên đĩa: còn đó, không phải link, đường dẫn cuối được phép, cùng volume, không phải tệp SYSTEM, không phải tệp chương trình dưới `%SystemRoot%`, không đổi), cộng thêm: đường dẫn cuối phải **chính là** đường dẫn đó (`path-changed` nếu link hay tên ngắn ở thư mục cha dẫn đi nơi khác).
- Quy tắc `admin` chỉ được dùng biến máy (`nfd rules validate` báo lỗi nếu có `${HOME}`, `${TEMP}`…): với tài khoản quản trị khác (người dùng chuẩn nhập mật khẩu admin), tiến trình elevated không bao giờ đụng hồ sơ của ai.

**Mục bị xóa đi đâu.** Vào **Thùng rác** (khôi phục bằng Explorer, Windows hỏi quyền quản trị). Không dùng khu cách ly: nó nằm trong hồ sơ người dùng mà chương trình của người dùng ghi được, nên một tiến trình quản trị chuyển tệp vào đó có thể bị chuyển hướng, và app không elevated cũng không dọn được những tệp đó khi hết hạn. Xóa vĩnh viễn chỉ khi lần dọn gốc chọn xóa vĩnh viễn (Expert) và quy tắc là An toàn có `regenerates` (bộ đệm, tệp tạm thuần túy); dialog xác nhận nói rõ điều này. Kết quả ghi vào nhật ký hoàn tác và Hoạt động với kind `clean-admin`; mục bị từ chối hiện là "bỏ qua" kèm mã.

**"Xóa khi khởi động lại" (`MoveFileExW` + `MOVEFILE_DELAY_UNTIL_REBOOT`): không làm.** Đường dẫn được ghi vào `PendingFileRenameOperations` và Session Manager xóa nó lúc khởi động với quyền SYSTEM, theo đường dẫn lúc đó, không kiểm tra gì. Tệp còn lại nằm trong thư mục người dùng (hoặc `C:\Windows\Temp`, nơi Users được tạo thư mục) có thể bị đổi thư mục cha thành junction trước khi khởi động lại, biến lệnh xóa thành "xóa tệp tùy ý với quyền SYSTEM", một lỗ hổng leo thang quyền đã biết (cùng họ với CVE-2025-21420 của Disk Cleanup). Không có cách kiểm tra lại lúc khởi động, nên phương án an toàn là: đóng chương trình rồi thử lại, hoặc khởi động lại máy rồi dọn.

**Kho của Windows** (WinSxS, Windows.old, điểm khôi phục, pagefile/hiberfil, Delivery Optimization) vẫn là mục Được bảo vệ, chỉ hướng dẫn: không bao giờ chiếm quyền sở hữu hay xóa trực tiếp. Trường `tool` của quy tắc (danh sách đóng, loader từ chối giá trị lạ, không bao giờ là dòng lệnh) cho nút "Mở công cụ của Windows": `disk-cleanup` (`cleanmgr.exe`, "Clean up system files" có Windows Update Cleanup cho kho thành phần, với tiến trình riêng của nó), `storage-settings` (`ms-settings:storagesense`), `system-protection` (`SystemPropertiesProtection.exe`), `performance-options` (`SystemPropertiesPerformance.exe`). Chương trình lấy từ `System32` của thư mục Windows mà Windows báo, không qua `PATH`. Chưa chạy `Dism /StartComponentCleanup` từ app: Disk Cleanup làm cùng việc và tự hiện tiến trình.
