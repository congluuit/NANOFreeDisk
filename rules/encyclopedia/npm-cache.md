+++
id = "npm-cache"
risk = "caution"
paths = ["${LOCALAPPDATA}/npm-cache", "${APPDATA}/npm-cache"]
+++

## en
### npm cache

npm's content-addressed cache of every package tarball it has downloaded, used to install faster and offline. It is verified on use, so a damaged entry is simply fetched again.

**If you delete it:** installs download everything again; offline installs fail until the cache is refilled.

**Instead:** `npm cache verify` removes unused and broken entries; `npm cache clean --force` empties it the supported way.

## vi
### Bộ đệm npm

Bộ đệm của npm chứa mọi gói tarball đã tải, giúp cài nhanh hơn và cài offline. Được kiểm tra khi dùng, nên mục hỏng sẽ được tải lại.

**Nếu xóa:** lần cài sau phải tải lại toàn bộ; cài offline sẽ lỗi cho tới khi bộ đệm đầy lại.

**Nên làm:** `npm cache verify` bỏ mục không dùng và mục hỏng; `npm cache clean --force` xóa sạch đúng cách.
