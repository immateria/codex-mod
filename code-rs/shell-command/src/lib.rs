//! Shell command parsing and canonicalization utilities shared across `code-rs` crates.

use std::path::Path;

use shlex::try_join;

pub mod bash;
pub mod command_safety;
pub mod command_canonicalization;
pub mod parse_command;

/// Escape a command argv for display.
///
/// Prefers shell-escaped rendering when possible, falling back to a simple
/// space-joined string if `shlex` fails.
pub fn escape_command(command: &[String]) -> String {
    try_join(command.iter().map(String::as_str)).unwrap_or_else(|_| command.join(" "))
}

/// If the argv looks like `bash -lc <script>` or `bash -c <script>`, return the
/// inner script (stripping any `source … && (...)` wrapper), otherwise return
/// the escaped argv.
pub fn strip_bash_lc_and_escape(command: &[String]) -> String {
    match command {
        [first, second, third]
            if is_shell_like_executable(first) && (second == "-lc" || second == "-c") =>
        {
            strip_rc_source_wrapper(third)
                .unwrap_or(third.as_str())
                .to_string()
        }
        _ => escape_command(command),
    }
}

fn strip_rc_source_wrapper(script: &str) -> Option<&str> {
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

    Some(trimmed[inner_start..end].trim())
}

/// True if `token` looks like a common POSIX shell program (or a path to one).
pub fn is_shell_like_executable(token: &str) -> bool {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    let name = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "bash"
            | "bash.exe"
            | "sh"
            | "sh.exe"
            | "zsh"
            | "zsh.exe"
            | "dash"
            | "dash.exe"
            | "ksh"
            | "ksh.exe"
            | "busybox"
    )
}
