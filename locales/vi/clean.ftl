# Màn hình Dọn dẹp, Nhật ký hoạt động và bộ máy an toàn (Phase 3). Nạp cùng app.ftl;
# mọi key phải có trong en/clean.ftl.

nav-activity = Lịch sử dọn dẹp

## Dọn dẹp
clean-title = Tệp rác
clean-intro = Bộ nhớ đệm và tệp tạm mà ứng dụng tự tạo lại.
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
clean-safe-hint = Nếu bấm: bạn xem tổng trước, rồi mọi mục An toàn được chuyển vào khu cách ly.
clean-pick-hint = Gồm quy tắc này
clean-if-deleted = Nếu xóa thì:
clean-close-first = { $apps } đang chạy. Hãy đóng trước, nếu không các tệp nó đang mở sẽ được bỏ qua.
clean-files = { $count } tệp
clean-preview = Xem trước
clean-hide = Ẩn
clean-preview-more = Đang hiện { $shown } mục lớn nhất trong { $count } mục.
clean-guide-title = Chỉ hướng dẫn ({ $count }): ứng dụng không bao giờ xóa các mục này
clean-run-hint = Nếu bấm: bạn xem tổng dung lượng trước, rồi các mục đã chọn được chuyển vào khu cách ly.
clean-confirm-title = Dọn các mục này?
clean-confirm-body = { $size } từ { $rules } quy tắc sẽ được chuyển vào khu cách ly.
clean-confirm-note = Mỗi mục được kiểm tra lại ngay trước khi chuyển.
clean-confirm-permanent-note = Xóa vĩnh viễn: sau đó không khôi phục được gì.
clean-permanent = Xóa vĩnh viễn thay vào đó (Chuyên gia, chỉ mục An toàn)
clean-type-delete = Gõ DELETE để xác nhận:
clean-confirm-go = Dọn
clean-confirm-permanent = Xóa vĩnh viễn
clean-running = Đang dọn… { $done } / { $total }
clean-freed-so-far = Đã dọn { $size }
clean-cancel = Dừng
clean-done-title = Đã giải phóng { $size }
clean-done-detail = { $done } xong, { $partial } một phần (tệp đang dùng), { $skipped } bỏ qua, { $failed } lỗi.
clean-was-cancelled = đã dừng sớm
clean-problems = { $count } mục chưa được dọn hết
clean-undo = Hoàn tác trong Lịch sử dọn dẹp
clean-dismiss = Đóng
clean-result = Đã dọn { $size } · { $done } mục xong, { $skipped } còn lại.
clean-journal-failed = Đã dọn, nhưng không ghi được nhật ký hoàn tác ({ $error }). Các mục trong khu cách ly được liệt kê trong từng manifest.jsonl.
clean-status-done = xong
clean-status-partial = một phần: có tệp đang được dùng
clean-status-skipped = bỏ qua
clean-status-failed = lỗi
clean-status-declined = đã từ chối
clean-status-cancelled = chưa làm (đã dừng)
clean-status-not-selected = không chọn

