+++
id = "cargo-registry"
risk = "caution"
paths = ["${HOME}/.cargo/registry", "${HOME}/.cargo/git"]
+++

## en
### Cargo registry and git cache (Rust)

Downloaded crate archives, their unpacked sources and git dependencies for every Rust project you built. Shared by all projects; it only grows.

**If you delete it:** nothing breaks, but the next build downloads and unpacks every dependency again (needs internet; offline builds fail).

**Instead:** `cargo install cargo-cache` then `cargo cache --autoclean` keeps what is needed; never delete `.cargo\bin` (your installed tools) or `config.toml` / `credentials.toml`.

## vi
### Registry và git cache của Cargo (Rust)

Các gói crate đã tải, mã nguồn đã giải nén và dependency git của mọi project Rust bạn từng build. Dùng chung cho mọi project và chỉ tăng lên.

**Nếu xóa:** không hỏng gì, nhưng lần build sau phải tải và giải nén lại mọi dependency (cần mạng; build offline sẽ lỗi).

**Nên làm:** `cargo install cargo-cache` rồi `cargo cache --autoclean` giữ lại phần cần; không bao giờ xóa `.cargo\bin` (công cụ đã cài) hay `config.toml` / `credentials.toml`.
