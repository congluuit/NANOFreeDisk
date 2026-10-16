# Hardware Monitor + Tasks — Kế hoạch (Phase 3B, chạy song song với Phase 3 Clean)

> Trạng thái: **kế hoạch**, chưa code. Quyết định 2026-10-16: làm song song với Phase 3 (Safety + Clean), dùng **PawnIO** (opt-in) cho cảm biến CPU/mainboard, tab **Tasks** cho phép người dùng *tự bấm* kết thúc tiến trình (app không bao giờ tự kill). Xem `docs/adr/0002-hardware-sensors-pawnio.md`.
>
> Khi gộp vào `docs/PLAN.md`: thêm trụ cột này vào §0/§1.2, thêm hàng vào §8, sửa §13.3 (hàng "Tự kill process / dừng service") và F6 trong §13.2 theo mục 6 bên dưới.

## 1. Mục tiêu

Một view **Hardware** thay cho việc mở thêm HWiNFO / LibreHardwareMonitor / CPU-Z / Task Manager khi chỉ cần xem nhanh:

1. **Tổng quan cấu hình** — toàn bộ phần cứng máy trên một trang, sao chép / xuất được (gửi hỗ trợ kỹ thuật).
2. **Cảm biến** — nhiệt độ, điện áp, công suất, xung, tải, quạt của từng thành phần; cột Value / Min / Max như LibreHardwareMonitor; ngưỡng nóng tô màu.
3. **Tasks** — danh sách tiến trình kiểu Task Manager (CPU / RAM / Disk / GPU), tìm kiếm, sắp xếp, cây cha–con, End task có bảo vệ.
4. **Thẻ nhiệt độ trên Home** — CPU / GPU / ổ nóng nhất, bấm vào mở Hardware.

Nguyên tắc: **chỉ đọc phần cứng** — không bao giờ ghi MSR, đổi điện áp, xung, quạt, power limit. Tiến trình chỉ bị kết thúc khi người dùng bấm và xác nhận.

## 2. UI

```
Sidebar: … Analyze │ Clean │ Hardware ▸ [Tổng quan] [Cảm biến] [Tasks]
```

- **Tổng quan**: thẻ theo nhóm — Hệ thống (hãng/model máy, OS + build, uptime), CPU (tên, P/E core, thread, cache, xung base/boost, socket), Mainboard + BIOS, RAM (tổng, từng thanh: hãng, part number, dung lượng, tốc độ, khe), GPU (tên, VRAM, driver), Ổ đĩa (model, bus, dung lượng, nhiệt, sức khỏe — dùng chung Health Phase 7), Màn hình, Mạng, Pin. Nút *Sao chép dạng văn bản* / *Xuất JSON* (schema `nanofreedisk.hwinfo.v1`).
- **Cảm biến**: cây `Thiết bị → nhóm (Temperatures / Voltages / Powers / Clocks / Load / Fans / Data) → cảm biến` với Value / Min / Max, sparkline 60 s khi hover, nút *Reset min/max*; Simple mode chỉ hiện các cảm biến chính (CPU package, P/E-core max, GPU core/hotspot, ổ, quạt), Expert hiện tất cả.
- **Tasks**: thanh tóm tắt (CPU %, RAM, Disk B/s, GPU %) + bảng: Tên, PID, Trạng thái, CPU %, RAM (private working set), Disk R/W/s, GPU %, Publisher (Authenticode, cache theo đường dẫn), Đường dẫn; Expert thêm command line, user, priority, số handle/thread. Nhóm *Ứng dụng* (có cửa sổ) / *Nền* / *Windows*; chế độ cây theo parent PID. Menu chuột phải: *Đóng ứng dụng* (WM_CLOSE), *Kết thúc tiến trình*, *Kết thúc cây tiến trình*, *Mở vị trí tệp*, *Sao chép đường dẫn / command line*, *Mở trong Task Manager*.
- Mọi chuỗi EN + VI (`locales/{en,vi}/hwmon.ftl`), mọi màu là theme token (thêm token `temp-ok / temp-warm / temp-hot`).

## 3. Nguồn dữ liệu

### 3.1 Không cần driver, không cần admin (mặc định)

