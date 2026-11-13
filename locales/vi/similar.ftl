# Ảnh giống nhau trong màn hình Tệp trùng (docs/SIMILAR_PHOTOS.md). Mọi khóa phải
# có trong en/similar.ftl.

similar-mode-label = Tìm gì
similar-mode-identical = Tệp giống hệt
similar-mode-identical-hint = Các tệp có nội dung giống nhau đến từng byte. Mỗi nhóm luôn giữ lại một bản.
similar-mode-similar = Ảnh giống nhau
similar-mode-similar-hint = Ảnh trông như nhau nhưng không cùng một tệp: đổi kích thước, gửi qua ứng dụng chat, xuất hai lần hoặc cắt bớt một chút.

similar-notice-title = Đây là những ảnh trông giống nhau, không phải bản sao giống hệt.
similar-notice-body = Không có ảnh nào được chọn sẵn. Hãy mở từng nhóm, so sánh và chỉ đánh dấu những ảnh bạn thật sự không cần. Ảnh tốt nhất của mỗi nhóm đã được đánh dấu.

similar-find-mine = Tìm ảnh giống nhau trong tệp của tôi
similar-find-mine-again = Tìm lại trong tệp của tôi
similar-find-mine-hint = Nếu bấm: các thư mục Tài liệu, Tải về, Ảnh, Video, Nhạc và Màn hình nền được đọc và so sánh, không thay đổi gì. Tệp đám mây chỉ trực tuyến không bao giờ bị tải về.
similar-choose = Chọn thư mục hoặc ổ đĩa
similar-choose-hint = Nếu bấm: bạn tự chọn thư mục hoặc ổ đĩa cần so sánh thay cho các thư mục cá nhân.
similar-use-scan = Dùng lần quét trong Phân tích
similar-find = Tìm ảnh giống nhau

similar-preset = Mức giống
similar-preset-strict = Chặt
similar-preset-normal = Vừa
similar-preset-loose = Rộng
similar-preset-strict-hint = Chặt: cùng một ảnh ở kích thước khác, bị ứng dụng chat nén lại, xuất hai lần hoặc khác thông tin đi kèm. Lựa chọn an toàn nhất.
similar-preset-normal-hint = Vừa: thêm cả ảnh nén mạnh hơn, chỉnh sửa nhỏ và cắt bớt một chút. Hãy xem từng nhóm trước khi xóa.
similar-preset-loose-hint = Rộng: thêm cả ảnh cắt hoặc chỉnh sửa thấy rõ. Các ảnh chụp liên tiếp có thể vào cùng nhóm, nên hãy kiểm tra từng ảnh.

similar-phase-scan = Đang liệt kê tệp… đã thấy { $files }
similar-phase-collect = Đang tìm ảnh…
similar-phase-hash = Đang đọc ảnh: { $done } / { $total }
similar-phase-compare = Đang so sánh ảnh: { $done } / { $total }
similar-cancelled = Đã hủy tìm ảnh giống nhau.

similar-headline = Các ảnh trông giống một ảnh tốt hơn chiếm { $size }, trong { $count } nhóm
similar-headline-note = Giống nhau không có nghĩa là giống hệt: không có ảnh nào được chọn sẵn. Hãy đánh dấu ảnh muốn xóa, hoặc dùng "Chỉ giữ ảnh tốt nhất" trong từng nhóm.
similar-headline-none = Không tìm thấy ảnh nào trông giống nhau.
similar-stats = Đã so sánh { $images } ảnh, đọc mới { $decoded }, { $cached } lấy từ bộ nhớ đệm, { $seconds } giây.
similar-none = Không có ảnh giống nhau ở mức này. Chọn mức rộng hơn có thể tìm thấy thêm.

