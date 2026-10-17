//! The sentences of the advice, in English and Vietnamese. Rule packs, the AI
//! leftovers catalog and the Folder Encyclopedia bring their own; these cover what
//! the engine itself concludes (build output by project type, system and personal
//! folders, old files…), so the CLI, the JSON and the app say the same thing.

use std::sync::Arc;

use serde::Serialize;

/// A sentence in both languages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Text {
    pub en: String,
    pub vi: String,
}

impl Text {
    #[must_use]
    pub fn new(en: impl Into<String>, vi: impl Into<String>) -> Self {
        Self {
            en: en.into(),
            vi: vi.into(),
        }
    }

    /// The text in `lang` (`vi`), English otherwise.
    #[must_use]
    pub fn get(&self, lang: &str) -> &str {
        if lang.eq_ignore_ascii_case("vi") {
            &self.vi
        } else {
            &self.en
        }
    }
}

pub(crate) fn arc(en: &str, vi: &str) -> Arc<Text> {
    Arc::new(Text::new(en, vi))
}

/// "If you delete this…" for a build-output folder of `kind`.
pub(crate) fn build_output(kind: &str) -> Arc<Text> {
    match kind {
        "rust" => arc(
            "Rust build output (`target`): the next `cargo build` creates it again, which can take minutes for a large project.",
            "Kết quả build của Rust (`target`): lần `cargo build` sau sẽ tạo lại, có thể mất vài phút với dự án lớn.",
        ),
        "node" => arc(
            "Installed packages or build output of a JavaScript project: `npm install` or the next build brings it back.",
            "Gói đã cài hoặc kết quả build của dự án JavaScript: `npm install` hoặc lần build sau sẽ tạo lại.",
        ),
        "python" => arc(
            "Python caches, build output or a virtual environment: the tools create them again (a virtual environment needs `pip install` again).",
            "Cache, kết quả build hoặc môi trường ảo của Python: công cụ sẽ tạo lại (môi trường ảo cần `pip install` lại).",
        ),
        "dotnet" => arc(
            ".NET build output or the Visual Studio cache: rebuilt by the next build.",
            "Kết quả build .NET hoặc cache của Visual Studio: lần build sau sẽ tạo lại.",
        ),
        "cmake" => arc(
            "A CMake build folder: the next configure and build create it again.",
            "Thư mục build của CMake: lần configure và build sau sẽ tạo lại.",
        ),
        "gradle" => arc(
            "Gradle's project cache or build output: rebuilt by the next build.",
            "Cache dự án hoặc kết quả build của Gradle: lần build sau sẽ tạo lại.",
        ),
        "maven" => arc(
            "Maven build output (`target`): `mvn package` creates it again.",
            "Kết quả build của Maven (`target`): `mvn package` sẽ tạo lại.",
        ),
        "flutter" => arc(
            "Flutter / Dart tool cache: `flutter pub get` and the next build create it again.",
            "Cache công cụ Flutter / Dart: `flutter pub get` và lần build sau sẽ tạo lại.",
        ),
        "unity" => arc(
            "Unity caches and logs: Unity recreates them when the project opens (re-importing `Library` can take a long time).",
            "Cache và log của Unity: Unity tạo lại khi mở dự án (import lại `Library` có thể mất nhiều thời gian).",
        ),
        "unreal" => arc(
            "Unreal Engine intermediate files and caches: the editor rebuilds them (DerivedDataCache takes long).",
            "Tệp trung gian và cache của Unreal Engine: editor sẽ dựng lại (DerivedDataCache mất nhiều thời gian).",
        ),
        "godot" => arc(
            "Godot's import cache (`.godot`): re-imported when the project opens.",
            "Cache import của Godot (`.godot`): được import lại khi mở dự án.",
        ),
        "terraform" => arc(
            "Terraform providers and modules (`.terraform`): `terraform init` downloads them again.",
            "Provider và module của Terraform (`.terraform`): `terraform init` sẽ tải lại.",
        ),
        "ruby" => arc(
            "Ruby gems installed into the project, or its cache: `bundle install` brings them back.",
            "Gem Ruby cài vào dự án hoặc cache của nó: `bundle install` sẽ cài lại.",
        ),
        _ => arc(
            "Build output: the project's next build creates it again.",
            "Kết quả build: lần build sau của dự án sẽ tạo lại.",
        ),
    }
}

