use std::path::Path;

use crate::bash::try_parse_bash;
use crate::bash::try_parse_word_only_commands_sequence;
use crate::command_safety::context::CommandSafetyContext;
use crate::command_safety::context::CommandSafetyOs;
use crate::command_safety::context::CommandSafetyShellFamily;
use crate::command_safety::windows_dangerous_commands::is_dangerous_command_windows;
use crate::command_safety::windows_dangerous_commands::is_dangerous_windows_token_sequence;
use crate::config_types::CommandSafetyRuleset;
use crate::util::is_shell_like_executable;

pub fn command_might_be_dangerous(command: &[String]) -> bool {
    let context = CommandSafetyContext::current().with_command_shell(command);
    command_might_be_dangerous_with_context_and_rules(command, context, CommandSafetyRuleset::Auto)
}

pub fn command_might_be_dangerous_with_context(
    command: &[String],
    context: CommandSafetyContext,
) -> bool {
    command_might_be_dangerous_with_context_and_rules(command, context, CommandSafetyRuleset::Auto)
}

pub fn command_might_be_dangerous_with_context_and_rules(
    command: &[String],
    context: CommandSafetyContext,
    ruleset: CommandSafetyRuleset,
) -> bool {
    let effective_context = context.with_command_shell(command);

    // Keep PowerShell/CMD/URL-launch protections active on all platforms so
    // pwsh-on-macOS/Linux users get the same guardrails, and so Windows users
    // on alternate shells still get Windows-specific safety checks.
    let shell_is_windows_like = matches!(
        effective_context.shell,
        CommandSafetyShellFamily::PowerShell | CommandSafetyShellFamily::Cmd
    );
    let platform_is_windows = matches!(effective_context.os, CommandSafetyOs::Windows);
    let use_windows_checks = match ruleset {
        CommandSafetyRuleset::Windows => true,
        CommandSafetyRuleset::Posix => false,
        CommandSafetyRuleset::Auto => shell_is_windows_like || platform_is_windows,
    };
    if use_windows_checks
        && (is_dangerous_command_windows(command) || is_dangerous_windows_token_sequence(command))
    {
        return true;
    }

    if is_dangerous_to_call_with_exec(command) {
        return true;
    }

    // Support `<shell> -c|-lc "<script>"` for shell-like wrappers (bash, sh,
    // zsh, etc.) and nushell's common `nu -c` form.
    if let Some(all_commands) = parse_shell_c_plain_commands(command)
        && all_commands.iter().any(|cmd| {
            is_dangerous_to_call_with_exec(cmd)
                || (use_windows_checks && is_dangerous_windows_token_sequence(cmd))
        })
    {
        return true;
    }

    false
}

fn parse_shell_c_plain_commands(command: &[String]) -> Option<Vec<Vec<String>>> {
    let [shell, flag, script] = command else {
        return None;
    };

    if !matches!(flag.as_str(), "-c" | "-lc") {
        return None;
    }

    if !is_shell_like_executable(shell) && !is_nushell(shell) {
        return None;
    }

    if let Some(tree) = try_parse_bash(script)
        && let Some(commands) = try_parse_word_only_commands_sequence(&tree, script)
    {
        return Some(commands);
    }

    parse_plain_word_commands_fallback(script)
}

fn parse_plain_word_commands_fallback(script: &str) -> Option<Vec<Vec<String>>> {
    let tokens = shlex::split(script)?;
    if tokens.is_empty() {
        return None;
    }

    let mut all_commands: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for token in tokens {
        if matches!(token.as_str(), "|" | "||" | "&&" | ";") {
            if current.is_empty() {
                return None;
            }
            all_commands.push(std::mem::take(&mut current));
        } else {
            current.push(token);
        }
    }

    if current.is_empty() {
        return None;
    }
    all_commands.push(current);
    Some(all_commands)
}

fn is_nushell(token: &str) -> bool {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("nu") || name.eq_ignore_ascii_case("nu.exe"))
        .unwrap_or(false)
}

fn is_git_global_option_with_value(arg: &str) -> bool {
    matches!(
        arg,
        "-C" | "-c"
            | "--config-env"
            | "--exec-path"
            | "--git-dir"
            | "--namespace"
            | "--super-prefix"
            | "--work-tree"
    )
}

