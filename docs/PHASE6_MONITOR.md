# Phase 6 — Monitor (Disk Activity): kế hoạch chi tiết

> Trạng thái: **xong** (2026-10-16) — lát 1 ✅ (lõi `nfd-monitor`, Counters, PDH, `nfd monitor`), lát 2 ✅ (ETW, `--source`, `--watch`, job CI `etw` trên runner admin), lát 3 ✅ (helper protocol 2 dùng chung với cảm biến, backend app, lịch sử 24 h, cảnh báo log spam), lát 4 ✅ (view Hoạt động ổ đĩa gồm cả tab F7, Settings, `gui-smoke`), lát 5 ✅ (F7 tạo tệp theo completion, tray Tạm dừng), lát 6 ✅ (overhead: Counters 0,034 % trên laptop tham chiếu, ETW 0,38 % trên runner CI — `docs/monitor-overhead.md`). Bản chi tiết của hàng "6 – Monitor" trong `docs/PLAN.md` §8, theo thiết kế §4.5, mục C8 ① *Log-spam detector*, F7 ② *"Who touched this?"* (lát tùy chọn) và G5 ① *Tray "pause monitor"* (§13.2). Monitor **chỉ đọc**: không bao giờ kết thúc tiến trình, không đổi gì trên máy; "Kết thúc tác vụ" chỉ đi qua luồng có bảo vệ đã có của Hardware → Tasks (`nfd_tasks::end_task`, dialog `EndTaskDialog`).

## 1. Mục tiêu & DoD

- Trả lời nhanh "**cái gì đang đọc/ghi ổ của tôi?**": bảng tiến trình (đọc B/s, ghi B/s, IOPS, tệp đang thao tác), biểu đồ realtime 60 s / 10 phút, thông số từng ổ vật lý (throughput, hàng đợi, độ trễ, % rảnh), top tệp đang bị ghi, nút *Mở thư mục* / *Phân tích thư mục*.
- **Cảnh báo log spam (C8)**: một tiến trình ghi liên tục > X MB/s trong Y phút (mặc định 5 MB/s × 5 phút, tắt mặc định) → thông báo hệ thống + toast; tệp log > 1 GB đang bị ghi → mục cảnh báo trong view.
- **Lịch sử 24 h** trong SQLite (`nfd-history`), downsample theo phút: tổng đọc/ghi, đỉnh B/s, top tiến trình ghi/đọc.
- **DoD:**
  - Overhead của Monitor **< 2 % CPU** (tổng mọi luồng logic, cách Task Manager tính) khi đang lấy mẫu mỗi giây, đo bằng tải nhẹ (sao chép / ghi vài trăm MB, không stress test) trên laptop tham chiếu i9-13900HX; ghi kết quả vào `docs/monitor-overhead.md`.
  - Không quyền admin vẫn chạy (nguồn *Counters*); có quyền (helper elevated hoặc app/CLI đang chạy admin) dùng ETW để có tệp cụ thể.
  - Monitor không có đường nào kill/suspend/đổi priority tiến trình (test: không gọi API nào trong `nfd-monitor`; End task chỉ qua `nfd_tasks::end_task`).
  - Phiên ETW luôn được dừng: khi tắt Monitor, khi app/helper thoát, và phiên "mồ côi" (app crash) bị dừng ở lần chạy sau (phiên kernel sống tiếp sau khi tiến trình chết).
  - EN + VI (`locales/{en,vi}/monitor.ftl`), mọi màu là theme token; `gui-smoke.mjs` phủ view Monitor.

## 2. Thành phần

### 2.1 Crate mới `nfd-monitor` (§3.1)

Phụ thuộc `nfd-tasks` (danh sách tiến trình một lần gọi kernel) và `nfd-sensors::win::pdh` (PDH). Không phụ thuộc `nfd-history` (app ghi lịch sử, CLI đọc) và không có API nào thay đổi trạng thái máy.

**Ba nguồn dữ liệu** (`Source`), chọn theo thứ tự:

