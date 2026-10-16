# Phase 5 — Duplicates: kế hoạch chi tiết

> Trạng thái: **kế hoạch**, chưa code (2026-10-16). Bản chi tiết của hàng "5 – Duplicates" trong `docs/PLAN.md` §8, theo thiết kế §4.4, lệnh `nfd dupes` ở §4.7, mục A7 ① "Find duplicates here" (§13.2). Mọi thao tác xóa / thay thế đi qua đường Phase 3: `Planner → SafetyGate → Executor → UndoJournal` (`docs/PHASE3_CLEAN.md`).

## 1. Mục tiêu & DoD

- Tìm **tệp trùng nội dung** (byte-for-byte) trên một hoặc nhiều thư mục / ổ, nhanh, không bao giờ báo nhầm, và xử lý an toàn: Recycle Bin, Quarantine, hoặc **thay bằng hardlink** (cùng ổ NTFS) / **block clone** (ReFS, Dev Drive).
- **DoD:**
  - 100 GB ảnh/video hỗn hợp trên SSD: lần đầu < 5 phút; quét lại (cache hash) < 30 s.
  - **0 báo nhầm**: một nhóm chỉ hình thành khi full hash BLAKE3 trùng, và nội dung được kiểm tra lại ngay trước khi hành động.
  - Bất biến "mỗi nhóm luôn giữ ít nhất 1 bản" được kiểm bằng `proptest` ở Planner, không chỉ ở UI.
  - Không bao giờ đọc nội dung cloud placeholder (OneDrive "chỉ online" → đọc sẽ kéo tệp về máy), có test.
  - Mọi thao tác có Restore (Recycle Bin / Quarantine 7 ngày); EN + VI (`locales/{en,vi}/dupes.ftl`); `gui-smoke.mjs` phủ view Duplicates bằng fixture riêng.

## 2. Thành phần

### 2.1 Crate mới `nfd-dupes` (feature crate theo §13.5)

Phụ thuộc `nfd-core` (scanner, `ScanTree`, plan), `nfd-platform`, `nfd-history` (cache hash). Không có đường xóa riêng — chỉ sinh `PlanItem`.

**Pipeline** (mỗi bước thu hẹp tập ứng viên, có `Progress { phase, done, total, bytes }` + cờ hủy):

1. **Tập tệp**: dùng `ScanTree` có sẵn (từ Analyze hoặc quét mới bằng scanner — không follow link, không vượt volume). Loại ngay:
   - tệp < ngưỡng (mặc định 1 MB, Expert chỉnh được; tệp rỗng không bao giờ là "trùng");
   - reparse point, **cloud placeholder** (`OFFLINE`, `RECALL_ON_OPEN`, `RECALL_ON_DATA_ACCESS`), tệp có thuộc tính SYSTEM;
   - đường dẫn bị `SafetyGate` coi là Protected (Windows, Program Files, `$Recycle.Bin`, credential…), kể cả thư mục lưu hội thoại của các AI agent (§5.1);
   - mặc định bỏ qua **thư mục ứng dụng** (vì app cần đủ tệp của nó): `AppData\Local\Programs`, `node_modules`, `.git`, `.venv`, `site-packages`, thư viện game (Steam `steamapps`, Epic, GOG…), `WindowsApps`, `.nfd-quarantine`. Có công tắc Expert "Gồm cả thư mục ứng dụng" — khi bật, mục trong đó chỉ được *xem*, không chọn sẵn.
2. **Gom theo (volume, kích thước logic)**; bỏ nhóm 1 phần tử. Các đường dẫn cùng `file_id` (hardlink sẵn có) gộp làm **một** bản — không bao giờ coi là trùng của chính nó.
3. **Hash từng phần**: BLAKE3 của 16 KB đầu + 16 KB giữa + 16 KB cuối (tệp ≤ 48 KB → hash toàn bộ luôn) → tách nhóm.
4. **Hash toàn bộ**: BLAKE3 đọc bằng buffer 1 MB (không `mmap`: tệp bị cắt ngắn khi đang map gây crash), song song theo ổ: SSD/NVMe N luồng, HDD 1 luồng đọc tuần tự (dùng `StorageDeviceSeekPenaltyProperty` như `nfd-hwinfo`).
5. **Paranoid** (tùy chọn): so sánh từng byte trong nhóm.

**Cache hash** trong `nfd-history` (SQLite cùng DB): khóa `(volume serial, file_id, size, mtime)` → `partial`, `full`. Sai khác bất kỳ trường nào → hash lại. Giới hạn kích thước cache, xóa cùng "Xóa lịch sử" trong Settings.

