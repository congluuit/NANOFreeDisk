+++
id = "hiberfil"
risk = "protected"
volume_paths = ["hiberfil.sys"]
+++

## en
### Hibernation file

Holds the contents of memory when the PC hibernates, and powers **Fast Startup**. Its size is a large share of your RAM.

**If you delete it:** you cannot while hibernation is on; Windows recreates it.

**Instead:** if you never hibernate, run `powercfg /h off` in an administrator terminal: the file disappears and Fast Startup is turned off (boot can be a few seconds slower). `powercfg /h /type reduced` keeps Fast Startup with a smaller file.

## vi
### Tệp ngủ đông (hibernation)

Lưu nội dung bộ nhớ khi máy ngủ đông và phục vụ **Fast Startup**. Kích thước bằng một phần lớn dung lượng RAM.

**Nếu xóa:** không xóa được khi chế độ ngủ đông đang bật; Windows sẽ tạo lại.

**Nên làm:** nếu không bao giờ dùng ngủ đông, chạy `powercfg /h off` trong terminal quyền admin: tệp biến mất và Fast Startup bị tắt (khởi động có thể chậm hơn vài giây). `powercfg /h /type reduced` giữ Fast Startup với tệp nhỏ hơn.