| Nguồn | Khi nào | Có gì |
|---|---|---|
| `Etw` | app/CLI đang chạy admin | như `Helper`, trong tiến trình |
| `Helper` | người dùng bấm "Bật chi tiết (cần admin)" → UAC → helper elevated (dùng chung helper với Advanced sensors: một tiến trình, một lần UAC) | đọc/ghi **theo tệp** của từng tiến trình, tạo / xóa / đổi tên tệp, top tệp bị ghi |
| `Counters` | mặc định, không cần admin | `IO_COUNTERS` từng tiến trình (delta/giây: byte + số thao tác đọc/ghi), không có tệp |

Thông số **ổ vật lý** luôn lấy từ PDH (`\PhysicalDisk(*)\Disk Read Bytes/sec`, `Disk Write Bytes/sec`, `Disk Transfers/sec`, `Avg. Disk Queue Length`, `Avg. Disk sec/Transfer`, `% Idle Time`) ở cả ba nguồn.

Lưu ý ngữ nghĩa (UI nói rõ): `Counters` đếm **mọi I/O** của tiến trình (cả mạng, pipe); ETW đếm **I/O tệp** trên volume cục bộ (lọc `\Device\HarddiskVolume*`, bỏ pipe/mạng). Cả hai là I/O logic của tiến trình — ghi vào cache rồi được Windows xả xuống ổ sau; số byte *vật lý* của ổ xem ở phần Ổ đĩa. Đây là lý do không dùng `DiskIo` cho bảng tiến trình: ghi qua cache được lazy writer xả trong ngữ cảnh tiến trình `System`, sẽ giấu mất thủ phạm log spam.

**ETW** (`win::etw`):
- Phiên **SystemTraceProvider riêng** (`EVENT_TRACE_SYSTEM_LOGGER_MODE`, Windows 8+), tên cố định `NANOFreeDisk Disk Monitor` — không chiếm `NT Kernel Logger` (Process Monitor, WPR… vẫn dùng được). Real-time, buffer 64 KB, flush 1 s.
- Cờ: `FILE_IO_INIT` (Create / Read / Write / Cleanup / Close / SetInfo / Delete / Rename / *Path), `DISK_FILE_IO` (sự kiện Name: FileKey → tên tệp), `THREAD` (map TID → PID khi header không có PID).
- Parser thuần (`etw_parse`, chạy được trên mọi OS, test bằng bản ghi tổng hợp): layout `FileIo` v2 và v3 (64-bit), `Thread` v3+; phiên bản lạ → bỏ qua và đếm. Callback chỉ cập nhật hash map (không cấp phát với Read/Write), map FileObject/FileKey → đường dẫn có giới hạn (200k mục).
- Đường dẫn thiết bị → ký tự ổ qua `QueryDosDeviceW`.
- Dừng: `ControlTraceW(STOP)` khi drop; trước khi bắt đầu, dừng phiên cùng tên còn sót. Đếm sự kiện mất (`EventsLost`) và báo trong UI.
- **Lưới an toàn**: nếu ETW không giao sự kiện tệp nào trong 10 s trong khi PDH thấy ổ có ghi → báo `etw-silent` và quay về `Counters`.

**Lõi thuần (test được, không phụ thuộc OS):**
- `Aggregator`: nhận `IoEvent { pid, op, bytes, file }` → cộng dồn theo tiến trình và theo tệp trong một tick.
- `Monitor::tick(raw, disks, now)`: ghép delta với danh sách tiến trình (tên, đường dẫn, `create_time` — định danh tiến trình là PID + thời điểm tạo, PID tái sử dụng không bị cộng nhầm) → `MonitorSnapshot`.
- `Chart`: vòng 600 điểm × 1 s (đọc/ghi tổng) → UI cắt 60 s hoặc 10 phút.
- `Minute`: downsample theo phút (tổng byte, đỉnh B/s, top 5 tiến trình ghi và đọc).
- `SpamWatch` (C8): theo tiến trình (PID + create time), cửa sổ trượt; báo một lần cho mỗi đợt, báo lại sau khi đã dưới ngưỡng ≥ 1 phút; `LogFileWatch`: tệp `.log/.txt/.etl/.jsonl/.out` > 1 GB đang bị ghi (kích thước đọc bằng metadata, cache 60 s).

### 2.2 Helper elevated (mở rộng `nfd-helper`)