**Kết quả** `DupeReport { groups: Vec<DupeGroup>, wasted_bytes, scanned_files, skipped: Vec<(path, reason)> }`; `DupeGroup { hash, size, copies: Vec<Copy { path, mtime, file_id, volume, flags }> }`; `flags` gồm: trong OneDrive/Dropbox/Google Drive (đã đồng bộ), trong git repo, trong thư mục ứng dụng, chỉ đọc. Sắp xếp theo `wasted = size × (số bản − 1)`.

### 2.2 Chọn bản giữ & Planner

- **Chiến lược tự chọn** (chọn bản *giữ*, phần còn lại đánh dấu): mới nhất, cũ nhất, đường dẫn ngắn nhất, trong thư mục ưu tiên (danh sách người dùng đặt), *không* nằm trong Downloads / Temp / Desktop. Mặc định: giữ bản ngoài Downloads/Temp, rồi bản cũ nhất (thường là bản gốc).
- `dupes::plan(groups, selection, action) -> CleanPlan` — `Origin::Duplicate { group, keeper }`; Planner từ chối plan nếu một nhóm bị chọn hết; nhóm mà mọi bản đều không được phép xóa → chỉ xem.
- **Kiểm tra lại ngay trước khi hành động** (bắt buộc, không tắt được): bản giữ và bản xóa vẫn tồn tại, cùng size/mtime như lúc quét, **hash lại toàn bộ cả hai** (hoặc so byte) — khác → bỏ qua mục đó, báo lại. Bản giữ bị xóa/đổi giữa chừng → cả nhóm bị bỏ qua.
- Cảnh báo riêng (🟡, xác nhận thêm): bản xóa nằm trong thư mục đồng bộ đám mây (xóa ở đây = xóa trên cloud và mọi máy khác), trong git repo (tệp được theo dõi), chỉ đọc.

### 2.3 Hành động

| Hành động | Khi nào | Cách làm | Undo |
|---|---|---|---|
| Recycle Bin | mặc định | `nfd-platform::trash` như Analyze | Recycle Bin / Activity log |
| Quarantine | Expert, hoặc khi Recycle Bin quá chậm (nhiều tệp) | `exec` có sẵn | Restore 7 ngày |
| **Thay bằng hardlink** | cùng volume NTFS, bản giữ < 1023 link | tạo hardlink tới bản giữ ở tên tạm cùng thư mục → chuyển bản trùng vào Quarantine → đổi tên link vào chỗ cũ; lỗi bất kỳ bước nào → hoàn tác các bước trước | Restore = xóa link + trả bản gốc từ Quarantine (7 ngày); sau đó link vẫn giữ nguyên dữ liệu |
| **Block clone** | ReFS / Dev Drive | `FSCTL_DUPLICATE_EXTENTS_TO_FILE` vào tệp tạm → thay chỗ như trên | như hardlink |

Hardlink có rủi ro thật, nên UI nói rõ trong "Nếu bấm thì…": các đường dẫn **dùng chung một nội dung** — ứng dụng sửa tệp tại chỗ sẽ đổi nội dung ở mọi đường dẫn; thuộc tính, quyền và thời gian cũng dùng chung. Vì vậy hardlink chỉ được **chọn sẵn** cho loại tệp ít bị sửa (ảnh, video, nhạc, bộ cài, ảnh đĩa, archive, model AI `.gguf/.safetensors`), loại khác phải tick tay; không bao giờ đề xuất trong thư mục đồng bộ đám mây hay git repo. Block clone là copy-on-write nên không có rủi ro này.

### 2.4 Thư mục trùng

Sau khi có full hash: hash của thư mục = hash có thứ tự của (tên, hash/kích thước) các con → thư mục giống hệt nhau gom thành nhóm, hiện ở cấp **cao nhất** (không liệt kê cả thư mục con trùng). Chỉ tính khi mọi tệp con đều đã được hash (thư mục có tệp bị loại ở bước 1 → không tham gia). Hành động: Recycle Bin / Quarantine cả thư mục, cùng quy tắc giữ 1 bản.

### 2.5 CLI

```
nfd dupes <path...> [--min-size 1MB] [--include-app-dirs] [--paranoid] [--threads N]
                    [--json|--csv] [--keep oldest|newest|shortest|prefer:<dir>]
                    [--action list|recycle|quarantine|hardlink] [--apply]   # mặc định list, dry-run
nfd dupes --folders <path...>
```