pub(crate) fn build_output_untouched() -> Arc<Text> {
    arc(
        "The project's source code, settings and git history stay; only this build folder goes, into the quarantine (restorable for 7 days).",
        "Mã nguồn, thiết lập và lịch sử git của dự án được giữ nguyên; chỉ thư mục build này bị dọn, vào khu cách ly (khôi phục được trong 7 ngày).",
    )
}

pub(crate) fn rule_untouched(min_age_days: u32) -> Arc<Text> {
    if min_age_days > 0 {
        Arc::new(Text::new(
            format!(
                "Only what this rule covers goes, into the quarantine (restorable for 7 days). Files written in the last {min_age_days} day(s) and 🔒 kept paths stay."
            ),
            format!(
                "Chỉ những gì quy tắc này bao phủ bị dọn, vào khu cách ly (khôi phục được trong 7 ngày). Tệp ghi trong {min_age_days} ngày gần đây và các đường dẫn 🔒 Giữ lại không bị đụng tới."
            ),
        ))
    } else {
        arc(
            "Only what this rule covers goes, into the quarantine (restorable for 7 days). 🔒 Kept paths stay.",
            "Chỉ những gì quy tắc này bao phủ bị dọn, vào khu cách ly (khôi phục được trong 7 ngày). Các đường dẫn 🔒 Giữ lại không bị đụng tới.",
        )
    }
}

pub(crate) fn store_untouched() -> Arc<Text> {
    arc(
        "The package manager's own command does it (Dev caches → Stores): it knows what installed projects still need.",
        "Lệnh của chính trình quản lý gói làm việc này (Cache lập trình → Package store): nó biết dự án đã cài còn cần gì.",
    )
}

pub(crate) fn worktree_static() -> Arc<Text> {
    arc(
        "A git worktree an AI coding agent worked in. It can go once git confirms that nothing in it is uncommitted and its branch is merged or pushed, and its session is finished.",
        "Một git worktree mà AI agent đã làm việc trong đó. Có thể gỡ khi git xác nhận không còn gì chưa commit, nhánh đã merge hoặc đã push, và phiên làm việc đã kết thúc.",
    )
}

pub(crate) fn worktree_untouched() -> Arc<Text> {
    arc(
        "The branch and its commits stay (`git worktree remove`, never `--force`); git refuses when something is uncommitted. Conversation history is never touched.",
        "Nhánh và các commit được giữ (`git worktree remove`, không bao giờ `--force`); git từ chối nếu còn thay đổi chưa commit. Lịch sử hội thoại không bao giờ bị đụng tới.",
    )
}

pub(crate) fn leftover_untouched() -> Arc<Text> {
    arc(
        "The conversation itself stays (~/.claude/projects and the other agents' history are never touched).",
        "Bản thân cuộc hội thoại được giữ nguyên (~/.claude/projects và lịch sử của các agent khác không bao giờ bị đụng tới).",
    )
}

pub(crate) fn history() -> Arc<Text> {
    arc(
        "Conversation history of an AI agent: sessions you can resume, project memory, prompts. NANOFreeDisk never deletes it, whoever asks.",
        "Lịch sử hội thoại của AI agent: các phiên có thể tiếp tục, bộ nhớ dự án, prompt. NANOFreeDisk không bao giờ xóa, dù ai yêu cầu.",
    )
}

