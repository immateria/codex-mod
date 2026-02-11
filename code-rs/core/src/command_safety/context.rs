use std::path::Path;

use crate::shell::Shell;
use crate::util::is_shell_like_executable;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandSafetyOs {
    Windows,
    Macos,
    Linux,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandSafetyShellFamily {
    PosixLike,
    PowerShell,
    Cmd,
    Nushell,
    Elvish,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandSafetyContext {
    pub os: CommandSafetyOs,
    pub shell: CommandSafetyShellFamily,
}

impl CommandSafetyContext {
    pub fn current() -> Self {
        Self {
            os: current_os(),
            shell: CommandSafetyShellFamily::Unknown,
        }
    }

    pub fn from_shell(shell: &Shell) -> Self {
        let shell_family = shell
            .name()
            .as_deref()
            .and_then(infer_shell_family_from_token)
            .or(match shell {
                Shell::PowerShell(_) => Some(CommandSafetyShellFamily::PowerShell),
                _ => None,
            })
            .unwrap_or(CommandSafetyShellFamily::Unknown);

        Self {
            os: current_os(),
            shell: shell_family,
        }
    }

    /// Returns a copy with shell-family inferred from the command argv when
    /// the command explicitly invokes a shell executable.
    pub fn with_command_shell(self, command: &[String]) -> Self {
        let inferred = command
            .first()
            .and_then(|token| infer_shell_family_from_token(token));
        if let Some(shell) = inferred {
            Self { shell, ..self }
        } else {
            self
        }
    }
}

#[cfg(target_os = "windows")]
fn current_os() -> CommandSafetyOs {
    CommandSafetyOs::Windows
}

#[cfg(target_os = "macos")]
fn current_os() -> CommandSafetyOs {
    CommandSafetyOs::Macos
}

#[cfg(target_os = "linux")]
fn current_os() -> CommandSafetyOs {
    CommandSafetyOs::Linux
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn current_os() -> CommandSafetyOs {
    CommandSafetyOs::Other
}

fn infer_shell_family_from_token(token: &str) -> Option<CommandSafetyShellFamily> {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();

    if matches!(base.as_str(), "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe") {
        return Some(CommandSafetyShellFamily::PowerShell);
    }
    if matches!(base.as_str(), "cmd" | "cmd.exe") {
        return Some(CommandSafetyShellFamily::Cmd);
    }
    if matches!(base.as_str(), "nu" | "nu.exe") {
        return Some(CommandSafetyShellFamily::Nushell);
    }
    if matches!(base.as_str(), "elvish" | "elvish.exe") {
        return Some(CommandSafetyShellFamily::Elvish);
    }
    if is_shell_like_executable(trimmed) {
        return Some(CommandSafetyShellFamily::PosixLike);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_shell_from_command_override() {
        let base = CommandSafetyContext {
            os: CommandSafetyOs::Windows,
            shell: CommandSafetyShellFamily::Unknown,
        };

        let command = vec!["pwsh".to_string(), "-Command".to_string(), "ls".to_string()];
        let effective = base.with_command_shell(&command);
        assert_eq!(effective.shell, CommandSafetyShellFamily::PowerShell);
    }
}