## Những gì một lần dọn còn để lại, và vì sao
clean-left-title = Những gì vẫn còn
clean-left-in-use-title = Đang được { $apps } dùng
clean-left-in-use-unknown = Đang được một chương trình hoặc Windows dùng
clean-left-in-use-body = Một chương trình đang chạy mở các tệp này nên Windows không cho ai di chuyển chúng. Hãy lưu công việc, đóng chương trình đó rồi thử lại. NANOFreeDisk không bao giờ tự đóng chương trình của bạn.
clean-left-retry = Tôi đã đóng, thử lại
clean-left-admin-title = Cần quyền quản trị
clean-left-admin-body = Windows chỉ cho quản trị viên xóa các tệp hệ thống này. NANOFreeDisk có thể xin Windows cấp quyền một lần và kiểm tra lại từng mục trước khi di chuyển bất cứ thứ gì.
clean-left-admin-button = Dọn với quyền quản trị
clean-left-other-title = Chưa xóa được vì lý do khác
clean-left-other-body = Các mục này được bỏ qua để giữ an toàn hoặc không xóa được. Lý do ghi cạnh từng mục.
clean-left-other-retry = Kiểm tra lại và thử lại
clean-left-size = { $size } vẫn nằm trên ổ đĩa
clean-left-more = và { $count } mục khác
clean-admin-title = Dọn với quyền quản trị?
clean-admin-body = Windows sẽ hỏi quyền quản trị một lần. Sau đó NANOFreeDisk tự nạp lại quy tắc của mình, kiểm tra lại từng mục và từ chối mọi thứ không có quy tắc nào bao gồm. Chỉ các mục sau được đụng tới:
clean-admin-where = Các mục bị xóa sẽ vào Thùng rác. Bạn có thể khôi phục từ đó; khi khôi phục, Windows sẽ hỏi quyền quản trị lần nữa.
clean-admin-where-permanent = Bạn đã chọn xóa vĩnh viễn: các mục thuộc quy tắc bộ nhớ đệm và tệp tạm thuần túy sẽ bị xóa hẳn và không khôi phục được. Mọi thứ khác vào Thùng rác.
clean-admin-go = Tiếp tục, Windows sẽ hỏi
clean-admin-declined = Bạn đã từ chối lời nhắc quyền quản trị. Không có gì thay đổi.
clean-admin-disabled = Lời nhắc quyền quản trị bị tắt trong lần chạy thử này.
clean-retry-nothing = Không còn gì để dọn: các mục đó đã tự biến mất.
clean-rule-admin = Cần quyền quản trị
clean-rule-admin-hint = Lần dọn bình thường bỏ qua những gì chỉ quản trị viên mới xóa được ở đây; bạn có thể dọn phần đó sau với một lời nhắc của Windows.
clean-tool-disk-cleanup = Mở Disk Cleanup
clean-tool-storage-settings = Mở cài đặt Lưu trữ
clean-tool-system-protection = Mở System Protection
clean-tool-performance-options = Mở Performance Options
clean-tool-hint = Mở công cụ của Windows dành cho việc này. NANOFreeDisk không bao giờ tự xóa các mục này.

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
reason-relative-path = Không phải đường dẫn đầy đủ thông thường; bị từ chối.
reason-unknown-rule = Không có quy tắc nào của ứng dụng bao gồm mục này; bị từ chối.
reason-not-admin-rule = Quy tắc của mục này không cho phép dọn bằng quyền quản trị.
reason-expert-only = Mục Nâng cao cần chế độ Chuyên gia.
reason-not-matched = Không có quy tắc nào bao gồm đường dẫn này; bị từ chối.
reason-kept = Một quy tắc Giữ lại bảo vệ vị trí này.
reason-not-found-now = Mục này không còn nữa, hoặc không quy tắc nào đề xuất nó nữa.
reason-path-changed = Một liên kết hoặc tên khác giờ trỏ sang nơi khác; để nguyên.
reason-unknown = Chưa xóa ({ $code }).

## Nhật ký hoạt động
activity-title = Lịch sử dọn dẹp
activity-intro = Mọi thứ ứng dụng đã dọn hoặc xóa, theo ngày. Mục trong khu cách ly khôi phục được trong 7 ngày, mục trong Thùng rác cho tới khi Thùng rác được dọn.
activity-empty = Chưa dọn hay xóa gì.
activity-kind-clean = Dọn dẹp
activity-kind-manual = Xóa thủ công
activity-kind-tidy = Dọn gọn
activity-kind-clean-admin = Dọn dẹp (quyền quản trị)
activity-items = { $count } mục
activity-freed = giải phóng { $size }
activity-state-active = đang giữ
activity-state-restored = đã khôi phục
activity-state-partly-restored = đã khôi phục một phần
activity-state-purged = đã xóa vĩnh viễn
activity-details = Chi tiết
activity-restore = Khôi phục ({ $count })
activity-restore-hint = Nếu bấm: các mục trở về chỗ cũ, giữ nguyên ngày giờ. Không ghi đè thứ gì đang có ở đó.
activity-restore-one = Khôi phục mục này
activity-purge = Xóa vĩnh viễn
activity-purge-hint = Xóa ngay những gì khu cách ly đang giữ của lần dọn này để lấy lại dung lượng (không hoàn tác được)
activity-purge-one = Xóa vĩnh viễn mục này
activity-purge-title = Xóa vĩnh viễn?
activity-purge-body = { $count } mục của lần dọn này ({ $size }) sẽ bị xóa vĩnh viễn. Sau đó không khôi phục được nữa.
activity-purge-one-body = Mục này ({ $size }) sẽ bị xóa vĩnh viễn. Sau đó không khôi phục được nữa.
activity-purged = Đã xóa vĩnh viễn { $count } mục: lấy lại { $size } trên ổ đĩa.
activity-purge-failed = { $count } mục không xóa được, vẫn nằm trong khu cách ly.
activity-in-quarantine = { $count } mục trong khu cách ly · vẫn chiếm { $size } trên ổ
activity-expires = { $days ->
    [0] tự xóa hôm nay
    [one] tự xóa ngày mai
   *[other] tự xóa sau { $days } ngày
}
activity-restored = Đã khôi phục { $done } mục{ $failed ->
    [0] .
   *[other] ; { $failed } mục không khôi phục được.
}
activity-restore-failed = không khôi phục được
activity-restore-conflicts = đã có thứ cùng tên ở đó; vẫn giữ trong khu cách ly
activity-restore-missing = không còn trong khu cách ly hay Thùng rác
activity-restore-not-restorable = đã xóa vĩnh viễn
activity-item-restored = đã khôi phục
activity-item-purged = đã xóa vĩnh viễn
activity-mode-recycle-bin = Thùng rác
activity-mode-quarantine = khu cách ly
activity-mode-permanent = vĩnh viễn