pub(crate) fn system() -> Arc<Text> {
    arc(
        "Part of Windows. Deleting files here breaks Windows or its updates. The junk inside (Temp, update downloads, logs) is cleaned by the rules; the rest only with Windows' own tools.",
        "Một phần của Windows. Xóa tệp ở đây làm hỏng Windows hoặc bản cập nhật. Rác bên trong (Temp, tệp tải về của Windows Update, log) được dọn bằng quy tắc; phần còn lại chỉ bằng công cụ của chính Windows.",
    )
}

pub(crate) fn programs() -> Arc<Text> {
    arc(
        "Installed programs. Deleting folders here breaks them and leaves their settings behind: uninstall them from Settings → Apps instead.",
        "Chương trình đã cài. Xóa thư mục ở đây làm hỏng chương trình và để lại thiết lập rác: hãy gỡ trong Cài đặt → Ứng dụng.",
    )
}

pub(crate) fn protected(template: &str) -> Arc<Text> {
    Arc::new(Text::new(
        format!(
            "A protected location ({template}): system data, credentials or tool settings. Never deleted from inside the app."
        ),
        format!(
            "Vị trí được bảo vệ ({template}): dữ liệu hệ thống, thông tin đăng nhập hoặc thiết lập công cụ. Không bao giờ bị xóa từ trong ứng dụng."
        ),
    ))
}

pub(crate) fn personal() -> Arc<Text> {
    arc(
        "Your own photos, videos or music. Only you know what is worth keeping: NANOFreeDisk never suggests deleting them. Look for duplicates instead.",
        "Ảnh, video hoặc nhạc của riêng bạn. Chỉ bạn biết cái gì đáng giữ: NANOFreeDisk không bao giờ gợi ý xóa. Hãy tìm tệp trùng lặp thay vào đó.",
    )
}

pub(crate) fn guide_untouched() -> Arc<Text> {
    arc(
        "Only Windows' (or the tool's) own command shrinks this safely: NANOFreeDisk explains how and deletes nothing here itself.",
        "Chỉ lệnh của chính Windows (hoặc của công cụ) mới thu nhỏ được phần này an toàn: NANOFreeDisk giải thích cách làm và không tự xóa gì ở đây.",
    )
}

pub(crate) fn explained_untouched() -> Arc<Text> {
    arc(
        "Nothing proves what it belongs to here, so it is only moved to the Recycle Bin when you pick it: you can restore it from there.",
        "Không có gì xác nhận nó thuộc về đâu, nên chỉ được chuyển vào Thùng rác khi bạn tự chọn: có thể khôi phục từ đó.",
    )
}

pub(crate) fn old_large() -> Arc<Text> {
    arc(
        "Large and not modified for more than a year. Only you know whether it is still needed.",
        "Dung lượng lớn và không được sửa đổi hơn một năm. Chỉ bạn biết nó có còn cần hay không.",
    )
}

pub(crate) fn old_installer() -> Arc<Text> {
    arc(
        "An installer or disk image left in Downloads for more than 30 days: once the program is installed it is usually not needed again.",
        "Bộ cài hoặc ảnh đĩa nằm trong Downloads hơn 30 ngày: khi chương trình đã cài xong thường không cần nữa.",
    )
}

pub(crate) fn recycle_untouched() -> Arc<Text> {
    arc(
        "It goes to the Recycle Bin: you can restore it from there.",
        "Tệp được chuyển vào Thùng rác: có thể khôi phục từ đó.",
    )
}

pub(crate) fn duplicate(keeper: &str) -> Arc<Text> {
    Arc::new(Text::new(
        format!("Another copy of this file stays at {keeper}. Review the copies in Duplicates."),
        format!(
            "Một bản sao khác của tệp này vẫn còn ở {keeper}. Xem lại các bản sao trong Tệp trùng lặp."
        ),
    ))
}

/// First paragraph of an encyclopedia article body, without Markdown emphasis.
pub(crate) fn first_paragraph(body: &str) -> String {
    body.split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty())
        .unwrap_or_default()
        .replace("**", "")
        .replace('\n', " ")
}
