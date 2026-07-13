use std::collections::HashSet;
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::{ffi::OsStr, ptr};

use url::Url;

const EXTERNAL_LINK_HOSTS: &[&str] = &[
    "bms-ir.org",
    "darksabun.club",
    "github.com",
    "ir.stellabms.xyz",
    "lr2.sakura.ne.jp",
    "lr2ir.com",
    "script.google.com",
    "walkure.net",
    "www.bms-ir.org",
];

pub fn is_app_url(url: &Url) -> bool {
    url.scheme() == "tauri"
        || url.host_str() == Some("tauri.localhost")
        || cfg!(debug_assertions) && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
}

pub fn open_external_url(url: &Url) {
    if !matches!(url.scheme(), "http" | "https") {
        return;
    }
    let Some(host) = url.host_str() else {
        return;
    };
    let known_hosts = EXTERNAL_LINK_HOSTS.iter().copied().collect::<HashSet<_>>();
    let approved = known_hosts.contains(host)
        || matches!(
            rfd::MessageDialog::new()
                .set_title("外部サイトを開きますか？")
                .set_description(format!("{}://{}", url.scheme(), host))
                .set_buttons(rfd::MessageButtons::YesNo)
                .set_level(rfd::MessageLevel::Info)
                .show(),
            rfd::MessageDialogResult::Yes
        );
    if approved {
        shell_open(url.as_str());
    }
}

fn shell_open(value: &str) {
    let operation = wide("open");
    let target = wide(value);
    unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            ptr::null(),
            ptr::null(),
            1,
        );
    }
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(iter::once(0)).collect()
}

#[allow(non_snake_case)]
#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        hwnd: *mut core::ffi::c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show_command: i32,
    ) -> isize;
}

pub fn ensure_writable_directory(path: &Path) -> bool {
    if std::fs::create_dir_all(path).is_err() {
        return false;
    }
    let test_path = path.join(format!(".l2tv-write-test-{}", std::process::id()));
    let writable = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&test_path)
        .is_ok();
    let _ = std::fs::remove_file(test_path);
    writable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_url_filter_only_accepts_internal_origins() {
        assert!(is_app_url(
            &Url::parse("tauri://localhost/index.html").unwrap()
        ));
        assert!(is_app_url(
            &Url::parse("http://tauri.localhost/index.html").unwrap()
        ));
        assert!(!is_app_url(&Url::parse("https://example.com/").unwrap()));
        assert!(!is_app_url(
            &Url::parse("file:///C:/Windows/win.ini").unwrap()
        ));
    }
}
