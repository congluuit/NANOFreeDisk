# Monitor — đo overhead (DoD Phase 6: < 2 % CPU)

> Đo 2026-10-16 bằng `scripts/monitor-overhead.ps1`: chạy `nfd monitor --json` (bản release) trong một khoảng thời gian cố định, cùng lúc ghi **tải nhẹ, đều** (4 MB mỗi 0,5 s ≈ 8 MB/s xoay vòng 8 tệp — không phải stress test), rồi lấy CPU time của tiến trình monitor chia cho (thời gian × số luồng logic), cách Task Manager tính.

| Máy | Nguồn | Thời gian | Ghi | CPU time | % mọi luồng logic | % một luồng |
|---|---|---|---|---|---|---|
| Laptop tham chiếu i9-13900HX (32 luồng), Windows 10 22H2, không admin | Counters | 60,8 s | 464 MB | 0,66 s | **0,034 %** | 1,08 % |
| Runner GitHub `windows-latest` (4 vCPU), admin | ETW (`--source etw`) | 30,8 s | 232 MB | 0,47 s | **0,381 %** | 1,52 % |
| Runner GitHub `windows-latest` (4 vCPU), admin | Counters | 30,6 s | 232 MB | 0,38 s | **0,306 %** | 1,23 % |

Kết luận: cả hai nguồn dưới 2 % CPU một cách thoải mái (kể cả khi tính theo một luồng). Job CI `etw` chạy lại phép đo này mỗi lần và **thất bại nếu ≥ 2 %**.

## Ghi chú

- Số trên là CPU của **tiến trình monitor** (lấy mẫu mỗi giây: một lần `NtQuerySystemInformation`, một lần PDH, gộp và (ETW) giải mã sự kiện trong callback). Chi phí phía kernel để *ghi* sự kiện ETW được tính vào các luồng đang làm I/O, không đo riêng được bằng cách này; với các cờ đang dùng (`FILE_IO_INIT`, `DISK_FILE_IO`, `THREAD`; `FILE_IO` chỉ khi đang "Ai đã chạm vào đây?") nó là một bản ghi nhỏ cho mỗi request I/O.
- ETW không đo được trên laptop tham chiếu trong phiên này vì shell không có quyền admin (kernel trace bị từ chối — đúng như thiết kế: app dùng helper elevated). Phiên ETW thật được kiểm tra trên runner CI (admin): `crates/nfd-monitor/tests/etw_live.rs` (tạo / ghi / đổi tên / xóa được gán đúng PID và đường dẫn, không đếm đôi khi flush, phiên dừng sạch) và `nfd monitor --source etw`.
- Trong app, Monitor chỉ lấy mẫu khi view đang mở hoặc cảnh báo log spam bật, và không bao giờ khi tạm dừng (nút trong view hoặc tray).

## Chạy lại

```powershell
cargo build --release -p nfd-cli
pwsh scripts/monitor-overhead.ps1 -Source counters -Seconds 60
pwsh scripts/monitor-overhead.ps1 -Source etw -Seconds 60   # cần PowerShell chạy admin
```
