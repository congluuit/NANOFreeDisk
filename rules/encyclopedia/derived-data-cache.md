+++
id = "derived-data-cache"
risk = "caution"
names = ["DerivedDataCache"]
+++

## en
### Unreal Engine Derived Data Cache

Compiled shaders and cooked asset data that Unreal Engine builds from your content (per project, or shared under `%LOCALAPPDATA%\UnrealEngine\Common\DerivedDataCache`).

**If you delete it:** nothing is lost; the editor recompiles shaders and rebuilds data the next time, which makes the first launch slow.

**Instead:** close the editor first; for shared caches set a size limit in `DefaultEngine.ini` or point it to a bigger drive.

## vi
### Derived Data Cache của Unreal Engine

Shader đã biên dịch và dữ liệu asset đã xử lý mà Unreal Engine tạo từ nội dung của bạn (theo từng project, hoặc dùng chung ở `%LOCALAPPDATA%\UnrealEngine\Common\DerivedDataCache`).

**Nếu xóa:** không mất gì; editor sẽ biên dịch lại shader và tạo lại dữ liệu, lần mở đầu tiên sẽ chậm.

**Nên làm:** đóng editor trước; với cache dùng chung hãy đặt giới hạn dung lượng trong `DefaultEngine.ini` hoặc chuyển sang ổ lớn hơn.
