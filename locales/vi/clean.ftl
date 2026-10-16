# Màn hình Dọn dẹp, Nhật ký hoạt động và bộ máy an toàn (Phase 3). Nạp cùng app.ftl;
# mọi key phải có trong en/clean.ftl.

nav-activity = Hoạt động

## Dọn dẹp
clean-title = Dọn dẹp
clean-intro = Rác và bộ nhớ đệm mà ứng dụng tự tạo lại, tìm theo các gói quy tắc. Không gì bị xóa thẳng: các mục được chuyển vào khu cách ly và có thể khôi phục trong Hoạt động suốt 7 ngày.
clean-activity-hint = Mọi thứ đã dọn hoặc xóa, kèm nút Khôi phục
clean-rescan = Kiểm tra lại
clean-scanning = Đang tìm rác… (chỉ đọc)
clean-not-scanned = Chưa kiểm tra.
clean-nothing = Hiện không có gì để dọn.
clean-checked-at = Kiểm tra lúc { $date }
clean-level-safe = An toàn
clean-level-caution = Thận trọng
clean-level-advanced = Nâng cao
clean-level-protected = Được bảo vệ
clean-safe-button = Dọn an toàn ({ $size })
clean-safe-hint = Nếu bấm: mọi mục An toàn được chuyển vào khu cách ly. Ứng dụng tự tạo lại chúng; bạn có thể khôi phục trong Hoạt động suốt 7 ngày.
clean-pick-hint = Gồm quy tắc này
clean-if-deleted = Nếu xóa thì:
clean-close-first = { $apps } đang chạy — hãy đóng trước, nếu không các tệp nó đang mở sẽ được bỏ qua.
clean-files = { $count } tệp
clean-preview = Xem trước
clean-hide = Ẩn
clean-preview-more = Đang hiện { $shown } mục lớn nhất trong { $count } mục.
clean-guide-title = Chỉ hướng dẫn ({ $count }) — ứng dụng không bao giờ xóa các mục này
clean-selected = Đã chọn { $count } quy tắc · { $size }
clean-run = Dọn mục đã chọn
clean-run-hint = Nếu bấm: bạn xem tổng dung lượng trước, rồi các mục đã chọn được chuyển vào khu cách ly.
clean-confirm-title = Dọn các mục này?
clean-confirm-body = { $size } từ { $rules } quy tắc sẽ được chuyển vào khu cách ly.
clean-confirm-note = Mỗi mục được kiểm tra lại ngay trước khi chuyển. Bạn có thể khôi phục tất cả trong Hoạt động suốt 7 ngày; sau đó khu cách ly được dọn trống.
clean-confirm-permanent-note = Xóa vĩnh viễn: sau đó không khôi phục được gì.
clean-confirm-running = Một số ứng dụng vẫn đang chạy; các tệp chúng đang mở sẽ được giữ nguyên.
clean-permanent = Xóa vĩnh viễn thay vào đó (Expert, chỉ mục An toàn)
clean-type-delete = Gõ DELETE để xác nhận:
clean-confirm-go = Chuyển vào khu cách ly
clean-confirm-permanent = Xóa vĩnh viễn
clean-running = Đang dọn… { $done } / { $total }
clean-freed-so-far = Đã giải phóng { $size }
clean-cancel = Dừng
clean-done-title = Đã giải phóng { $size }
clean-done-detail = { $done } xong, { $partial } một phần (tệp đang dùng), { $skipped } bỏ qua, { $failed } lỗi.
clean-was-cancelled = đã dừng sớm
clean-problems = { $count } mục chưa được dọn hết
clean-undo = Hoàn tác trong Hoạt động
clean-dismiss = Đóng
clean-result = Đã giải phóng { $size } · dọn { $done } mục, bỏ qua { $skipped }.
clean-journal-failed = Đã dọn, nhưng không ghi được nhật ký hoàn tác ({ $error }). Các mục trong khu cách ly được liệt kê trong từng manifest.jsonl.
clean-status-done = xong
clean-status-partial = một phần — có tệp đang được dùng
clean-status-skipped = bỏ qua
clean-status-failed = lỗi
clean-status-declined = đã từ chối
clean-status-cancelled = chưa làm (đã dừng)
clean-status-not-selected = không chọn

