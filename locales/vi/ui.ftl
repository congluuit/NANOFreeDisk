# Bố cục dùng chung cho mọi màn hình (thẻ số liệu, khung, phần Khám phá và
# Gợi ý của Phân tích).

## Stat cards

stat-free = Còn trống
stat-of = trên { $total }
stat-safe = Xóa an toàn
stat-review = Cần xem lại
stat-items = { $count ->
   *[other] { $count } mục
}
stat-checking = đang kiểm tra…
stat-outside = Ngoài lần quét này
stat-outside-unit = hệ thống & thư mục khác
stat-outside-hint = Dung lượng đã dùng của ổ mà lần quét này không bao gồm: tệp riêng của Windows không đọc được, và các thư mục nằm ngoài thư mục đã quét.

stat-free-on = Còn trống trên { $drive }
stat-safe-scan = từ lần quét gần nhất
stat-safe-junk = cache & tệp tạm
stat-last-scan = Lần quét gần nhất
stat-no-scan = chưa quét lần nào
stat-drives = Ổ đĩa
stat-drives-low = { $count ->
   *[other] { $count } ổ sắp đầy
}
stat-drives-ok = đều còn chỗ

## Home

home-free-btn = Giải phóng { $size } an toàn
home-drives-title = Ổ đĩa
home-drives-sub = Chọn một ổ để xem cái gì chiếm chỗ.

## Analyze

analyze-title = Bản đồ ổ đĩa · { $name }
explorer-title = Khám phá
explorer-sub = Cái gì chiếm chỗ trong { $name }, theo dung lượng
explorer-lens-map = Bản đồ
explorer-lens-folders = Thư mục
explorer-mapped = Đã vẽ { $size }
explorer-color-type = Loại tệp
explorer-color-cleanup = Dọn dẹp
legend-safe = Dọn an toàn
explorer-idle = Nhấp một ô hoặc một dòng để chọn · nhấp đúp để mở thư mục
details-show = Hiện chi tiết
details-hide = Ẩn chi tiết

## Recommendations

recs-title = Gợi ý
recs-sub = Tích các mục muốn dọn. Không gì bị xóa trước khi bạn xác nhận.
recs-clean-n = Dọn { $count } mục đã chọn
recs-details = Chi tiết
recs-copy = Sao chép danh sách
recs-copy-hint = Sao chép các mục đã tích (tên, dung lượng, vị trí) dạng văn bản. Không xóa gì.
recs-copied = { $count ->
   *[other] Đã sao chép { $count } mục
}

## Selection footer

sel-compact = Đã chọn { $size }
sel-compact-items = { $count ->
   *[other] { $count } mục
}
sel-none-short = Chưa chọn gì
sel-clear-short = Bỏ chọn
sel-select-safe = Chọn mục an toàn

## Clean

stat-checked = Kiểm tra lần cuối
stat-rules = { $count ->
   *[other] { $count } quy tắc tìm thấy
}

## Cleanup history

stat-freed = Đã giải phóng
stat-operations = { $count ->
   *[other] qua { $count } lần dọn
}
stat-restorable = Có thể khôi phục
stat-restorable-unit = mục, từ khu cách ly hoặc Thùng rác

## Tidy up

stat-of-folders = trên { $total } thư mục
stat-checked-n = đã kiểm tra { $count }
stat-listed-only = chỉ liệt kê, xóa trong Explorer

## Dev caches

stat-kept = Giữ lại
stat-kept-unit = thư mục được giữ có chủ ý

## Duplicates

stat-wasted = Có thể giải phóng
stat-in-groups = { $count ->
   *[other] trong { $count } nhóm tệp giống hệt nhau
}
stat-compared = Tệp đã so sánh
stat-compared-unit = trong { $seconds } giây
