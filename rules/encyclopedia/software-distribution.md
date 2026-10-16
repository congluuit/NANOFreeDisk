+++
id = "software-distribution"
risk = "advanced"
paths = ["${SYSTEMROOT}/SoftwareDistribution"]
rules = ["windows.update.download-cache"]
+++

## en
### Windows Update download area

Where Windows Update downloads and stages updates (`Download`) and keeps its update history database (`DataStore`). It can grow to several GB while updates are pending.

**If you delete it:** while Windows Update is running, updates can fail or get stuck; the update history list is lost.

**Instead:** use Disk Cleanup → "Windows Update Cleanup" / "Delivery Optimization Files", or Settings → Windows Update → troubleshoot. NANOFreeDisk's Clean module (Phase 3) will do this the supported way.

## vi
### Vùng tải xuống của Windows Update

Nơi Windows Update tải và chuẩn bị bản cập nhật (`Download`) và lưu cơ sở dữ liệu lịch sử cập nhật (`DataStore`). Có thể lên vài GB khi đang chờ cài cập nhật.

**Nếu xóa:** trong lúc Windows Update đang chạy, cập nhật có thể lỗi hoặc treo; danh sách lịch sử cập nhật bị mất.

**Nên làm:** dùng Disk Cleanup → "Windows Update Cleanup" / "Delivery Optimization Files". Module Dọn dẹp của NANOFreeDisk (Phase 3) sẽ làm việc này đúng cách.
