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
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
))]
fn has_gui_env() -> bool {
    let has_display = std::env::var("DISPLAY")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let has_wayland = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    has_display || has_wayland
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
))]
fn xdg_current_desktop() -> String {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
))]
fn command_exists(command: &str) -> bool {
    which::which(command).is_ok()
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
))]
fn run_picker_command(command: &str, args: &[String]) -> Result<Option<PathBuf>> {
    use anyhow::Context;
    use std::process::Command;

    let output = Command::new(command)
        .args(args)
        .output()
        .with_context(|| format!("failed to run `{command}`"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let selected = stdout.trim();
    if output.status.success() {
        if selected.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(selected)))
        }
    } else if output.status.code() == Some(1) && selected.is_empty() {
        // Common "user cancelled" exit code.
        Ok(None)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow::anyhow!(
            "`{command}` failed: {}",
            stderr.trim().replace('\n', " ")
        ))
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
))]
fn pick_linux_bsd(kind: NativePickerKind, title: &str) -> Result<Option<PathBuf>> {
    if !has_gui_env() {
        return Err(anyhow::anyhow!(
            "no GUI session detected (missing DISPLAY/WAYLAND_DISPLAY)"
        ));
    }

    let title = normalize_title(title);
    let desktop = xdg_current_desktop().to_ascii_lowercase();

    // Prefer the DE-native pickers first; fall back to portal (rfd) last.
    let mut backends: Vec<&'static str> = Vec::new();
    if desktop.contains("kde") {
        backends.extend(["kdialog", "zenity", "qarma", "yad"]);
    } else if desktop.contains("gnome") || desktop.contains("unity") || desktop.contains("cinnamon") {
        backends.extend(["zenity", "kdialog", "qarma", "yad"]);
    } else {
        backends.extend(["zenity", "kdialog", "qarma", "yad"]);
    }

    for backend in backends {
        if !command_exists(backend) {
            continue;
        }

        let result = match backend {
            "zenity" | "qarma" => {
                let mut args: Vec<String> = vec![
                    "--file-selection".to_string(),
                    "--title".to_string(),
                    title.clone(),
                ];
                if kind == NativePickerKind::Folder {
                    args.push("--directory".to_string());
                }
                run_picker_command(backend, &args)
            }
            "yad" => {
                let mut args: Vec<String> = vec![
                    "--file-selection".to_string(),
                    "--title".to_string(),
                    title.clone(),
                ];
                if kind == NativePickerKind::Folder {
                    args.push("--directory".to_string());
                }
                run_picker_command(backend, &args)
            }
            "kdialog" => {
                let args: Vec<String> = match kind {
                    NativePickerKind::File => vec![
                        "--getopenfilename".to_string(),
                        "--title".to_string(),
                        title.clone(),
                    ],
                    NativePickerKind::Folder => vec![
                        "--getexistingdirectory".to_string(),
                        "--title".to_string(),
                        title.clone(),
                    ],
                };
                run_picker_command(backend, &args)
            }
            _ => continue,
        };

        match result {
            Ok(Some(path)) => return Ok(Some(path)),
            Ok(None) => return Ok(None), // user cancelled
            Err(_err) => {
                // Try next backend.
                continue;
            }
        }
    }

    // Last resort: XDG portal via rfd.
    pick_rfd(kind, &title)
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
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    ))]
    {
        return pick_linux_bsd(kind, title);
    }

    pick_rfd(kind, title)
}
