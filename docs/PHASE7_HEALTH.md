# Phase 7 — Health: kế hoạch chi tiết

> Trạng thái: **đang làm** — lát 1 ✅ (crate `nfd-health`, NVMe / ATA không cần admin, volume, `nfd health`, `docs/health-validation.md`), lát 2 ✅ (đọc đầy đủ bằng quyền admin qua helper: SMART + ngưỡng, SAT qua cầu USB, dirty bit), lát 3 ⏳ (Health Score, view Health, xuất JSON/HTML), lát 4 ⏳ (cảnh báo + tray). Bản chi tiết của hàng "7 – Health" trong `docs/PLAN.md` §8, theo thiết kế §4.6, lệnh `nfd health` ở §4.7, mục F1 (Health Score: SMART/NVMe + nhiệt độ), phần chỉ-đọc của F4 và G5 (tray) ở §13.2. Health **chỉ đọc**: không ghi cấu hình SMART, không bật/tắt tính năng ổ, không chạy self-test.

## 1. Mục tiêu & DoD

- Trả lời câu hỏi của nút G1 "Ổ đĩa có khỏe không?" cho mọi ổ vật lý: **Tốt / Cần chú ý / Xấu** (kiểu CrystalDiskInfo), kèm lý do đọc được bằng lời, nhiệt độ, tuổi thọ còn lại, giờ chạy, dữ liệu đã ghi.
- Kiểm tra volume chỉ-đọc: dirty bit, TRIM, BitLocker, cluster size, căn lề phân vùng 4K, xu hướng dung lượng trống 30 ngày → ngày dự kiến đầy (dùng lại `History::forecast`).
- Health Score (F1) có thêm điểm trừ cho ổ Cần chú ý / Xấu, ổ quá nhiệt (theo ngưỡng của chính ổ), volume bị đánh dấu dirty.
- Cảnh báo (G5): thông báo hệ thống khi trạng thái SMART đổi xấu đi, nhiệt độ ổ vượt ngưỡng của ổ, ổ còn < 10 % trống; icon tray với dung lượng trống từng ổ và tình trạng chung.
- **DoD:**
  - Kết quả khớp CrystalDiskInfo trên ≥ 5 ổ mẫu (SATA SSD, NVMe, HDD, USB). Máy tham chiếu (laptop i9-13900HX) chỉ có **2 ổ NVMe** và không cài CrystalDiskInfo; không mua / gắn thêm phần cứng. Vì vậy: (a) so trên 2 ổ NVMe thật với một bộ đọc độc lập (PowerShell + `DeviceIoControl` viết riêng, `Get-PhysicalDisk`) và ghi vào `docs/health-validation.md`; (b) SATA / HDD / USB được phủ bằng test trên sector SMART đúng định dạng thật (bảng dựng từ giá trị điển hình — không phải dump của ổ thật) cho 3 kết quả điển hình của CrystalDiskInfo; (c) tài liệu ghi rõ cách so với CrystalDiskInfo trên máy khác (bảng giá trị cần so, cách đọc) để hoàn tất DoD khi có ổ.
  - Không cần quyền admin cho NVMe (đã kiểm tra: log 02h đọc được qua `IOCTL_STORAGE_QUERY_PROPERTY` với handle không quyền) và cho thông tin ổ / volume cơ bản; phần cần admin hiện rõ "cần quyền quản trị" và có nút đọc một lần qua helper (UAC).
  - Không bao giờ gửi lệnh ghi tới ổ: helper chỉ có các lệnh đọc cố định (IDENTIFY, SMART READ DATA, SMART READ THRESHOLDS), yêu cầu qua pipe chỉ mang số thứ tự ổ, không mang byte lệnh.
  - EN + VI (`locales/{en,vi}/health.ftl`), mọi màu là token theme, CLI `nfd health [--json]`, schema `nanofreedisk.health.v1`, `gui-smoke.mjs` phủ view Health.

## 2. Thành phần

### 2.1 Crate mới `nfd-health` (theo §3.1)

Phụ thuộc `nfd-hwinfo` (mở `\\.\PhysicalDriveN` không quyền, tên bus, nhiệt độ + ngưỡng nhiệt của ổ), `nfd-platform` (danh sách volume). Không phụ thuộc UI; mọi diễn giải (Tốt / Cần chú ý / Xấu) là hàm thuần có test.

