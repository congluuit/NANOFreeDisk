+++
id = "temp"
risk = "caution"
paths = ["${TEMP}", "${SYSTEMROOT}/Temp"]
rules = ["windows.temp.user", "windows.temp.system"]
+++

## en
### Temporary files

Scratch space for installers, updaters and running programs. Most of it is left over and safe to remove, but files that are **in use** belong to programs that are running right now.

**If you delete it:** a running installer or program can fail; old leftovers are simply gone. Windows recreates the folder itself.

**Instead:** use Settings → System → Storage → Temporary files, or NANOFreeDisk's Clean module (Phase 3), which skips files in use and recent files.

## vi
### Tệp tạm

Vùng làm việc tạm của bộ cài, trình cập nhật và chương trình đang chạy. Phần lớn là đồ thừa và xóa được, nhưng tệp **đang được dùng** thuộc về chương trình đang chạy.

**Nếu xóa:** bộ cài hay chương trình đang chạy có thể lỗi; phần thừa cũ chỉ đơn giản biến mất. Windows tự tạo lại thư mục.

**Nên làm:** dùng Settings → System → Storage → Temporary files, hoặc module Dọn dẹp của NANOFreeDisk (Phase 3) — bỏ qua tệp đang dùng và tệp mới.
