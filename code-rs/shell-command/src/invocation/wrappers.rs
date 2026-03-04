use std::path::Path;

use crate::is_shell_like_executable;

use super::ScriptWrapper;
use super::ScriptWrapperFamily;

pub(crate) fn extract_script_wrapper(command: &[String]) -> Option<ScriptWrapper> {
    match command {
        [shell, flag, script]
            if is_shell_like_executable(shell) && matches!(flag.as_str(), "-c" | "-lc") =>
        {
            Some(ScriptWrapper {
                family: ScriptWrapperFamily::PosixLike,
                mode_flag: flag.clone(),
                script: strip_rc_source_wrapper(script).unwrap_or_else(|| script.trim().to_string()),
            })
        }

        // Busybox applet form: `busybox sh -c "..."` (common on Android/Termux).
        [busybox, applet, flag, script]
            if is_busybox_executable(busybox)
                && is_shell_like_executable(applet)
                && matches!(flag.as_str(), "-c" | "-lc") =>
        {
            Some(ScriptWrapper {
                family: ScriptWrapperFamily::PosixLike,
                mode_flag: flag.clone(),
                script: strip_rc_source_wrapper(script).unwrap_or_else(|| script.trim().to_string()),
            })
        }

        [nu, flag, script] if is_nushell_executable(nu) && matches!(flag.as_str(), "-c" | "-lc") => {
            Some(ScriptWrapper {
                family: ScriptWrapperFamily::Nushell,
                mode_flag: flag.clone(),
                script: script.trim().to_string(),
            })
        }

        [elvish, flag, script] if is_elvish_executable(elvish) && flag == "-c" => Some(ScriptWrapper {
            family: ScriptWrapperFamily::Elvish,
            mode_flag: flag.clone(),
            script: script.trim().to_string(),
        }),

        _ => None,
    }
}

fn strip_rc_source_wrapper(script: &str) -> Option<String> {
    let trimmed = script.trim();
    if !trimmed.starts_with("source ") {
        return None;
    }

    let start = trimmed.find("&& (")?;
    let inner_start = start + "&& (".len();
    let end = trimmed.rfind(')')?;
    if end <= inner_start {
        return None;
    }

    Some(trimmed[inner_start..end].trim().to_string())
}

pub(crate) fn extract_cmd_wrapper(command: &[String]) -> Option<(String, String)> {
    let (exe, rest) = command.split_first()?;
    if !is_cmd_executable(exe) {
        return None;
    }
    if rest.is_empty() {
        return None;
    }

    let mut idx = 0;
    while idx < rest.len() {
        let arg = &rest[idx];
        let lower = arg.to_ascii_lowercase();
        match lower.as_str() {
            "/c" | "/r" | "-c" | "/k" => {
                let body = rest.get(idx + 1..)?;
                if body.is_empty() {
                    return None;
                }
                let script = if let [only] = body {
                    only.clone()
                } else {
                    body.join(" ")
                };
                return Some((lower, script.trim().to_string()));
            }
            _ if lower.starts_with('/') => {
                idx += 1;
                continue;
            }
            _ => {
                return None;
            }
        }
    }

    None
}

pub(crate) fn extract_powershell_script(command: &[String]) -> Option<String> {
    let (exe, rest) = command.split_first()?;
    if !is_powershell_executable(exe) {
        return None;
    }
    if rest.is_empty() {
        return None;
    }

    let mut idx = 0;
    while idx < rest.len() {
        let arg = &rest[idx];
        let lower = arg.to_ascii_lowercase();
        match lower.as_str() {
            "-command" | "/command" | "-c" => {
                return rest.get(idx + 1).cloned().map(|s| s.trim().to_string());
            }
            _ if lower.starts_with("-command:") || lower.starts_with("/command:") => {
                let script = arg.split_once(':')?.1;
                return Some(script.trim().to_string());
            }
            // Benign, no-arg flags we tolerate.
            "-nologo" | "-noprofile" | "-noninteractive" | "-mta" | "-sta" => {
                idx += 1;
                continue;
            }
            // Unknown switch -> skip it conservatively.
            _ if lower.starts_with('-') => {
                idx += 1;
                continue;
            }
            _ => {
                return Some(join_arguments_as_script(&rest[idx..]));
            }
        }
    }

    None
}

pub(crate) fn is_powershell_executable(exe: &str) -> bool {
    let executable_name = Path::new(exe)
        .file_name()
        .and_then(|osstr| osstr.to_str())
        .unwrap_or(exe)
        .to_ascii_lowercase();

    matches!(
        executable_name.as_str(),
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
    )
}

fn is_cmd_executable(exe: &str) -> bool {
    let executable_name = Path::new(exe)
        .file_name()
        .and_then(|osstr| osstr.to_str())
        .unwrap_or(exe)
        .to_ascii_lowercase();

    matches!(executable_name.as_str(), "cmd" | "cmd.exe")
}

fn is_busybox_executable(exe: &str) -> bool {
    let executable_name = Path::new(exe)
        .file_name()
        .and_then(|osstr| osstr.to_str())
        .unwrap_or(exe)
        .to_ascii_lowercase();
    executable_name == "busybox" || executable_name == "busybox.exe"
}

fn is_nushell_executable(exe: &str) -> bool {
    let executable_name = Path::new(exe)
        .file_name()
        .and_then(|osstr| osstr.to_str())
        .unwrap_or(exe)
        .to_ascii_lowercase();
    executable_name == "nu" || executable_name == "nu.exe"
}

fn is_elvish_executable(exe: &str) -> bool {
    let executable_name = Path::new(exe)
        .file_name()
        .and_then(|osstr| osstr.to_str())
        .unwrap_or(exe)
        .to_ascii_lowercase();
    executable_name == "elvish" || executable_name == "elvish.exe"
}

fn join_arguments_as_script(args: &[String]) -> String {
    args.join(" ").trim().to_string()
}