similar-not-compared = Không so sánh: { $count } tệp
similar-skip-small = Quá nhỏ
similar-skip-small-why = Ảnh dưới { $size } là biểu tượng, nhãn dán và hình trang web, không phải ảnh chụp.
similar-skip-cloud = Chỉ trực tuyến
similar-skip-cloud-why = Tệp đám mây chỉ trực tuyến không bao giờ được mở, nên không bao giờ bị tải về.
similar-skip-reparse = Liên kết
similar-skip-reparse-why = Lối tắt và liên kết tới nơi khác không được đi theo.
similar-skip-system = Tệp hệ thống
similar-skip-system-why = Tệp được Windows đánh dấu là tệp hệ thống thì không đụng tới.
similar-skip-protected = Được bảo vệ
similar-skip-protected-why = Những nơi NANOFreeDisk không bao giờ xóa (thư mục hệ thống, chương trình, thông tin đăng nhập, lịch sử hội thoại với AI).
similar-skip-app-dir = Trong thư mục ứng dụng
similar-skip-app-dir-why = Ảnh trong chương trình và gói phần mềm là của chúng. Chế độ Chuyên gia có thể liệt kê, nhưng không bao giờ chọn.
similar-skip-unreadable = Không đọc được
similar-skip-unreadable-why = Tệp hỏng, bị cắt dở, đang bị chương trình khác khóa, hoặc là loại ảnh phiên bản này chưa đọc được (ảnh HEIC, RAW và TIFF chưa được so sánh).
similar-skip-changed = Thay đổi trong lúc đọc
similar-skip-changed-why = Tệp thay đổi trong lúc được đọc, hãy tìm lại sau.
similar-skip-too-large = Quá lớn
similar-skip-too-large-why = Trên 64 MB hoặc trên 100 megapixel: được bỏ qua để việc tìm kiếm nhẹ nhàng.
similar-skip-plain = Không có chi tiết
similar-skip-plain-why = Ảnh một màu hay trang giấy trắng thì không có gì để so sánh.

similar-group-title = { $count } ảnh trông giống nhau
similar-best-pixels = Ảnh tốt nhất có nhiều điểm ảnh nhất.
similar-best-size = Cùng kích thước điểm ảnh: ảnh tốt nhất là tệp lớn nhất.
similar-best-place = Cùng ảnh và cùng dung lượng: ảnh tốt nhất là ảnh nằm ngoài Tải về, Màn hình nền và Temp.
similar-group-picked = Đã đánh dấu { $count }
similar-extra-hint = Dung lượng của các ảnh ngoài ảnh tốt nhất (bản sao giống hệt được tính ở mục Tệp giống hệt).
similar-all-but-best = Chỉ giữ ảnh tốt nhất
similar-all-but-best-hint = Nếu bấm: mọi ảnh trong nhóm này trừ ảnh tốt nhất được đánh dấu để xóa. Ảnh trong thư mục đám mây, git, ứng dụng hoặc chỉ đọc vẫn không được đánh dấu.
similar-untick = Bỏ đánh dấu
similar-untick-all = Bỏ đánh dấu tất cả
similar-best = Tốt nhất
similar-remove = Xóa
similar-tile-tick = Nếu bấm: ảnh này được đánh dấu để xóa.
similar-tile-untick = Nếu bấm: ảnh này được giữ lại.
similar-mp = { $mp } MP
similar-identical = Bản sao giống hệt
similar-identical-hint = Giống từng byte với tệp { $name }.
similar-compare = So sánh cạnh nhau
similar-compare-title = So sánh
similar-close = Đóng
similar-all-ticked = Mọi ảnh trong nhóm này đều được đánh dấu, nên sẽ không còn ảnh nào được giữ lại.
similar-keep-none = Xóa mọi ảnh trong nhóm này, tôi không cần ảnh nào

similar-picked = Đã đánh dấu { $size }: { $count } ảnh
similar-blocked = { $count } nhóm có mọi ảnh đều được đánh dấu: hãy đánh dấu "Xóa mọi ảnh" trong nhóm đó hoặc bỏ đánh dấu một ảnh.
similar-confirm-recycle = Chuyển các ảnh này vào Thùng rác?
similar-confirm-quarantine = Chuyển các ảnh này vào khu cách ly?
similar-confirm-body = { $files } ảnh từ { $groups } nhóm, { $size }.
similar-confirm-note = Đây là các ảnh trông giống nhau do chính bạn chọn. Mỗi ảnh được kiểm tra lại ngay trước khi chuyển; ảnh nào đã thay đổi hoặc chỉ còn trên đám mây sẽ được bỏ qua.
similar-confirm-emptied = { $count } nhóm sẽ không còn ảnh nào, như bạn đã xác nhận.

activity-kind-similar = Đã xóa ảnh giống nhau
reason-not-similar = Không phải ảnh bạn đã đánh dấu, nên được giữ nguyên.
