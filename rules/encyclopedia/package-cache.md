+++
id = "package-cache"
risk = "advanced"
paths = ["${PROGRAMDATA}/Package Cache"]
+++

## en
### Installer package cache

Visual Studio, .NET, SQL Server and other WiX/Burn installers keep their packages here so they can repair, modify or uninstall themselves without the original download.

**If you delete it:** "Modify", "Repair" and sometimes "Uninstall" of those products fail or ask for the original media.

**Instead:** remove Visual Studio workloads or old SDK/runtime versions you do not need with their own installer (Visual Studio Installer, `dotnet --list-sdks` + Apps & features); their cached packages are removed too.

## vi
### Bộ đệm gói cài đặt

Visual Studio, .NET, SQL Server và các bộ cài WiX/Burn khác lưu gói cài ở đây để tự sửa, thay đổi hoặc gỡ cài đặt mà không cần tải lại.

**Nếu xóa:** "Modify", "Repair" và đôi khi cả "Uninstall" của các sản phẩm đó sẽ lỗi hoặc đòi bộ cài gốc.

**Nên làm:** gỡ workload Visual Studio hay các phiên bản SDK/runtime cũ không cần bằng chính bộ cài của chúng (Visual Studio Installer, `dotnet --list-sdks` + Apps & features); gói lưu sẵn sẽ được xóa theo.