**Liệt kê ổ** (không cần admin):
- `IOCTL_STORAGE_QUERY_PROPERTY`: `StorageDeviceProperty` (model, serial, firmware, bus, removable), `StorageDeviceSeekPenaltyProperty` (SSD/HDD), `StorageDeviceTrimProperty` (ổ hỗ trợ TRIM), `StorageDeviceTemperatureProperty` (nhiệt độ, ngưỡng cảnh báo / tới hạn của ổ); `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` (dung lượng).
- Ổ ↔ volume: `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` trên `\\.\C:` (số ổ + offset phân vùng → kiểm tra căn lề 4K).
- WMI `MSFT_PhysicalDisk` / `MSFT_StorageReliabilityCounter`: **không dùng** trong v1 — cần COM/WMI (dependency lớn), `Get-StorageReliabilityCounter` cần admin, và mọi trường của nó (nhiệt độ, wear, lỗi đọc/ghi, giờ chạy) đã có từ log NVMe / bảng SMART đọc trực tiếp. Ghi lại ở §5.

**NVMe** — serial thật, model, firmware, WCTEMP/CCTEMP từ Identify Controller (`StorageAdapterProtocolSpecificProperty`, CNS 01h — descriptor của Windows chỉ cho EUI-64); health log qua `StorageDeviceProtocolSpecificProperty` (`ProtocolTypeNvme`, `NVMeDataTypeLogPage`, log `02h`, NSID 0 = cả controller): Critical Warning, Composite Temperature, Available Spare / Threshold, Percentage Used, Data Units Read/Written (× 512 000 byte), Host Read/Write Commands, Controller Busy Time, Power Cycles, Power-On Hours, Unsafe Shutdowns, Media and Data Integrity Errors, Number of Error Log Entries, Warning / Critical Composite Temperature Time. Log toàn số 0 (một số driver trả về như vậy với NSID `FFFFFFFF`) → coi là không đọc được và thử NSID khác.

**ATA / SATA**:
- Không admin: `IOCTL_STORAGE_PREDICT_FAILURE` → cờ "dự báo hỏng" + 512 byte dữ liệu SMART (bảng thuộc tính; không có ngưỡng).
- Admin (helper hoặc app/CLI đang chạy elevated): `SMART_RCV_DRIVE_DATA` với `READ_ATTRIBUTES` (D0h) và `READ_THRESHOLDS` (D1h) → bảng thuộc tính + ngưỡng.
- Diễn giải các thuộc tính chính: 05 Reallocated Sectors, C5 Current Pending, C6 Offline Uncorrectable, C7 UDMA CRC errors, C2/BE nhiệt độ, 09 Power-On Hours, 0C Power Cycles, F1/F2 Total LBAs Written/Read, các thuộc tính tuổi thọ SSD (A9, AD, B1, CA, E7, E8, E9 — theo vendor).

**USB** (bus `USB`): thử **SAT** — `IOCTL_SCSI_PASS_THROUGH` với CDB `ATA PASS-THROUGH (16)` chỉ mang SMART READ DATA / READ THRESHOLDS (cần admin); cầu không hỗ trợ → "Không đọc được SMART qua cầu USB này" (không coi là lỗi ổ).

**Volume** (mọi volume cố định + volume của ổ rời):
- File system, cluster size (`GetDiskFreeSpaceW`), dung lượng / trống, căn lề 4K của phân vùng.
- TRIM của Windows: `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem\DisableDeleteNotify` (NTFS) / `RefsDisableDeleteNotify` (ReFS) — chỉ đọc; chỉ báo khi ổ SSD hỗ trợ TRIM mà Windows đang tắt.
- BitLocker: thuộc tính shell `System.Volume.BitLockerProtection` (không cần admin): bật / tắt / đang mã hóa / tạm dừng / khóa.
- Dirty bit: `FSCTL_IS_VOLUME_DIRTY` (cần admin → helper; không có thì "chưa kiểm tra").
- Xu hướng: `History::forecast` (hồi quy trên dung lượng trống 30 ngày của lịch sử quét) → "đầy trong ~N ngày" hoặc "chưa đủ lịch sử".

**Kết quả** `HealthReport { schema, generated_at, elevated, drives: Vec<Drive>, volumes: Vec<Volume> }`; `Drive { index, model, serial, firmware, bus, size, ssd, removable, trim, status, reasons, temperature, temp_warning, temp_critical, life_left, power_on_hours, power_cycles, bytes_written, bytes_read, nvme?, ata?, source, unsupported? , volumes }`. Serial được hiện trong app; báo cáo xuất có tùy chọn ẩn serial (mặc định ẩn) vì người dùng gửi đi cho hỗ trợ kỹ thuật.

