+++
id = "pycache"
risk = "safe"
names = ["__pycache__"]
+++

## en
### Python bytecode cache (__pycache__)

Compiled `.pyc` files Python writes next to your modules so imports start faster. They are recreated automatically whenever the source changes.

**If you delete it:** nothing is lost; the next run is a little slower while Python compiles again.

**Instead:** set `PYTHONDONTWRITEBYTECODE=1` if you do not want them at all; virtual environments (`.venv`) are usually much bigger and can be recreated from `requirements.txt` or the lock file.

## vi
### Bộ đệm bytecode Python (__pycache__)

Các tệp `.pyc` đã biên dịch mà Python ghi cạnh module để import nhanh hơn. Tự tạo lại mỗi khi mã nguồn thay đổi.

**Nếu xóa:** không mất gì; lần chạy sau chậm hơn chút trong lúc Python biên dịch lại.

**Nên làm:** đặt `PYTHONDONTWRITEBYTECODE=1` nếu không muốn có chúng; môi trường ảo (`.venv`) thường lớn hơn nhiều và tạo lại được từ `requirements.txt` hoặc file lock.
