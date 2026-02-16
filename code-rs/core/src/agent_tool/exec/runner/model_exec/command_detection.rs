#[cfg(target_os = "windows")]
fn default_pathext_or_default() -> Vec<String> {
    std::env::var("PATHEXT")
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .split(';')
                .filter(|entry| !entry.is_empty())
                .map(|entry| entry.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                ".com".into(),
                ".exe".into(),
                ".bat".into(),
                ".cmd".into(),
                ".ps1".into(),
            ]
        })
}

// Cross-platform check whether an executable is available in PATH and
// directly spawnable by std::process::Command (no shell wrappers).
pub(super) fn command_exists(cmd: &str) -> bool {
    if cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') || cmd.contains('\\') {
        let path = std::path::Path::new(cmd);
        if path.extension().is_some() {
            return std::fs::metadata(path)
                .map(|metadata| metadata.is_file())
                .unwrap_or(false);
        }

        #[cfg(target_os = "windows")]
        {
            for ext in default_pathext_or_default() {
                let candidate = path
                    .with_extension("")
                    .with_extension(ext.trim_start_matches('.'));
                if std::fs::metadata(&candidate)
                    .map(|metadata| metadata.is_file())
                    .unwrap_or(false)
                {
                    return true;
                }
            }
        }

        return std::fs::metadata(path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false);
    }

    #[cfg(target_os = "windows")]
    {
        let exts = default_pathext_or_default();
        let path_var = std::env::var_os("PATH");
        let path_iter = path_var
            .as_ref()
            .map(std::env::split_paths)
            .into_iter()
            .flatten();

        let candidates: Vec<String> = if std::path::Path::new(cmd).extension().is_some() {
            vec![cmd.to_string()]
        } else {
            exts.iter().map(|ext| format!("{cmd}{ext}")).collect()
        };

        for dir in path_iter {
            for candidate in &candidates {
                if dir.join(candidate).is_file() {
                    return true;
                }
            }
        }

        false
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;

        let Some(path_os) = std::env::var_os("PATH") else {
            return false;
        };

        for dir in std::env::split_paths(&path_os) {
            if dir.as_os_str().is_empty() {
                continue;
            }

            let candidate = dir.join(cmd);
            if let Ok(meta) = std::fs::metadata(&candidate)
                && meta.is_file()
            {
                let mode = meta.permissions().mode();
                if mode & 0o111 != 0 {
                    return true;
                }
            }
        }

        false
    }
}

pub(super) fn is_known_family(value: &str) -> bool {
    matches!(
        value,
        "claude" | "gemini" | "qwen" | "codex" | "code" | "cloud" | "coder"
    )
}
