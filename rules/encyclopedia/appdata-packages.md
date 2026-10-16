+++
id = "appdata-packages"
risk = "advanced"
paths = ["${LOCALAPPDATA}/Packages"]
+++

## en
### Microsoft Store app data

One folder per Store/MSIX app (Photos, Teams, WhatsApp, Xbox, Windows Terminal…) holding its settings, sign-ins, caches and sometimes downloaded media. Folder names look like `Microsoft.WindowsTerminal_8wekyb3d8bbwe`.

**If you delete it:** those apps lose their settings and sign-in, may not start until reinstalled, and local-only data (chats, offline media) is gone.

**Instead:** open Settings → Apps → the app → Advanced options → **Reset**, or clear the app's cache from inside the app. Look inside for a `LocalCache` or `TempState` folder: those are the usual space hogs.

## vi
### Dữ liệu ứng dụng Microsoft Store

Mỗi ứng dụng Store/MSIX (Photos, Teams, WhatsApp, Xbox, Windows Terminal…) có một thư mục chứa cài đặt, phiên đăng nhập, cache và đôi khi cả media đã tải. Tên thư mục dạng `Microsoft.WindowsTerminal_8wekyb3d8bbwe`.

**Nếu xóa:** các ứng dụng mất cài đặt và đăng nhập, có thể không mở được cho tới khi cài lại, dữ liệu chỉ lưu trên máy (tin nhắn, media offline) mất luôn.

**Nên làm:** vào Settings → Apps → ứng dụng → Advanced options → **Reset**, hoặc xóa cache ngay trong ứng dụng. Thư mục `LocalCache` hay `TempState` bên trong thường là thứ chiếm chỗ.
