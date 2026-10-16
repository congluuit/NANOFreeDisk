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

DevCache UI (Phase 4), Duplicates (Phase 5), System Restore Point tự động (cần helper elevated — làm cùng helper của Hardware Monitor), dọn system junk cần admin.

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