- Protocol **2**: thêm `MonitorStart { protocol, watch: Option<String> }`, `MonitorPoll` → `Response::Monitor { tick }`, `MonitorStop`. Không có request nào chạy lệnh, mở tệp để đọc nội dung hay kết thúc tiến trình; `watch` chỉ là tiền tố đường dẫn để lọc sự kiện.
- **Dùng chung một kết nối**: `nfd_helper::start()` trả về `Connection` (pipe trong `Arc<Mutex<…>>`); `RemoteProvider` (cảm biến) và `RemoteMonitor` cùng dùng. App giữ một `Helper` chung: bật Advanced sensors rồi bật Monitor chi tiết (hoặc ngược lại) không hỏi UAC lần hai.
- Helper dừng phiên ETW khi pipe đóng / app thoát (`Drop`).

### 2.3 Lịch sử 24 h (`nfd-history`)

Bảng `monitor_minutes(ts PRIMARY KEY, read, write, peak_read, peak_write, top TEXT)` (`top` = JSON top tiến trình), `CREATE TABLE IF NOT EXISTS` (không tăng schema, như `file_hashes`), xóa dòng > 24 h mỗi lần ghi, xóa cùng "Xóa lịch sử". Chỉ ghi khi Monitor đang chạy (view mở hoặc cảnh báo bật).

### 2.4 CLI

```
nfd monitor [--seconds N] [--interval 1] [--top 10] [--json] [--source auto|etw|counters]
            [--watch <dir>]                  # F7: ai tạo/ghi/xóa/đổi tên trong thư mục (cần ETW)
nfd monitor --history [--json] [--db <path>] # 24 h đã lưu (phút)
```

JSON Lines, mỗi dòng một snapshot, schema `nanofreedisk.monitor.v1` (`docs/schema/monitor-v1.schema.json`). `--source etw` khi không có quyền admin → lỗi rõ ràng (CLI không tự gọi UAC).

### 2.5 GUI

- View **Monitor** (sidebar thay ô "P6", Ctrl+K): thanh nguồn (Counters / ETW / Helper + nút "Bật chi tiết theo tệp (cần admin)"), nút **Tạm dừng / Tiếp tục**, số sự kiện mất (Expert).
- **Biểu đồ** realtime đọc/ghi tổng (canvas, màu token `chart-read` / `chart-write`), chuyển 60 s / 10 phút; thẻ từng ổ vật lý.
- **Bảng tiến trình**: tên, PID, đọc/s, ghi/s, IOPS, số tệp (ETW), sắp xếp; mở dòng → tệp của tiến trình đó (đường dẫn, đọc/s, ghi/s) + *Mở thư mục* (Explorer) / *Phân tích thư mục* (Analyze) / *Kết thúc tác vụ…* (mở `EndTaskDialog` có sẵn — verdict + xác nhận + chặn cứng tiến trình hệ thống).
- **Top tệp đang bị ghi** (ETW) — ở nguồn Counters hiện giải thích + nút bật chi tiết.
- **Cảnh báo**: danh sách cảnh báo log spam / tệp log lớn đang ghi trong phiên; Settings → Monitor: bật cảnh báo, X MB/s, Y phút.
- **24 giờ**: biểu đồ cột theo phút + top tiến trình ghi nhiều nhất trong 24 h.
- **Ai đã chạm vào đây? (F7)**: chọn thư mục → dòng thời gian tạo / ghi / xóa / đổi tên theo tiến trình (chỉ ETW).
- **Tray (G5)**: icon khay hệ thống với *Mở NANOFreeDisk*, *Tạm dừng / Tiếp tục Monitor*, *Thoát*.
- Expert hiện lệnh `nfd monitor …` tương đương.

## 3. Kiểm thử