### 2.2 Diễn giải kiểu CrystalDiskInfo

| Loại | Xấu | Cần chú ý |
|---|---|---|
| ATA (có ngưỡng) | thuộc tính bất kỳ có ngưỡng ≠ 0 và giá trị hiện tại < ngưỡng (như CrystalDiskInfo; smartctl dùng ≤); cờ dự báo hỏng | raw của 05 / C5 / C6 > 0 |
| ATA (không ngưỡng, không admin) | cờ dự báo hỏng | raw của 05 / C5 / C6 > 0; hiện "chưa có ngưỡng — đọc đầy đủ cần admin" |
| SSD (ATA) | — | thuộc tính tuổi thọ ≤ 10 % |
| NVMe | Critical Warning bit 0 (spare dưới ngưỡng), 2 (độ tin cậy giảm), 3 (chỉ đọc), 4 (volatile backup hỏng); Available Spare < Threshold | bit 1 (nhiệt độ); Percentage Used ≥ 90 (tuổi thọ còn ≤ 10 %); Media Errors > 0 |
| Không đọc được | — | — → **Không rõ** (không trừ điểm) |

Nhiệt độ không đổi trạng thái (như CrystalDiskInfo) mà là chỉ số riêng: ngưỡng là WCTEMP/CCTEMP của chính ổ (NVMe), nếu không có thì 60 °C (HDD) / 70 °C (SSD). Mỗi lý do có key message (`health-reason-*`) + giá trị, UI dịch.

### 2.3 Elevated helper (dùng chung với Hardware Monitor)

- `nfd-helper`: thêm `Request::Health` → `Response::Health { privileged }` (`nfd_health::Privileged`: bảng SMART + ngưỡng theo ổ, SAT theo ổ USB, dirty bit theo volume). Helper chỉ gọi các hàm đọc cố định của `nfd-health`; không có tham số lệnh từ app.
- App: nút "Đọc đầy đủ (quyền quản trị)" trong view Health → khởi động helper qua UAC (`nfd_helper::start`), gửi `Health`, đóng helper. Kết quả gộp vào báo cáo (`HealthReport::merge`). Từ chối UAC → giữ kết quả không admin.
- CLI `nfd health` trong terminal admin đọc thẳng (không qua helper).

### 2.4 Health Score (F1)

`nfd_core::health::DriveFacts` (thuần, có test) + `add_drive_findings`: ổ **Xấu** −35 (`drive-bad`), **Cần chú ý** −15 (`drive-caution`), ổ đang ≥ ngưỡng tới hạn −10 / ≥ ngưỡng cảnh báo −4 (`drive-hot` / `drive-warm`, bỏ qua khi Hardware Monitor đã có `hot-disk` / `warm-disk` cho ổ đó), volume dirty −10 (`volume-dirty`); ổ "Không rõ" chỉ là thông tin (`drive-unknown`). Nút Fix → view Health. `regrade` dùng chung các mốc điểm (≥ 85 tốt, ≥ 60 khá).

### 2.5 CLI

```
nfd health [--json] [--disk N] [--no-volumes] [--db <history.db>]
```

Bảng ổ (trạng thái, nhiệt độ, tuổi thọ, giờ chạy, đã ghi) + volume (FS, cluster, TRIM, BitLocker, dirty, trống, dự báo đầy). `--json` = `nanofreedisk.health.v1` (`docs/schema/health-v1.schema.json`). Mã thoát 0 = mọi ổ Tốt/Không rõ, 1 = có ổ Cần chú ý, 2 = có ổ Xấu (để script dùng).

### 2.6 GUI

- View **Health** (sidebar thay ô "P7", Ctrl+K, nút G1 "Ổ đĩa có khỏe không" ở Home): thẻ mỗi ổ (badge trạng thái màu `risk-safe` / `risk-caution` / `risk-protected`, nhiệt độ, tuổi thọ, giờ chạy, đã ghi, bus), chọn ổ → lý do + bảng NVMe log / thuộc tính SMART (Simple: các mục chính; Expert: toàn bộ, raw hex), ghi chú (USB không hỗ trợ, cần admin); bảng volume; nút "Đọc đầy đủ (quyền quản trị)", "Làm mới", xuất JSON / HTML (HTML tự chứa, không script, màu = CSS variables của theme, EN/VI), Expert: lệnh `nfd health` tương đương.
- Home: HealthCard hiện các finding mới, Fix → Health.