| Dữ liệu | Nguồn |
|---|---|
| CPU tên, family, P/E core, cache | `CPUID` (leaf 0x04, 0x1A hybrid; `raw-cpuid` crate) + `GetLogicalProcessorInformationEx` |
| Mainboard, BIOS, RAM từng thanh, hãng máy | SMBIOS qua `GetSystemFirmwareTable('RSMB')` (type 0, 1, 2, 4, 17) |
| GPU tên, VRAM, LUID | DXGI `EnumAdapters1`; phiên bản driver từ registry class GUID display |
| **GPU nhiệt độ, tải, VRAM dùng, xung, quạt** | `D3DKMTQueryAdapterInfo(KMTQAITYPE_ADAPTERPERFDATA)` (cách Task Manager làm, WDDM ≥ 2.4); NVIDIA thêm NVML (`nvml.dll` đi kèm driver) cho công suất/hotspot; AMD: ADLX sau |
| CPU tải từng core | `NtQuerySystemInformation(SystemProcessorPerformanceInformation)` |
| CPU xung (xấp xỉ) | PDH `\Processor Information(*)\% Processor Performance` × xung base — ghi rõ "ước tính" khi không có PawnIO |
| RAM dùng / khả dụng | `GlobalMemoryStatusEx` |
| Nhiệt độ ACPI thermal zone | PDH `\Thermal Zone Information(*)\Temperature` (thô, ghi rõ nguồn) |
| Ổ đĩa nhiệt, sức khỏe | NVMe log 02h / SMART — dùng chung `nfd-health` (Phase 7); cái nào cần admin đi qua helper |
| Pin | `IOCTL_BATTERY_QUERY_INFORMATION` (design / full charge capacity → độ chai) |
| Tiến trình | `NtQuerySystemInformation(SystemProcessInformation)` (1 lời gọi cho cả danh sách), `GetProcessIoCounters` (delta/s), PDH `\GPU Engine(*)\Utilization Percentage` theo `pid_`, `QueryFullProcessImageNameW`, `WinVerifyTrust` (cache) |

### 3.2 Với PawnIO (opt-in, qua helper elevated)

| Dữ liệu | Module PawnIO chính thức |
|---|---|
| Intel: nhiệt từng core + package, TjMax, công suất package/IA/GT (RAPL), PL1/PL2/PL4, VID, xung thật, voltage offset (chỉ đọc mailbox) | `IntelMSR` |
| AMD Zen: Tctl/Tdie, CCD, SoC, công suất (RAPL), xung | `AMDFamily17` (+ các family mới) |
| Mainboard: quạt, điện áp, nhiệt Super I/O (Nuvoton, ITE, Fintek…) | `LpcIO` — làm theo từng chip, có bảng chip đã kiểm chứng |
| RAM SPD nhiệt, PCH | `SmbusI801` / … — giai đoạn sau |

Ảnh chụp tham chiếu (i9-13900HX, System76 Adder WS): Package/P-core/E-core temp, Powers, PL1/PL2/PL4, VID, Clocks → cần `IntelMSR`; PCH temp → giai đoạn sau.

## 4. Kiến trúc

- Crate **`nfd-hwinfo`**: thông tin tĩnh (CPUID, SMBIOS, DXGI, display, network, battery) → `HwInventory` (serde, schema `nanofreedisk.hwinfo.v1`). Không cần admin. Có test parse SMBIOS từ blob mẫu trong `tests/fixtures/smbios/`.
- Crate **`nfd-sensors`**: `trait SensorProvider { fn id(); fn discover() -> Vec<SensorInfo>; fn sample(&mut self, out: &mut Vec<Reading>) }`; providers: `os` (CPU load, RAM, thermal zone), `gpu` (D3DKMT, NVML), `storage` (dùng `nfd-health`), `pawnio` (feature `pawnio`, chỉ trong helper). `SensorHub` lấy mẫu 1 s trên thread riêng, chỉ chạy khi view Hardware mở hoặc bật cảnh báo nhiệt; giữ min/max và ring buffer 10 phút.
- Crate **`nfd-tasks`**: snapshot tiến trình + delta CPU/IO/GPU, phân nhóm App/Nền/Windows, `end_process(pid, start_time, mode)` qua bộ kiểm tra mục 5.
- **Helper elevated** (§3.2 nguyên tắc 6 của PLAN, cùng binary `--elevated-helper`, named pipe có kiểm tra token + PID cha): lần đầu dùng cho PawnIO; về sau dùng chung cho SMART, ETW, MFT. Helper chỉ nhận lệnh *đọc cảm biến* và *kết thúc PID đã xác nhận* — không có lệnh tùy ý.
- Tauri: `src-tauri/src/commands/hwmon.rs` (`hw_inventory`, `sensors_subscribe` → event `sensor-sample` mỗi 1 s, `sensors_reset_minmax`, `tasks_snapshot`, `task_end`), view `src/lib/views/Hardware*.svelte`, mock backend cho `pnpm dev`.
- CLI: `nfd hw [--json]`, `nfd sensors [--watch] [--json]`, `nfd ps [--sort cpu] [--json]`.

## 5. An toàn cho Tasks (End task)

