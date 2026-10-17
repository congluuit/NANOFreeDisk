# Cache lập trình → AI leftovers (PLAN §13.2 C10, docs/AI_LEFTOVERS.md). Mọi key phải
# có trong en/ai-leftovers.ftl.

dev-tab-ai = Rác của AI
ail-intro = Những gì AI coding agent để lại theo từng phiên: worktree, scratchpad, snapshot trình duyệt, ảnh chụp, kết quả test, trình duyệt Playwright cũ. Cuộc hội thoại luôn được giữ. Rác của phiên còn mở trong ứng dụng Claude hoặc hoạt động trong { $days } ngày gần đây được liệt kê nhưng không chọn sẵn.
ail-rescan = Quét lại
ail-rescan-hint = Tìm lại rác của AI (chỉ đọc: không xóa gì).
ail-finished = Dọn phiên đã xong (chỉ an toàn) · { $size }
ail-finished-hint = Chỉ chọn rác 🟢 của các phiên đã xong, rồi hỏi lại trước khi làm. Thư mục vào quarantine (khôi phục được trong 7 ngày); worktree được git gỡ, nhánh vẫn giữ nguyên.
ail-loading = Đang tìm rác của AI… (chỉ đọc)
ail-summary = { $sessions } phiên, { $projects } thư mục dự án · có thể dọn { $size }.
ail-no-git = Không tìm thấy git: worktree và thư mục tên chung (test-results, screenshots…) không được đề xuất.
ail-no-project = Temp & kho của công cụ
ail-session = Từ phiên “{ $title }”, hoạt động { $days ->
        [0] hôm nay
       *[other] { $days } ngày trước
    }
ail-session-untitled = Phiên { $tool } { $id }
ail-open = đang mở
ail-open-hint = Phiên vẫn đang mở (chưa archive) trong ứng dụng Claude và có thể được tiếp tục: những gì nó dùng không được chọn sẵn.
ail-wt-orphan = Không còn là worktree mà git biết: các tệp trong đó không nằm trong git.
ail-wt-prunable = Thư mục đã mất; chỉ còn bản ghi của git (git worktree prune).
ail-wt-merged = đã merge vào { $branch }
ail-wt-pushed = đã push
ail-wt-unmerged = chưa merge, chưa push
ail-wt-detached = (detached)
ail-wt-changes = { $count ->
        [0] không có thay đổi
       *[other] { $count } thay đổi
    }
ail-wt-ignored = { $size } bị git bỏ qua (vào quarantine trước)
ail-wt-artifacts = Không gỡ được, nhưng có thể dọn build output của nó:
ail-keep-session-open = Giữ lại: phiên của nó vẫn đang mở trong ứng dụng Claude.
ail-keep-session-recent = Giữ lại: phiên của nó hoạt động trong { $days } ngày gần đây.
ail-keep-recent-write = Giữ lại: được ghi trong { $days } ngày gần đây.
ail-kept = { $what } ({ $tool }): lịch sử hội thoại, luôn được giữ.
ail-none = Không tìm thấy rác của AI.
ail-clean-hint = Chuyển các thư mục đã tích vào quarantine và gỡ các worktree đã tích bằng git — sau hộp xác nhận cho thấy chính xác lệnh sẽ chạy.
ail-confirm-title = Dọn rác của AI?
ail-confirm-body = { $count } thư mục vào quarantine; giải phóng tổng cộng { $size }.
ail-confirm-git = Sau đó git chạy { $count ->
        [one] lệnh này
       *[other] các lệnh này
    } (mỗi worktree được kiểm tra lại ngay trước khi gỡ):
ail-confirm-note = Thư mục khôi phục được trong Hoạt động trong 7 ngày. Worktree đã gỡ được tạo lại bằng “git worktree add” trên nhánh của nó — nhánh không bao giờ bị xóa. Cuộc hội thoại không bao giờ bị động tới.

reason-tracked = Có tệp được git theo dõi bên trong.
reason-contains-history = Chứa lịch sử hội thoại AI, thứ không bao giờ bị xóa.
reason-too-large = Quá nhiều tệp để kiểm tra hết.
reason-no-git = Cần git nhưng không tìm thấy.
reason-git-failed = git không trả lời được cho mục này.
reason-dirty = Có thay đổi chưa commit hoặc tệp chưa được theo dõi.
reason-not-merged = Nhánh chưa được merge vào nhánh mặc định và chưa được push.
reason-locked = Worktree đang bị khóa.
reason-too-many-ignored = Quá nhiều tệp bị bỏ qua để liệt kê hết.
reason-foreign-worktree = Worktree của một repository khác.
reason-ignored-left = Một số tệp bị bỏ qua không chuyển được vào quarantine nên git không gỡ worktree.
reason-session-live = Phiên của nó hoạt động trong 24 giờ qua và có thể vẫn đang làm việc: không động tới.
reason-not-a-worktree = git không còn liệt kê nó là worktree.

activity-kind-ai-leftovers = Đã dọn rác của AI
activity-kind-worktree = Đã gỡ worktree (git)

health-ai-leftovers = { $size } do AI agent để lại trong các phiên đã xong.
health-fix-devcache = Xem rác của AI

home-ai-title = Rác của AI
home-ai-body = Có thể dọn an toàn { $size } từ các phiên AI đã xong.
home-ai-open = Xem lại