- Unit: rates từ hai lần đếm (PID tái sử dụng không cộng nhầm, tiến trình mới = 0), aggregator, chart (vòng, cắt 60 / 600), downsample phút, `SpamWatch` (đúng Y phút liên tục mới báo, rơi dưới ngưỡng thì đặt lại, không báo trùng), tệp log lớn.
- Parser ETW: bản ghi tổng hợp cho FileIo v2/v3 (Create, Read, Write, Delete, Rename, Name, DeletePath, RenamePath), Thread; bản ghi cụt / phiên bản lạ → bỏ qua, không panic (proptest trên byte ngẫu nhiên); đường dẫn thiết bị → ổ.
- Tích hợp **ETW thật** (`crates/nfd-monitor/tests/etw_live.rs`): chỉ chạy khi tiến trình đang admin (runner CI Windows là admin; máy dev không admin → in "skipped"): bắt đầu phiên, ghi một tệp tạm 4 MB, kiểm tra thấy PID của test ghi ≥ 4 MB vào đúng đường dẫn, dừng phiên và phiên không còn tồn tại.
- Helper: round-trip frame protocol 2, request lạ bị từ chối.
- `nfd-history`: ghi/đọc/prune phút > 24 h.
- UI: hàm thuần (định dạng, cắt chart, sort) trong `monitor.test.ts`; `gui-smoke.mjs` mở view Monitor (nguồn Counters), kiểm tra bảng, biểu đồ, tạm dừng, tab 24 h.
- Overhead: đo CPU time của tiến trình (app hoặc `nfd monitor`) qua 60 s lấy mẫu với tải ghi nhẹ; ETW đo trên runner CI / khi có admin.

## 4. Lát cắt

1. **Lõi + Counters** — crate `nfd-monitor` (model, rates, aggregator, chart, phút, `SpamWatch`), `nfd-tasks` lộ danh sách tiến trình + số thao tác I/O, PDH theo ổ, `nfd monitor` (Counters), schema JSON.
2. **ETW** — parser, phiên SystemTraceProvider riêng, map thiết bị → ổ, lưới an toàn, `nfd monitor --source etw [--watch]`, test ETW thật (chỉ khi admin) + bước CI.
3. **Helper + backend app** — protocol 2, kết nối dùng chung, `MonitorState` + lệnh Tauri, lịch sử 24 h, cảnh báo C8 nền + thông báo.
4. **GUI** — view Monitor (biểu đồ, ổ, tiến trình, tệp, top ghi, cảnh báo, 24 h), End task qua dialog có bảo vệ, Settings → Monitor, mock, locales, `gui-smoke`.
5. **F7 + G5** — tab "Ai đã chạm vào đây?", tray với Tạm dừng Monitor.
6. **DoD** — đo overhead (`docs/monitor-overhead.md`), README, CHANGELOG, PLAN.

## 5. Ngoài phạm vi Phase 6

- Byte vật lý của ổ **theo tiến trình** (`DiskIo` + IssuingThreadId) — chỉ cần khi điều tra paging; v1.x nếu có nhu cầu.
- Mạng theo tiến trình, GPU — không thuộc Monitor ổ đĩa.
- Kết thúc / tạm dừng / hạ priority tiến trình từ Monitor — không bao giờ tự động; người dùng làm qua Tasks.
- Monitor chạy như service khi app đóng; lịch sử dài hơn 24 h.
- macOS (`fs_usage`) / Linux (`/proc/diskstats`, fanotify) — Phase 10.

## 6. Rủi ro

- **Không kiểm chứng ETW được trên máy dev không admin** → parser test bằng bản ghi tổng hợp, test ETW thật trên runner CI (admin), lưới an toàn `etw-silent` quay về Counters.
- Layout sự kiện kernel đổi theo bản Windows → đọc theo `Version` của sự kiện, phiên bản lạ bị bỏ qua và đếm (UI báo "không giải mã được").
- Tải sự kiện cao (build lớn: hàng trăm nghìn sự kiện/giây) → callback chỉ cập nhật map, buffer 64 KB, đếm sự kiện mất thay vì chặn; giới hạn map tên tệp.
- Phiên kernel mồ côi sau crash chiếm buffer → tên cố định, dừng phiên cũ khi bắt đầu.
- Antivirus coi ETW kernel là đáng ngờ → chỉ bật khi người dùng bấm, giải thích trong UI.
- Số liệu Counters gồm cả I/O mạng/pipe → UI ghi rõ "mọi I/O", khuyên bật chi tiết khi cần tệp cụ thể.
