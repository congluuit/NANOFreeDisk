//! macOS privacy protection (TCC): without **Full Disk Access** the system hides
//! Mail, Messages, Safari, other apps' sandboxes and Time Machine from every app,
//! so a scan of the home folder or the disk is incomplete (those folders show up
//! as unreadable). [`full_disk_access`] tells whether the app has it, so the UI can
//! say how to grant it ([`PRIVACY_SETTINGS_URL`]).

/// System Settings → Privacy & Security → Full Disk Access.
pub const PRIVACY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

/// `Some(true)` when the app may read the folders only Full Disk Access opens,
/// `Some(false)` when macOS refuses them, `None` when it cannot be told (none of
/// them exists, or not macOS).
#[must_use]
pub fn full_disk_access() -> Option<bool> {
    imp::full_disk_access()
}

#[cfg(target_os = "macos")]
mod imp {
    use std::io;
    use std::path::PathBuf;

    pub(super) fn full_disk_access() -> Option<bool> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        // the system TCC database, then folders every Mac has
        let file = std::fs::File::open("/Library/Application Support/com.apple.TCC/TCC.db");
        let dirs = ["Library/Safari", "Library/Mail", "Library/Messages"]
            .map(|d| std::fs::read_dir(home.join(d)).map(|_| ()));
        let mut denied = false;
        for r in std::iter::once(file.map(|_| ())).chain(dirs) {
            match r {
                Ok(()) => return Some(true),
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => denied = true,
                Err(_) => {}
            }
        }
        denied.then_some(false)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub(super) const fn full_disk_access() -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn answers_without_panicking() {
        let fda = super::full_disk_access();
        if cfg!(not(target_os = "macos")) {
            assert_eq!(fda, None);
        }
    }
}