## Lý do một mục bị bỏ qua (mã của SafetyGate; mã của Phase 2 nằm trong app.ftl)
reason-stream = Không bao giờ xóa luồng dữ liệu phụ (alternate data stream).
reason-device = Đường dẫn thiết bị và tên dành riêng (NUL, COM1…) bị từ chối.
reason-cross-volume = Một liên kết trong đường dẫn trỏ sang ổ khác.
reason-system-file = Tệp hệ thống ở nơi Windows hoặc chương trình lưu tệp.
reason-system-binary = Tệp chương trình nằm trong thư mục Windows.
reason-cloud-file = Tệp giữ chỗ của đám mây: xóa nó sẽ xóa luôn bản trên đám mây.
reason-changed = Đã thay đổi kể từ lúc kiểm tra; bỏ qua cho an toàn.
reason-protected-level = Quy tắc đánh dấu vị trí này được bảo vệ.
reason-nested = Đã được tính qua thư mục cha.
reason-in-use = Có tệp đang được một chương trình sử dụng.
reason-notfound = Mục này đã không còn.
reason-permissiondenied = Bị từ chối truy cập (có thể cần quyền quản trị).
reason-unknown = Chưa xóa ({ $code }).

## Nhật ký hoạt động
activity-title = Hoạt động
activity-intro = Mọi thứ đã dọn hoặc xóa trong ứng dụng, theo ngày. Mục trong khu cách ly khôi phục được trong 7 ngày, mục trong Thùng rác cho tới khi Thùng rác được dọn.
activity-empty = Chưa dọn hay xóa gì.
activity-kind-clean = Dọn dẹp
activity-kind-manual = Chuyển vào Thùng rác
activity-kind-tidy = Dọn gọn
activity-items = { $count } mục
activity-freed = giải phóng { $size }
activity-state-active = đang giữ
activity-state-restored = đã khôi phục
activity-state-partly-restored = đã khôi phục một phần
activity-state-purged = đã dọn khu cách ly
activity-details = Chi tiết
activity-restore = Khôi phục ({ $count })
activity-restore-hint = Nếu bấm: các mục trở về chỗ cũ, giữ nguyên ngày giờ. Không ghi đè thứ gì đang có ở đó.
activity-restore-one = Khôi phục mục này
activity-purge-hint = Dọn trống khu cách ly này ngay (không hoàn tác được)
activity-purge-confirm = Xóa vĩnh viễn { $size } khỏi khu cách ly? Không thể hoàn tác.
activity-purged = Đã dọn trống khu cách ly.
activity-restored = Đã khôi phục { $done } mục{ $failed ->
    [0] .
   *[other] ; { $failed } mục không khôi phục được.
}
activity-restore-failed = không khôi phục được
activity-restore-conflicts = đã có thứ cùng tên ở đó; vẫn giữ trong khu cách ly
activity-restore-missing = không còn trong khu cách ly hay Thùng rác
activity-restore-not-restorable = đã xóa vĩnh viễn
activity-item-restored = đã khôi phục
activity-mode-recycle-bin = Thùng rác
activity-mode-quarantine = khu cách ly
activity-mode-permanent = vĩnh viễn

## Trang chủ và Điểm sức khỏe
home-free-checking = Đang tìm rác…
home-free-safely = Giải phóng { $size } an toàn ngay
home-free-nothing = Không có rác để dọn
home-free-sub = Bộ nhớ đệm và tệp tạm mà ứng dụng tự tạo lại — khôi phục được trong 7 ngày.
home-free-hint = Nếu bấm: màn hình Dọn dẹp mở ra và hỏi trước khi chuyển bất cứ thứ gì.
health-junk = Có thể dọn an toàn { $size } rác.
health-fix-clean = Dọn dẹp

## Dọn gọn và hộp thoại xóa
tidy-quarantine-note = Các mục đã chọn được chuyển vào khu cách ly và khôi phục được trong Hoạt động suốt 7 ngày.
tidy-selected = Đã chọn { $count }
tidy-quarantine = Chuyển vào khu cách ly ({ $count })
tidy-quarantine-hint = Nếu bấm: bạn xác nhận trước; các mục được chuyển vào khu cách ly (khôi phục được trong 7 ngày).
delete-title-quarantine = Chuyển vào khu cách ly?
delete-intro-quarantine = Các mục sau sẽ được chuyển vào khu cách ly:
delete-quarantine-note = Khôi phục được trong Hoạt động suốt 7 ngày. Mỗi mục được kiểm tra lại ngay trước khi chuyển.
delete-confirm-quarantine = Chuyển vào khu cách ly