fn is_git_global_option_with_inline_value(arg: &str) -> bool {
    matches!(
        arg,
        s if s.starts_with("--config-env=")
            || s.starts_with("--exec-path=")
            || s.starts_with("--git-dir=")
            || s.starts_with("--namespace=")
            || s.starts_with("--super-prefix=")
            || s.starts_with("--work-tree=")
    ) || ((arg.starts_with("-C") || arg.starts_with("-c")) && arg.len() > 2)
}

/// Find the first matching git subcommand, skipping known global options that
/// may appear before it (for example, `-C`, `-c`, `--git-dir`).
pub(crate) fn find_git_subcommand<'a>(
    command: &'a [String],
    subcommands: &[&str],
) -> Option<(usize, &'a str)> {
    let cmd0 = command.first().map(String::as_str)?;
    if !cmd0.ends_with("git") {
        return None;
    }

    let mut skip_next = false;
    for (idx, arg) in command.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }

        let arg = arg.as_str();

        if is_git_global_option_with_inline_value(arg) {
            continue;
        }

        if is_git_global_option_with_value(arg) {
            skip_next = true;
            continue;
        }

        if arg == "--" || arg.starts_with('-') {
            continue;
        }

        if subcommands.contains(&arg) {
            return Some((idx, arg));
        }

        // In git, the first non-option token is the subcommand.
        return None;
    }

    None
}

fn is_dangerous_to_call_with_exec(command: &[String]) -> bool {
    let cmd0 = command.first().map(String::as_str);

    match cmd0 {
        Some(cmd) if cmd.ends_with("git") => {
            let Some((subcommand_idx, subcommand)) =
                find_git_subcommand(command, &["reset", "rm", "branch", "push", "clean"])
            else {
                return false;
            };

            match subcommand {
                "reset" | "rm" => true,
                "branch" => git_branch_is_delete(&command[subcommand_idx + 1..]),
                "push" => git_push_is_dangerous(&command[subcommand_idx + 1..]),
                "clean" => git_clean_is_force(&command[subcommand_idx + 1..]),
                other => {
                    debug_assert!(false, "unexpected git subcommand from matcher: {other}");
                    false
                }
            }
        }

        Some("rm") => matches!(command.get(1).map(String::as_str), Some("-f" | "-rf")),

        // For `sudo <cmd>`, recurse into `<cmd>`.
        Some("sudo") => is_dangerous_to_call_with_exec(&command[1..]),

        _ => false,
    }
}

fn git_branch_is_delete(branch_args: &[String]) -> bool {
    branch_args.iter().map(String::as_str).any(|arg| {
        matches!(arg, "-d" | "-D" | "--delete")
            || arg.starts_with("--delete=")
            || short_flag_group_contains(arg, 'd')
            || short_flag_group_contains(arg, 'D')
    })
}

fn short_flag_group_contains(arg: &str, target: char) -> bool {
    arg.starts_with('-') && !arg.starts_with("--") && arg.chars().skip(1).any(|c| c == target)
}

fn git_push_is_dangerous(push_args: &[String]) -> bool {
    push_args.iter().map(String::as_str).any(|arg| {
        matches!(
            arg,
            "--force" | "--force-with-lease" | "--force-if-includes" | "--delete" | "-f" | "-d"
        ) || arg.starts_with("--force-with-lease=")
            || arg.starts_with("--force-if-includes=")
            || arg.starts_with("--delete=")
            || short_flag_group_contains(arg, 'f')
            || short_flag_group_contains(arg, 'd')
            || git_push_refspec_is_dangerous(arg)
    })
}

fn git_push_refspec_is_dangerous(arg: &str) -> bool {
    // `+<refspec>` forces updates and `:<dst>` deletes remote refs.
    (arg.starts_with('+') || arg.starts_with(':')) && arg.len() > 1
}

