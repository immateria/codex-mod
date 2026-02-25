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

#[cfg(target_os = "macos")]
fn pick_macos(kind: NativePickerKind, title: &str) -> Result<Option<PathBuf>> {
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

#[cfg(not(target_os = "macos"))]
fn pick_macos(_kind: NativePickerKind, _title: &str) -> Result<Option<PathBuf>> {
    Err(anyhow::anyhow!("native picker is only implemented for macOS"))
}

pub(crate) fn pick_path(kind: NativePickerKind, title: &str) -> Result<Option<PathBuf>> {
    pick_macos(kind, title)
}

