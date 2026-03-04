//! Shell command parsing and canonicalization utilities shared across `code-rs` crates.

use std::path::Path;

use shlex::try_join;

pub mod bash;
pub mod command_safety;
pub mod command_canonicalization;
pub mod parse_command;
mod invocation;

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
///
/// Note: despite the legacy name, this recognizes common script wrappers for
/// POSIX-like shells (bash/sh/zsh/dash/ksh/ash) plus `nu` and `elvish`.
pub fn strip_bash_lc_and_escape(command: &[String]) -> String {
    if let Some(wrapper) = invocation::extract_script_wrapper(command) {
        wrapper.script
    } else {
        escape_command(command)
    }
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
            | "ash"
            | "ash.exe"
            | "sh"
            | "sh.exe"
            | "zsh"
            | "zsh.exe"
            | "dash"
            | "dash.exe"
            | "ksh"
            | "ksh.exe"
    )
}