fn git_clean_is_force(clean_args: &[String]) -> bool {
    clean_args.iter().map(String::as_str).any(|arg| {
        matches!(arg, "--force" | "-f")
            || arg.starts_with("--force=")
            || short_flag_group_contains(arg, 'f')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(std::string::ToString::to_string).collect()
    }

    #[test]
    fn git_reset_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&["git", "reset"])));
    }

    #[test]
    fn bash_git_reset_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "bash",
            "-lc",
            "git reset --hard",
        ])));
    }

    #[test]
    fn zsh_git_reset_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "zsh",
            "-lc",
            "git reset --hard",
        ])));
    }

    #[test]
    fn nu_git_reset_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "nu",
            "-c",
            "git reset --hard",
        ])));
    }

    #[test]
    fn pwsh_remove_item_force_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "pwsh",
            "-Command",
            "Remove-Item test -Force",
        ])));
    }

    #[test]
    fn windows_nu_remove_item_force_is_dangerous() {
        let context = CommandSafetyContext {
            os: CommandSafetyOs::Windows,
            shell: CommandSafetyShellFamily::Nushell,
        };
        assert!(command_might_be_dangerous_with_context(
            &vec_str(&[
                "nu",
                "-c",
                "Remove-Item -Path test -Recurse -Force",
            ]),
            context,
        ));
    }

    #[test]
    fn unix_nu_remove_item_force_is_not_classified_windows_dangerous() {
        let context = CommandSafetyContext {
            os: CommandSafetyOs::Linux,
            shell: CommandSafetyShellFamily::Nushell,
        };
        assert!(!command_might_be_dangerous_with_context(
            &vec_str(&[
                "nu",
                "-c",
                "Remove-Item -Path test -Recurse -Force",
            ]),
            context,
        ));
    }

    #[test]
    fn windows_ruleset_can_be_forced_on_non_windows_context() {
        let context = CommandSafetyContext {
            os: CommandSafetyOs::Linux,
            shell: CommandSafetyShellFamily::Nushell,
        };
        assert!(command_might_be_dangerous_with_context_and_rules(
            &vec_str(&[
                "nu",
                "-c",
                "Remove-Item -Path test -Recurse -Force",
            ]),
            context,
            CommandSafetyRuleset::Windows,
        ));
    }

    #[test]
    fn posix_ruleset_skips_windows_heuristics_even_on_windows_context() {
        let context = CommandSafetyContext {
            os: CommandSafetyOs::Windows,
            shell: CommandSafetyShellFamily::Nushell,
        };
        assert!(!command_might_be_dangerous_with_context_and_rules(
            &vec_str(&[
                "nu",
                "-c",
                "Remove-Item -Path test -Recurse -Force",
            ]),
            context,
            CommandSafetyRuleset::Posix,
        ));
    }

    #[test]
    fn git_status_is_not_dangerous() {
        assert!(!command_might_be_dangerous(&vec_str(&["git", "status"])));
    }

    #[test]
    fn bash_git_status_is_not_dangerous() {
        assert!(!command_might_be_dangerous(&vec_str(&[
            "bash",
            "-lc",
            "git status",
        ])));
    }

    #[test]
    fn sudo_git_reset_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "sudo", "git", "reset", "--hard",
        ])));
    }

    #[test]
    fn usr_bin_git_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "/usr/bin/git",
            "reset",
            "--hard",
        ])));
    }

    #[test]
    fn git_branch_delete_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "git", "branch", "-d", "feature",
        ])));
        assert!(command_might_be_dangerous(&vec_str(&[
            "git", "branch", "-D", "feature",
        ])));
    }

    #[test]
    fn git_push_force_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "git", "push", "--force", "origin", "main",
        ])));
        assert!(command_might_be_dangerous(&vec_str(&[
            "git", "push", "-f", "origin", "main",
        ])));
    }

    #[test]
    fn git_push_delete_refspec_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "git", "push", "origin", ":feature",
        ])));
    }

    #[test]
    fn git_push_without_force_is_not_dangerous() {
        assert!(!command_might_be_dangerous(&vec_str(&[
            "git", "push", "origin", "main",
        ])));
    }

    #[test]
    fn git_clean_force_is_dangerous_even_when_f_is_not_first_flag() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "git", "clean", "-fdx",
        ])));
        assert!(command_might_be_dangerous(&vec_str(&[
            "git", "clean", "-xdf",
        ])));
    }

    #[test]
    fn rm_rf_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&["rm", "-rf", "/"])));
    }

    #[test]
    fn rm_f_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&["rm", "-f", "/"])));
    }
}