JSON schema `nanofreedisk.dupes.v1` (`docs/schema/dupes-v1.schema.json`). Không có `--apply` → chỉ in plan.

### 2.6 GUI

- View **Duplicates** (sidebar, Ctrl+K): chọn phạm vi (ổ / thư mục / "dùng lần quét hiện tại"), ngưỡng kích thước, loại tệp; tiến trình theo pha + hủy; tổng "lãng phí".
- Tab **Tệp** / **Thư mục**: danh sách nhóm ảo hóa (theo dung lượng lãng phí), mở nhóm thấy từng bản (đường dẫn, thư mục, mtime, cờ ☁/git/app), nút radio "giữ bản này", dropdown chiến lược tự chọn, *Mở trong Explorer*; xem trước ảnh nhỏ cho ảnh (đọc qua lệnh backend có giới hạn kích thước, không mở tệp bằng app ngoài).
- Thanh hành động: Recycle Bin / Thay bằng hardlink / (Expert) Quarantine; dialog xác nhận có tổng GB, số tệp, cảnh báo §2.2; xuất CSV/JSON.
- Từ Analyze: menu chuột phải *Tìm tệp trùng ở đây*; Explorer: thêm verb **"Find duplicates here"** vào `nfd-platform::shell_menu` (HKCU, gỡ sạch như verb hiện có).
- Expert hiện lệnh `nfd dupes …` tương đương. Health Score: mục thông tin "≈ X GB trùng lặp" (chỉ khi đã quét).

## 3. Kiểm thử

- Unit: gom nhóm theo size, tách bằng partial/full hash, hardlink cùng `file_id` không thành trùng, tệp rỗng/nhỏ bị loại, tệp chỉ khác byte cuối / byte giữa không bị gom, thư mục trùng ở cấp cao nhất.
- `proptest`: với mọi lựa chọn → plan không bao giờ xóa hết một nhóm; không có mục nào là bản giữ.
- Tích hợp (thư mục tạm): đổi nội dung bản giữ sau khi quét → mục bị bỏ qua; hardlink: thay → restore khôi phục đúng byte, thời gian, và hai tệp lại độc lập; lỗi giữa chừng (giả lập) → không mất tệp nào.
- Cloud placeholder: tệp mang thuộc tính `RECALL_ON_DATA_ACCESS` (giả lập qua attribute trên fixture / mock `fsinfo`) không bao giờ được mở để đọc.
- Benchmark `benches/dupes.rs` + đo thật 100 GB trên máy dev (ghi kết quả vào `docs/dupes-benchmark.md`).

## 4. Lát cắt

1. **Pipeline lõi** — `nfd-dupes` (bước 1–5), cache hash, `nfd dupes --json/--csv`, schema, benchmark.
2. **Chọn & xóa** — chiến lược giữ, `dupes::plan`, kiểm tra lại trước khi hành động, Recycle Bin / Quarantine, `--action recycle|quarantine --apply`, proptest bất biến.
3. **GUI** — view Duplicates, xem trước ảnh, xuất, menu Analyze + verb Explorer "Find duplicates here", Health info, `gui-smoke`.
4. **Hardlink & block clone** — `nfd-platform::link` (`CreateHardLinkW`, `FSCTL_DUPLICATE_EXTENTS_TO_FILE`), undo qua Quarantine, `--action hardlink`, UI cảnh báo §2.3.
5. **Thư mục trùng** — §2.4, tab Thư mục, `nfd dupes --folders`.

## 5. Ngoài phạm vi Phase 5

- Ảnh/video **gần giống** (perceptual hash, cặp RAW+JPEG — D5, v1.1).
- Hardlink `node_modules` giữa các project (B9, v2).
- Trùng **khác ổ** chỉ được liệt kê và xóa, không hardlink/clone (không thể qua volume).

## 6. Rủi ro

- Đọc toàn bộ 100 GB làm nóng ổ / chậm máy → giới hạn luồng theo loại ổ, nút Tạm dừng, ưu tiên I/O thấp (`FILE_FLAG_SEQUENTIAL_SCAN`, thread background mode).
- Antivirus quét mỗi tệp bị mở → chậm hơn đo đạc; benchmark ghi rõ có/không Defender real-time.
- Người dùng xóa bản trong thư mục đồng bộ → mất ở mọi máy: cờ ☁ + cảnh báo §2.2, không bao giờ chọn sẵn.
- Hardlink bị app sửa tại chỗ: chỉ chọn sẵn cho loại tệp ít sửa, giải thích rõ, ưu tiên block clone khi có ReFS.
