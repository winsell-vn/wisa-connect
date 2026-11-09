//! Where the refresh token rests between launches.
//!
//! Windows Credential Manager and the macOS Keychain are the storage on the two
//! platforms this app ships installers for. Everything else falls back to a
//! `0600` file under the app config directory — worse, but honest, and the
//! alternative (a secret-service dependency) would drag a D-Bus stack into the
//! build for a platform we do not release yet.
//!
//! Only the refresh token is stored here. The access token lives in memory and
//! is renewed on demand, so a stolen keychain entry cannot be replayed past the
//! refresh window.

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod store {
    const SERVICE: &str = "vn.wisa.connect";
    const ENTRY: &str = "refresh-token";

    fn entry() -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, ENTRY).map_err(|error| format!("Không mở được kho khoá hệ thống: {error}"))
    }

    pub fn save(token: &str) -> Result<(), String> {
        entry()?
            .set_password(token)
            .map_err(|error| format!("Không lưu được phiên đăng nhập vào kho khoá: {error}"))
    }

    pub fn load() -> Result<Option<String>, String> {
        match entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(format!("Không đọc được phiên đăng nhập từ kho khoá: {error}")),
        }
    }

    pub fn clear() -> Result<(), String> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(format!("Không xoá được phiên đăng nhập khỏi kho khoá: {error}")),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod store {
    use std::io::Write;
    use std::path::PathBuf;

    fn path() -> PathBuf {
        crate::config::get_app_config_dir().join("refresh-token")
    }

    pub fn save(token: &str) -> Result<(), String> {
        let target = path();
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Không tạo được thư mục cấu hình: {error}"))?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&target)
            .map_err(|error| format!("Không ghi được phiên đăng nhập: {error}"))?;
        file.write_all(token.as_bytes())
            .map_err(|error| format!("Không ghi được phiên đăng nhập: {error}"))
    }

    pub fn load() -> Result<Option<String>, String> {
        match std::fs::read_to_string(path()) {
            Ok(text) if !text.trim().is_empty() => Ok(Some(text.trim().to_string())),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("Không đọc được phiên đăng nhập: {error}")),
        }
    }

    pub fn clear() -> Result<(), String> {
        match std::fs::remove_file(path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Không xoá được phiên đăng nhập: {error}")),
        }
    }
}

pub use store::{clear, load, save};
