use anyhow::Result;
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd",
))]
use std::path::PathBuf;

fn spawn_detached(mut cmd: Command) -> Result<()> {
    // Best-effort: don't inherit the TUI stdio streams.
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.spawn()?;
    Ok(())
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
fn best_effort_target_for_reveal(path: &Path) -> PathBuf {
    if path.is_dir() {
        return path.to_path_buf();
    }
    path.parent().unwrap_or(path).to_path_buf()
}

pub(crate) fn reveal_path(path: &Path) -> Result<()> {
    if crate::chatwidget::is_test_mode() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        if path.is_dir() {
            cmd.arg(path);
        } else {
            cmd.arg("-R").arg(path);
        }
        spawn_detached(cmd)
    }

    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("explorer.exe");
        if path.is_dir() {
            cmd.arg(path);
        } else {
            cmd.arg(format!("/select,{}", path.display()));
        }
        spawn_detached(cmd)
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
    ))]
    {
        if !has_gui_env() {
            return Err(anyhow::anyhow!(
                "no GUI session detected (missing DISPLAY/WAYLAND_DISPLAY)"
            ));
        }

        let target = best_effort_target_for_reveal(path);

        // Best-effort: try common openers; settle for opening the parent folder.
        let candidates: [(&str, &[&str]); 5] = [
            ("xdg-open", &[]),
            ("gio", &["open"]),
            ("kde-open5", &[]),
            ("kde-open", &[]),
            ("gnome-open", &[]),
        ];

        for (bin, prefix) in candidates {
            if which::which(bin).is_err() {
                continue;
            }
            let mut cmd = Command::new(bin);
            for arg in prefix {
                cmd.arg(arg);
            }
            cmd.arg(&target);
            if spawn_detached(cmd).is_ok() {
                return Ok(());
            }
        }

        Err(anyhow::anyhow!(
            "no supported file manager opener found (tried xdg-open/gio/kde-open/gnome-open)"
        ))
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
    {
        let _ = path;
        Err(anyhow::anyhow!(
            "opening paths in a file manager is not supported on this platform"
        ))
    }
}
