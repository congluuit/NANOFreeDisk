+++
id = "winsxs"
risk = "protected"
paths = ["${SYSTEMROOT}/WinSxS"]
+++

## en
### Windows component store (WinSxS)

Every version of every Windows system file that updates, repairs and optional features may need. Explorer overstates its size: most files are **hard links** shared with `C:\Windows\System32`, so the real extra space is usually 5–10 GB.

**If you delete it:** Windows can no longer install updates, repair itself or turn features on — often it stops booting.

**Instead:** run `Dism /Online /Cleanup-Image /StartComponentCleanup` (or Disk Cleanup → "Windows Update Cleanup") to remove superseded versions safely.

## vi
### Kho thành phần Windows (WinSxS)

Chứa mọi phiên bản tệp hệ thống mà Windows cần để cập nhật, tự sửa lỗi và bật tính năng. Explorer báo dung lượng lớn hơn thực tế: phần lớn là **hard link** dùng chung với `C:\Windows\System32`, phần chiếm thêm thật thường chỉ 5–10 GB.

**Nếu xóa:** Windows không cài được bản cập nhật, không tự sửa được, thường không khởi động được nữa.

**Nên làm:** chạy `Dism /Online /Cleanup-Image /StartComponentCleanup` (hoặc Disk Cleanup → "Windows Update Cleanup") để bỏ các phiên bản cũ một cách an toàn.
