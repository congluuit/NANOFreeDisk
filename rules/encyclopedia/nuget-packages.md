+++
id = "nuget-packages"
risk = "caution"
paths = ["${HOME}/.nuget/packages"]
+++

## en
### .NET NuGet packages

The global NuGet folder: every package version restored by any .NET project on this PC. Projects reference packages here instead of keeping their own copy.

**If you delete it:** nothing is lost, but every project has to restore its packages again before it builds (needs internet).

**Instead:** `dotnet nuget locals global-packages --clear` (or `--clear all`) does it the supported way; close Visual Studio and Rider first.

## vi
### Gói NuGet của .NET

Thư mục NuGet toàn cục: mọi phiên bản gói mà các project .NET trên máy đã restore. Project tham chiếu gói ở đây thay vì giữ bản riêng.

**Nếu xóa:** không mất gì, nhưng mọi project phải restore gói lại trước khi build (cần mạng).

**Nên làm:** `dotnet nuget locals global-packages --clear` (hoặc `--clear all`) là cách đúng; đóng Visual Studio và Rider trước.
