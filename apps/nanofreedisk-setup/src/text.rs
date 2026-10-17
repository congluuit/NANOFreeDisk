//! Every string the setup shows, in English and Vietnamese (both first-class, as in
//! the app). `{x}` placeholders are filled with [`fill`].

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Vi,
}

impl Lang {
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code.get(..2)?.to_ascii_lowercase().as_str() {
            "en" => Some(Self::En),
            "vi" => Some(Self::Vi),
            _ => None,
        }
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Vi => "vi",
        }
    }

    #[must_use]
    pub const fn text(self) -> &'static Text {
        match self {
            Self::En => &EN,
            Self::Vi => &VI,
        }
    }
}

/// Replace `{key}` by `value` for each pair.
#[must_use]
pub fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut s = template.to_owned();
    for (k, v) in pairs {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

#[derive(Debug)]
pub struct Text {
    pub window_title: &'static str,
    pub uninstall_title: &'static str,
    pub tagline: &'static str,
    pub back: &'static str,
    pub next: &'static str,
    pub cancel: &'static str,
    pub install: &'static str,
    pub finish: &'static str,
    pub close: &'static str,
    pub browse: &'static str,
    pub welcome_title: &'static str,
    pub welcome_body: &'static str,
    pub license_title: &'static str,
    pub license_accept: &'static str,
    pub type_title: &'static str,
    pub type_me: &'static str,
    pub type_me_hint: &'static str,
    pub type_all: &'static str,
    pub type_all_hint: &'static str,
    pub options_title: &'static str,
    pub folder: &'static str,
    pub opt_start_menu: &'static str,
    pub opt_desktop: &'static str,
    pub opt_shell_menu: &'static str,
    pub opt_shell_menu_hint: &'static str,
    pub space_needed: &'static str,
    pub progress_title: &'static str,
    pub step_check: &'static str,
    pub step_files: &'static str,
    pub step_webview: &'static str,
    pub step_webview_download: &'static str,
    pub step_register: &'static str,
    pub step_shortcuts: &'static str,
    pub done_title: &'static str,
    pub done_body: &'static str,
    pub done_launch: &'static str,
    pub done_readme: &'static str,
    pub error_title: &'static str,
    pub error_admin: &'static str,
    pub error_not_writable: &'static str,
    pub error_payload: &'static str,
    pub error_running: &'static str,
    pub error_foreign: &'static str,
    pub run_other_uninstaller: &'static str,
    pub warn_webview: &'static str,
    pub elevating: &'static str,
    pub un_title: &'static str,
    pub un_body: &'static str,
    pub un_remove_data: &'static str,
    pub un_remove_data_hint: &'static str,
    pub un_button: &'static str,
    pub un_progress: &'static str,
    pub un_done: &'static str,
    pub un_done_kept: &'static str,
    pub language: &'static str,
    pub shell_caption: &'static str,
    pub shell_dupes_caption: &'static str,
}

pub const EN: Text = Text {
    window_title: "NANOFreeDisk Setup",
    uninstall_title: "Uninstall NANOFreeDisk",
    tagline: "See what fills your disk. Free it safely.",
    back: "Back",
    next: "Next",
    cancel: "Cancel",
    install: "Install",
    finish: "Finish",
    close: "Close",
    browse: "Browse…",
    welcome_title: "Welcome",
    welcome_body: "This installs NANOFreeDisk {version}: a disk analyzer, safe cleaner and dev-cache manager for Windows.\n\nNothing is deleted without your say-so: cleaned files go to the Recycle Bin or a 7-day quarantine you can restore from.",
    license_title: "License",
    license_accept: "I accept the MIT License",
    type_title: "Who is it for?",
    type_me: "Only for me (recommended)",
    type_me_hint: "Installs in your user folder. No administrator rights needed.",
    type_all: "For everyone on this PC",
    type_all_hint: "Installs in Program Files. Windows will ask for administrator rights.",
    options_title: "Folder and options",
    folder: "Install folder",
    opt_start_menu: "Start menu shortcut",
    opt_desktop: "Desktop shortcut",
    opt_shell_menu: "\"Analyze with NANOFreeDisk\" in the Explorer right-click menu",
    opt_shell_menu_hint: "Per user; you can turn it off later in Settings.",
    space_needed: "Space needed: {size}",
    progress_title: "Installing…",
    step_check: "Checking the folder",
    step_files: "Copying files",
    step_webview: "Checking Microsoft Edge WebView2",
    step_webview_download: "Downloading and installing Microsoft Edge WebView2",
    step_register: "Registering with Apps & features",
    step_shortcuts: "Creating shortcuts",
    done_title: "NANOFreeDisk is installed",
    done_body: "Installed in {dir}.\nUninstall it any time from Settings → Apps.",
    done_launch: "Open NANOFreeDisk",
    done_readme: "Read the README",
    error_title: "Setup could not finish",
    error_admin: "Installing for everyone needs administrator rights.",
    error_not_writable: "{dir} cannot be written without administrator rights. Nothing was installed. Choose another folder, or pick \"For everyone on this PC\" to install with administrator approval.",
    error_payload: "This setup file is incomplete (no program files inside). Download it again.",
    error_running: "NANOFreeDisk is running from this folder. Close it and try again.",
    error_foreign: "NANOFreeDisk {version} is already installed by another package (MSI or NSIS) in {dir}. Uninstall it first.",
    run_other_uninstaller: "Uninstall it now",
    warn_webview: "Microsoft Edge WebView2 could not be installed ({reason}). NANOFreeDisk needs it to open; get it from go.microsoft.com/fwlink/p/?LinkId=2124703.",
    elevating: "Waiting for administrator approval…",
    un_title: "Remove NANOFreeDisk",
    un_body: "This removes NANOFreeDisk from {dir}.\n\nFiles still in NANOFreeDisk's quarantine: you will be asked whether to put them back or delete them.",
    un_remove_data: "Also remove my data",
    un_remove_data_hint: "Scan history, settings and logs. Left untouched if not ticked, so a later install picks them up.",
    un_button: "Uninstall",
    un_progress: "Removing NANOFreeDisk…",
    un_done: "NANOFreeDisk was removed.",
    un_done_kept: "NANOFreeDisk was removed. Your data was kept.",
    language: "Language",
    shell_caption: "Analyze with NANOFreeDisk",
    shell_dupes_caption: "Find duplicates here",
};

pub const VI: Text = Text {
    window_title: "Cài đặt NANOFreeDisk",
    uninstall_title: "Gỡ cài đặt NANOFreeDisk",
    tagline: "Xem thứ gì chiếm ổ đĩa. Giải phóng an toàn.",
    back: "Quay lại",
    next: "Tiếp",
    cancel: "Hủy",
    install: "Cài đặt",
    finish: "Xong",
    close: "Đóng",
    browse: "Chọn…",
    welcome_title: "Chào mừng",
    welcome_body: "Trình này cài NANOFreeDisk {version}: phân tích ổ đĩa, dọn dẹp an toàn và quản lý cache lập trình cho Windows.\n\nKhông gì bị xóa nếu bạn chưa đồng ý: tệp đã dọn vào Thùng rác hoặc khu cách ly 7 ngày, khôi phục được.",
    license_title: "Giấy phép",
    license_accept: "Tôi đồng ý với Giấy phép MIT",
    type_title: "Cài cho ai?",
    type_me: "Chỉ cho tôi (khuyên dùng)",
    type_me_hint: "Cài vào thư mục người dùng. Không cần quyền quản trị.",
    type_all: "Cho mọi người trên máy này",
    type_all_hint: "Cài vào Program Files. Windows sẽ hỏi quyền quản trị.",
    options_title: "Thư mục và tùy chọn",
    folder: "Thư mục cài đặt",
    opt_start_menu: "Lối tắt trong Start menu",
    opt_desktop: "Lối tắt trên Desktop",
    opt_shell_menu: "\"Phân tích bằng NANOFreeDisk\" trong menu chuột phải của Explorer",
    opt_shell_menu_hint: "Theo từng người dùng; tắt được sau trong Cài đặt.",
    space_needed: "Dung lượng cần: {size}",
    progress_title: "Đang cài đặt…",
    step_check: "Kiểm tra thư mục",
    step_files: "Chép tệp",
    step_webview: "Kiểm tra Microsoft Edge WebView2",
    step_webview_download: "Tải và cài Microsoft Edge WebView2",
    step_register: "Đăng ký trong Apps & features",
    step_shortcuts: "Tạo lối tắt",
    done_title: "Đã cài NANOFreeDisk",
    done_body: "Đã cài vào {dir}.\nGỡ bất cứ lúc nào trong Settings → Apps.",
    done_launch: "Mở NANOFreeDisk",
    done_readme: "Đọc README",
    error_title: "Không cài xong được",
    error_admin: "Cài cho mọi người cần quyền quản trị.",
    error_not_writable: "Không ghi được vào {dir} nếu không có quyền quản trị. Chưa có gì được cài. Hãy chọn thư mục khác, hoặc chọn \"Cho mọi người trên máy này\" để cài với quyền quản trị.",
    error_payload: "Tệp cài đặt này không đầy đủ (không có tệp chương trình bên trong). Hãy tải lại.",
    error_running: "NANOFreeDisk đang chạy từ thư mục này. Hãy đóng ứng dụng rồi thử lại.",
    error_foreign: "NANOFreeDisk {version} đã được cài bằng gói khác (MSI hoặc NSIS) ở {dir}. Hãy gỡ bản đó trước.",
    run_other_uninstaller: "Gỡ bản đó ngay",
    warn_webview: "Không cài được Microsoft Edge WebView2 ({reason}). NANOFreeDisk cần nó để mở; tải tại go.microsoft.com/fwlink/p/?LinkId=2124703.",
    elevating: "Đang chờ cấp quyền quản trị…",
    un_title: "Gỡ NANOFreeDisk",
    un_body: "Trình này gỡ NANOFreeDisk khỏi {dir}.\n\nTệp còn trong khu cách ly của NANOFreeDisk: bạn sẽ được hỏi trả về chỗ cũ hay xóa hẳn.",
    un_remove_data: "Xóa cả dữ liệu của tôi",
    un_remove_data_hint: "Lịch sử quét, cài đặt và nhật ký. Không tích thì giữ nguyên, lần cài sau dùng lại.",
    un_button: "Gỡ cài đặt",
    un_progress: "Đang gỡ NANOFreeDisk…",
    un_done: "Đã gỡ NANOFreeDisk.",
    un_done_kept: "Đã gỡ NANOFreeDisk. Dữ liệu của bạn được giữ lại.",
    language: "Ngôn ngữ",
    shell_caption: "Phân tích bằng NANOFreeDisk",
    shell_dupes_caption: "Tìm tệp trùng ở đây",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_match_between_languages() {
        let en = format!("{EN:?}");
        let vi = format!("{VI:?}");
        for key in ["{version}", "{dir}", "{size}", "{reason}"] {
            assert_eq!(en.matches(key).count(), vi.matches(key).count(), "{key}");
        }
        assert_eq!(fill("a {x} b", &[("x", "1")]), "a 1 b");
        assert_eq!(Lang::from_code("vi-VN"), Some(Lang::Vi));
        assert_eq!(Lang::from_code("fr"), None);
    }
}