### 2.7 Cảnh báo & tray (G5)

- Khi bật (Settings → Health, mặc định bật): kiểm tra nhẹ mỗi 30 phút trong lúc app chạy (chỉ IOCTL, không quét) và ngay khi mở. So với trạng thái lần trước lưu ở `health-state.json` (thư mục dữ liệu app): trạng thái ổ xấu đi, nhiệt độ ổ ≥ ngưỡng cảnh báo của ổ (khi Hardware Monitor không bật cảnh báo nhiệt — tránh trùng), volume còn < 10 % trống (một lần mỗi lần đi xuống dưới ngưỡng). Thông báo qua `tauri-plugin-notification`, chữ do UI gửi xuống (EN/VI).
- Icon tray: tooltip + menu: Mở NANOFreeDisk, trạng thái ổ, dung lượng trống từng ổ (chỉ hiển thị), Thoát. Đóng cửa sổ vẫn thoát app như trước (không chạy nền ngầm).

## 3. Kiểm thử

- Unit (thuần): phân tích log NVMe 512 byte (trường 128-bit, nhiệt độ Kelvin), bảng SMART 30 mục + ngưỡng, các luật §2.2 trên từng ngưỡng, dump SMART mẫu (HDD Caution vì C5 > 0, SSD Good, HDD Bad vì 05 ≤ ngưỡng) với kết quả CrystalDiskInfo kèm theo; dựng CDB SAT; diff cảnh báo (đổi trạng thái, nhiệt độ, < 10 % một lần); Health Score với ổ Xấu / Cần chú ý / nóng / dirty.
- Tích hợp (Windows, không admin): liệt kê ít nhất một ổ, NVMe có Percentage Used / nhiệt độ hợp lý; `nfd health --json` đúng schema.
- `gui-smoke.mjs`: mở view Health, có thẻ ổ, xuất JSON/HTML vào thư mục tạm.
- So khớp: `docs/health-validation.md`.

## 4. Lát cắt

1. **Lõi không admin** — crate `nfd-health` (ổ, NVMe log 02h, ATA predict-failure, volume, luật §2.2), `nfd health [--json]`, schema, `docs/health-validation.md`.
2. **Quyền admin** — `SMART_RCV_DRIVE_DATA` + ngưỡng, SAT cho USB, dirty bit; `Request::Health` trong helper; `nfd health` elevated đọc thẳng.
3. **GUI + Health Score** — `DriveFacts` / `add_drive_findings`, lệnh app, view Health, Home, xuất JSON/HTML, `health.ftl`, mock, `gui-smoke`.
4. **Cảnh báo & tray** — watcher 30 phút, `health-state.json`, thông báo, icon tray, Settings.

## 5. Ngoài phạm vi Phase 7

- **Self-test** (short/extended), surface scan, `chkdsk` — F3, v1.x; không có nút nào gửi lệnh ghi hay lệnh bảo trì tới ổ.
- WMI `MSFT_StorageReliabilityCounter` (cần admin, trùng dữ liệu) — có thể thêm làm nguồn phụ khi một ổ không có SMART/NVMe đọc được.
- RAID / Storage Spaces: chỉ hiện ổ ảo + "không đọc được SMART của ổ thành viên".
- SCSI/SAS log pages, ổ SD/eMMC: "không hỗ trợ".
- TRIM chạy tay (`defrag /O`), đổi write-cache policy, Fast Startup (phần ghi của F4) — v1.x.

## 6. Rủi ro

- Một số cầu USB treo khi nhận lệnh pass-through lạ → chỉ gửi SAT 16 byte chuẩn, timeout 5 s, chỉ trong helper khi người dùng bấm; không bao giờ tự thử trong nền.
- Driver vendor (Intel RST, Samsung NVMe) có thể trả dữ liệu khác chuẩn → log toàn 0 bị bỏ, giá trị phi lý (nhiệt độ ≤ 0 / ≥ 150 °C) bị lọc, trạng thái "Không rõ" thay vì "Tốt".
- Ngưỡng tuổi thọ SSD ATA khác nhau theo vendor → chỉ dùng thuộc tính đã biết, còn lại hiện raw.
- Serial là dữ liệu nhận dạng → ẩn mặc định trong báo cáo xuất.
