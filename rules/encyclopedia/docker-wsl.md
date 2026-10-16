+++
id = "docker-wsl"
risk = "advanced"
paths = ["${LOCALAPPDATA}/Docker/wsl"]
+++

## en
### Docker Desktop disk image

Docker Desktop's WSL 2 virtual disk (`ext4.vhdx`) holding every image, container, volume and build cache. The file grows but never shrinks on its own.

**If you delete it:** all images, containers and **volumes (database data!)** are gone; Docker Desktop recreates an empty disk.

**Instead:** `docker system df` shows what uses the space; `docker system prune` (add `--volumes` only if you are sure) frees it, then compact the disk: `wsl --shutdown` and `Optimize-VHD` (Hyper-V) or diskpart `compact vdisk`.

## vi
### Ổ đĩa ảo của Docker Desktop

Ổ đĩa ảo WSL 2 của Docker Desktop (`ext4.vhdx`) chứa mọi image, container, volume và build cache. Tệp chỉ phình ra, không tự co lại.

**Nếu xóa:** mất toàn bộ image, container và **volume (dữ liệu database!)**; Docker Desktop tạo lại ổ trống.

**Nên làm:** `docker system df` cho biết cái gì chiếm chỗ; `docker system prune` (chỉ thêm `--volumes` khi chắc chắn) để giải phóng, rồi nén ổ: `wsl --shutdown` và `Optimize-VHD` (Hyper-V) hoặc diskpart `compact vdisk`.
