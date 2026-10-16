+++
id = "rust-target"
risk = "safe"
names = ["target"]
siblings = ["Cargo.toml"]
+++

## en
### Rust build output (target)

Everything `cargo build`, `cargo test` and `rust-analyzer` produce for this project: compiled dependencies, incremental caches and binaries. Debug builds of medium projects easily reach several GB.

**If you delete it:** nothing is lost; the next build compiles everything from scratch (slow once).

**Instead:** run `cargo clean` in the project, or `cargo sweep` to drop only artifacts older than N days across projects.

## vi
### Kết quả build Rust (target)

Mọi thứ `cargo build`, `cargo test` và `rust-analyzer` tạo ra cho project: dependency đã biên dịch, cache incremental và file chạy. Bản debug của project cỡ vừa dễ dàng lên vài GB.

**Nếu xóa:** không mất gì; lần build sau biên dịch lại từ đầu (chậm một lần).

**Nên làm:** chạy `cargo clean` trong project, hoặc `cargo sweep` để chỉ xóa artifact cũ hơn N ngày trên nhiều project.
