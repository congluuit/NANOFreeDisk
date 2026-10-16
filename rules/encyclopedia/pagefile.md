+++
id = "pagefile"
risk = "protected"
volume_paths = ["pagefile.sys", "swapfile.sys"]
+++

## en
### Page file (virtual memory)

Disk space Windows uses as extra memory when RAM is full, and for crash dumps. `swapfile.sys` does the same for Store apps. Windows sizes them automatically.

**If you delete it:** you cannot while Windows runs; turning it off can make programs crash with "out of memory" errors and prevents crash dumps.

**Instead:** Settings → System → About → Advanced system settings → Performance → Virtual memory lets you move it to another drive or set a fixed size.

## vi
### Tệp trang (bộ nhớ ảo)

Phần ổ đĩa Windows dùng làm bộ nhớ phụ khi RAM đầy và để ghi crash dump. `swapfile.sys` làm việc tương tự cho ứng dụng Store. Windows tự chọn kích thước.

**Nếu xóa:** không xóa được khi Windows đang chạy; tắt nó có thể khiến chương trình bị lỗi "hết bộ nhớ" và không ghi được crash dump.

**Nên làm:** Settings → System → About → Advanced system settings → Performance → Virtual memory cho phép chuyển sang ổ khác hoặc đặt kích thước cố định.
