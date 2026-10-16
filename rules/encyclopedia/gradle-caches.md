+++
id = "gradle-caches"
risk = "caution"
paths = ["${HOME}/.gradle/caches", "${HOME}/.gradle/wrapper/dists"]
+++

## en
### Gradle caches (Android, Java, Kotlin)

Downloaded dependencies, build caches, transformed Android libraries and Gradle distributions for every Gradle project. Android Studio users often have 10–30 GB here.

**If you delete it:** the next build downloads everything again and the first build is much slower.

**Instead:** stop daemons with `gradle --stop` first; Gradle 8+ cleans unused entries automatically after 30 days, or delete only old `wrapper\dists\gradle-*` versions you no longer use.

## vi
### Bộ đệm Gradle (Android, Java, Kotlin)

Dependency đã tải, build cache, thư viện Android đã chuyển đổi và các bản Gradle của mọi project Gradle. Người dùng Android Studio thường có 10–30 GB ở đây.

**Nếu xóa:** lần build sau phải tải lại toàn bộ và chậm hơn nhiều.

**Nên làm:** dừng daemon bằng `gradle --stop` trước; Gradle 8+ tự xóa mục không dùng sau 30 ngày, hoặc chỉ xóa các bản `wrapper\dists\gradle-*` cũ không còn dùng.
