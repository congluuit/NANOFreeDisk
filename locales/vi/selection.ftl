# Thanh chọn dùng chung cho mọi danh sách người dùng tick trước khi thao tác
# (SelectionBar.svelte, ConfirmTotals.svelte, GroupCheck.svelte). Mọi khóa phải có
# trong en/selection.ftl.

sel-summary = Đã chọn { $count } mục · { $files } tệp · { $size } sẽ được giải phóng
sel-summary-nofiles = Đã chọn { $count } mục · { $size } sẽ được giải phóng
sel-none = Chưa chọn mục nào — hãy tick từng mục hoặc dùng các nút bên dưới.
sel-skip = { $count } mục có thể bị bỏ qua vì { $apps } đang chạy ({ $size } không tính vào tổng)
sel-group-label = Lựa chọn
sel-all = Chọn tất cả
sel-clear = Bỏ chọn tất cả
sel-only = Chỉ chọn "{ $level }"
sel-only-hint = Thay lựa chọn hiện tại bằng mọi mục thuộc mức rủi ro này.
sel-keys-hint = Ctrl+A chọn mọi mục trong danh sách này, Esc bỏ chọn.
sel-group = Chọn mọi mục trong { $name }
sel-clean-n = Dọn { $count } mục ({ $size })
sel-recycle-n = Chuyển { $count } mục vào Thùng rác ({ $size })
sel-quarantine-n = Chuyển { $count } mục vào khu cách ly ({ $size })
sel-confirm-total = { $count } mục · { $size } sẽ được giải phóng
sel-confirm-level = { $count } mục · { $size }
sel-ack = Tôi hiểu: các mục Thận trọng / Nâng cao ở trên sẽ mất lịch sử hoặc thiết lập, hoặc phải tải lại.