## Khu cách ly (Hoạt động)
quarantine-title = Khu cách ly
quarantine-intro = Mục đã xóa nằm chờ ở đây để bạn có thể khôi phục. Chúng vẫn chiếm dung lượng cho tới khi bị xóa vĩnh viễn sau { $days } ngày, hoặc ngay bây giờ nếu bạn cần chỗ trống.
quarantine-held = vẫn chiếm trên ổ đĩa
quarantine-items = { $ops ->
    [one] mục, từ 1 lần dọn
   *[other] mục, từ { $ops } lần dọn
}
quarantine-next = lần tự xóa tiếp theo
quarantine-none = Khu cách ly trống: không có gì đang chờ xóa.
quarantine-recycle-note = Mục chuyển vào Thùng rác không nằm trong khu cách ly: Windows giữ chúng cho tới khi Thùng rác được dọn.
quarantine-open = Mở thư mục
quarantine-open-hint = Mở thư mục khu cách ly trong Explorer (khôi phục từ đây, đừng tự di chuyển)
quarantine-empty = Dọn sạch khu cách ly
quarantine-empty-hint = Xóa vĩnh viễn mọi thứ trong khu cách ly để lấy lại dung lượng ngay
quarantine-empty-title = Dọn sạch khu cách ly?
quarantine-empty-body = { $count } mục ({ $size }) sẽ bị xóa vĩnh viễn. Sau đó không khôi phục được nữa. Mục trong Thùng rác không bị động tới.
quarantine-only = Chỉ hiện các lần dọn còn mục trong khu cách ly

## Trang chủ và Điểm sức khỏe
home-free-checking = Đang tìm rác…
home-free-safely = Giải phóng { $size } an toàn ngay
home-free-nothing = Không có rác để dọn
home-free-sub = Bộ nhớ đệm và tệp tạm mà ứng dụng tự tạo lại, khôi phục được trong 7 ngày.
home-free-hint = Nếu bấm: màn hình Dọn dẹp mở ra và hỏi trước khi chuyển bất cứ thứ gì.
health-junk = Có thể dọn an toàn { $size } rác.
health-fix-clean = Dọn dẹp

## Dọn gọn và hộp thoại xóa
tidy-quarantine-note = Các mục đã chọn được chuyển vào khu cách ly và khôi phục được trong Lịch sử dọn dẹp suốt 7 ngày.
tidy-quarantine-hint = Nếu bấm: bạn xác nhận trước; các mục được chuyển vào khu cách ly (khôi phục được trong 7 ngày).
delete-title-quarantine = Xóa các mục này?
delete-intro-quarantine = Các mục sau sẽ bị xóa:
delete-quarantine-note = Mỗi mục được kiểm tra lại ngay trước khi chuyển.
delete-confirm-quarantine = Xóa

## Mọi công cụ xóa: một ghi chú an toàn, một hộp xác nhận, một kết quả
## (SafetyNote, RemoveConfirm, RunResult; clean/destination.ts)
safety-note = Mục bị xóa được chuyển vào khu cách ly, một thư mục giữ an toàn, và khôi phục được trong Lịch sử dọn dẹp suốt 7 ngày.
safety-note-recycle = Mục bị xóa được chuyển vào Thùng rác và khôi phục được từ đó cho tới khi Thùng rác được dọn.
safety-note-permanent = Xóa vĩnh viễn: không hoàn tác được.
confirm-what = Những gì sẽ xóa
confirm-where = Chuyển tới đâu và cách hoàn tác
confirm-space-later = Dung lượng được giải phóng hẳn sau 7 ngày, hoặc ngay khi bạn xóa vĩnh viễn trong Lịch sử dọn dẹp.
confirm-big = Khu cách ly giữ { $size } này trên ổ trong 7 ngày. Bạn muốn chuyển chúng đi đâu?
confirm-choice-quarantine = Khu cách ly: hoàn tác được trong 7 ngày
confirm-choice-recycle = Thùng rác: dung lượng được giải phóng khi bạn dọn Thùng rác
run-title = Đã dọn { $size }
run-counts = { $done } xong, { $stayed } còn lại.
run-held = { $size } sẽ được giải phóng hẳn sau 7 ngày, hoặc ngay bây giờ trong Lịch sử dọn dẹp.
run-fallback = Một số mục vào Thùng rác vì ổ của chúng không có khu cách ly.
run-history = Xem trong Lịch sử dọn dẹp
run-stayed-title = Những gì còn lại, và vì sao
clean-show-details = Xem bên trong từng mục
clean-hide-details = Ẩn chi tiết