- Người dùng bấm → dialog nêu tên, PID, publisher, đường dẫn, "Nếu bấm thì…" (dữ liệu chưa lưu có thể mất) → xác nhận. Không có kill hàng loạt, không có tự động.
- **Chặn cứng** (không có nút): tiến trình critical (`IsProcessCritical`, `ProcessBreakOnTermination`), PID 0/4, `Registry`, `Memory Compression`, `Secure System`, `smss`, `csrss`, `wininit`, `winlogon`, `services`, `lsass`, `lsaiso`, `fontdrvhost`, `dwm`, tiến trình protected/PPL (Defender…), chính NANOFreeDisk và helper.
- **Cảnh báo thêm** (xác nhận 2 lần): `svchost` (hiện các service bên trong), `explorer` (gợi ý *Khởi động lại Explorer* thay vì kết thúc), tiến trình Microsoft-signed trong `%SystemRoot%`.
- Chống trùng PID: luôn gửi kèm `CreationTime`; mở handle rồi kiểm tra lại trước `TerminateProcess`.
- "Đóng ứng dụng" thử `WM_CLOSE` tới cửa sổ chính trước, chờ 5 s, sau đó mới đề nghị buộc kết thúc.
- Tiến trình của user khác / elevated → qua helper, chỉ khi người dùng bấm *Chạy với quyền admin*.
- Mọi lần kết thúc ghi vào Activity log (G6). Không có Undo — dialog nói rõ.

## 6. PawnIO — cài đặt & phân phối

- **Opt-in**: Settings → *Cảm biến nâng cao (PawnIO)*; bộ cài thương hiệu có checkbox (mặc định **tắt**). Không có PawnIO app vẫn chạy đầy đủ, chỉ thiếu cảm biến CPU/mainboard và hiện lý do.
- Driver kernel **không thể "portable" trong thư mục app**: nó phải được đăng ký như một service hệ thống (cần admin một lần). Dùng **bộ cài chính thức `PawnIO_setup.exe`** (đi kèm trong bộ cài / tải từ pawnio.eu), kiểm tra chữ ký Authenticode trước khi chạy. PawnIO là thành phần dùng chung (Fan Control, LibreHardwareMonitor, OpenRGB) → gỡ NANOFreeDisk **không** gỡ PawnIO, chỉ nhắc người dùng.
- Phát hiện: mở device PawnIO; không có → trạng thái "Chưa cài"; có nhưng không load (HVCI/Memory Integrity, chặn bởi policy) → hiện lý do + hướng dẫn, không cố vượt.
- Giấy phép: PawnIO là GPL-2.0 có ngoại lệ cho chương trình độc lập giao tiếp *chỉ qua IOCTL* → app MIT gọi IOCTL là hợp lệ; module nạp vào phải tương thích giấy phép → chỉ dùng module chính thức (`PawnIO.Modules`). Phân phối lại bộ cài: thêm license + link mã nguồn vào `THIRD-PARTY-NOTICES`; **xác nhận điều khoản phân phối với tác giả trước khi phát hành**.
- Phải kiểm chứng trước khi ship: dung lượng thực tế bộ cài/driver, cờ cài im lặng, Windows Defender không cảnh báo, chạy được khi bật Memory Integrity (HVCI).

## 7. Các lát cắt (slices)

1. **Tổng quan cấu hình** — `nfd-hwinfo`, view Tổng quan, `nfd hw`, xuất JSON/văn bản.
2. **Cảm biến không driver** — `nfd-sensors` (os + gpu), view Cảm biến, thẻ nhiệt độ Home, `nfd sensors`.
3. **Tasks** — `nfd-tasks`, view Tasks, End task theo mục 5, `nfd ps`.
4. **Helper elevated + PawnIO Intel** — named pipe, cài/phát hiện PawnIO, `IntelMSR` (temps, RAPL, PL, VID, xung).
5. **PawnIO AMD + Super I/O** — quạt, điện áp mainboard theo chip.
6. **Cảnh báo & Health** — ngưỡng nhiệt (tray toast), đóng góp `HealthContribution` (CPU/GPU/ổ quá nóng), nhật ký 24 h.

## 8. Definition of Done

- Giá trị khớp LibreHardwareMonitor / HWiNFO trong ±2 °C, ±5 % công suất/xung trên máy tham chiếu (i9-13900HX) và ≥ 1 máy AMD.
- Lấy mẫu 1 s tốn < 1 % CPU; view đóng → không lấy mẫu.
- Tasks: danh sách 300 tiến trình cập nhật mỗi 1 s không giật; test chặn cứng cho toàn bộ danh sách mục 5.
- Không cảnh báo Windows Defender; chạy (thiếu PawnIO thì hạ cấp êm) khi bật Memory Integrity.
- `scripts/check-locales.mjs` xanh; `gui-smoke.mjs` phủ 3 tab (Tasks chỉ thử trên tiến trình con do smoke tự tạo).
