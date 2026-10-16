+++
id = "git-folder"
risk = "advanced"
names = [".git"]
+++

## en
### Git repository data (.git)

The whole history of this project: every commit, branch, stash and un-pushed work, plus its configuration. The working files you see are only the current checkout.

**If you delete it:** the folder stops being a repository; anything not pushed to a remote — commits, branches, stashes — is lost for good.

**Instead:** `git gc --aggressive --prune=now` shrinks it; `git lfs prune` for LFS projects; check `git status` and `git log --branches --not --remotes` before removing a repository you think is pushed.

## vi
### Dữ liệu kho Git (.git)

Toàn bộ lịch sử của project: mọi commit, nhánh, stash và thay đổi chưa push, cùng cấu hình. Các tệp bạn thấy chỉ là bản checkout hiện tại.

**Nếu xóa:** thư mục không còn là repository; mọi thứ chưa push lên remote — commit, nhánh, stash — mất vĩnh viễn.

**Nên làm:** `git gc --aggressive --prune=now` để thu nhỏ; `git lfs prune` với project dùng LFS; kiểm tra `git status` và `git log --branches --not --remotes` trước khi xóa một repo bạn nghĩ đã push hết.
