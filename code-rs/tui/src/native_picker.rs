use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativePickerKind {
    File,
    Folder,
}

fn normalize_title(title: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        "Select".to_string()
    } else {
        trimmed.replace(['\r', '\n'], " ")
    }
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
))]
fn pick_rfd(kind: NativePickerKind, title: &str) -> Result<Option<PathBuf>> {
    if crate::chatwidget::is_test_mode() {
        return Ok(None);
    }

    let title = normalize_title(title);
    let dialog = rfd::FileDialog::new().set_title(&title);
    Ok(match kind {
        NativePickerKind::File => dialog.pick_file(),
        NativePickerKind::Folder => dialog.pick_folder(),
    })
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
)))]
fn pick_rfd(_kind: NativePickerKind, _title: &str) -> Result<Option<PathBuf>> {
    Err(anyhow::anyhow!(
        "native picker is not supported on this platform"
    ))
}

pub(crate) fn pick_path(kind: NativePickerKind, title: &str) -> Result<Option<PathBuf>> {
    pick_rfd(kind, title)
}
