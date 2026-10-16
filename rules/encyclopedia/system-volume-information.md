+++
id = "system-volume-information"
risk = "protected"
volume_paths = ["System Volume Information"]
+++

## en
### System Volume Information

System Restore points, shadow copies (previous versions of files), the search index and other per-drive system data. Even administrators cannot open it by default.

**If you delete it:** you lose restore points and file history; Windows features that rely on it may break.

**Instead:** System Properties → System Protection → Configure lets you lower the space restore points may use or delete them all; `vssadmin list shadowstorage` shows the current use.

## vi
### System Volume Information

Điểm khôi phục hệ thống (System Restore), shadow copy (phiên bản trước của tệp), chỉ mục tìm kiếm và dữ liệu hệ thống khác của từng ổ. Mặc định cả admin cũng không mở được.

**Nếu xóa:** mất điểm khôi phục và lịch sử tệp; các tính năng Windows dựa vào nó có thể hỏng.

**Nên làm:** System Properties → System Protection → Configure cho phép giảm dung lượng dành cho điểm khôi phục hoặc xóa hết; `vssadmin list shadowstorage` cho biết đang dùng bao nhiêu.
