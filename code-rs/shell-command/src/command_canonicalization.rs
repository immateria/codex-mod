use std::path::Path;

use crate::invocation;
use crate::invocation::ScriptWrapperFamily;
use crate::invocation::Invocation;

const CANONICAL_SHELL_SCRIPT_PREFIX: &str = "__code_shell_script__";
const CANONICAL_NUSHELL_SCRIPT_PREFIX: &str = "__code_nushell_script__";
const CANONICAL_ELVISH_SCRIPT_PREFIX: &str = "__code_elvish_script__";
const CANONICAL_CMD_SCRIPT_PREFIX: &str = "__code_cmd_script__";
const CANONICAL_POWERSHELL_SCRIPT_PREFIX: &str = "__code_powershell_script__";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalApprovalCommandKind {
    /// A plain argv vector that can be compared directly (e.g. `["cargo", "test"]`).
    Argv,
    /// A shell wrapper command where we can't safely recover a tokenized argv.
    ShellScript,
    /// A Nushell wrapper command where we canonicalize to the script text.
    NushellScript,
    /// An Elvish wrapper command where we canonicalize to the script text.
    ElvishScript,
    /// A CMD wrapper command where we canonicalize to the script text.
    CmdScript,
    /// A PowerShell wrapper command where we canonicalize to the script text.
    PowerShellScript,
}

pub fn canonical_approval_command_kind(canonical: &[String]) -> CanonicalApprovalCommandKind {
    let (_, rest) = split_prefix_wrappers(canonical);
    match rest.first().map(String::as_str) {
        Some(CANONICAL_SHELL_SCRIPT_PREFIX) => CanonicalApprovalCommandKind::ShellScript,
        Some(CANONICAL_NUSHELL_SCRIPT_PREFIX) => CanonicalApprovalCommandKind::NushellScript,
        Some(CANONICAL_ELVISH_SCRIPT_PREFIX) => CanonicalApprovalCommandKind::ElvishScript,
        Some(CANONICAL_CMD_SCRIPT_PREFIX) => CanonicalApprovalCommandKind::CmdScript,
        Some(CANONICAL_POWERSHELL_SCRIPT_PREFIX) => CanonicalApprovalCommandKind::PowerShellScript,
        _ => CanonicalApprovalCommandKind::Argv,
    }
}

/// Canonicalize command argv for approval-cache matching.
///
/// This keeps approval decisions stable across wrapper-path differences (for
/// example `/bin/bash -lc` vs `bash -lc`) and across shell wrapper tools while
/// preserving exact script text for complex scripts where we cannot safely
/// recover a tokenized command sequence.
pub fn canonicalize_command_for_approval(command: &[String]) -> Vec<String> {
    let classified = invocation::classify(command);

    let mut canonical = match &classified.invocation {
        Invocation::ScriptWrapper {
            family,
            mode_flag,
            script,
        } => {
            if let Some(commands) = invocation::parse_word_only_commands(script)
                && let [single_command] = commands.as_slice()
            {
                single_command.clone()
            } else {
                let prefix = match family {
                    ScriptWrapperFamily::PosixLike => CANONICAL_SHELL_SCRIPT_PREFIX,
                    ScriptWrapperFamily::Nushell => CANONICAL_NUSHELL_SCRIPT_PREFIX,
                    ScriptWrapperFamily::Elvish => CANONICAL_ELVISH_SCRIPT_PREFIX,
                };
                vec![prefix.to_string(), mode_flag.clone(), script.clone()]
            }
        }
        Invocation::PowerShellScript { script } => {
            vec![CANONICAL_POWERSHELL_SCRIPT_PREFIX.to_string(), script.clone()]
        }
        Invocation::CmdScript { mode, script } => vec![
            CANONICAL_CMD_SCRIPT_PREFIX.to_string(),
            mode.clone(),
            script.clone(),
        ],
        Invocation::Argv(argv) => argv.clone(),
    };

    // Preserve prefix wrappers so approvals for `ls` do not implicitly approve
    // `sudo ls`, and so env/sudo wrapped scripts remain distinct.
    //
    // Note: we only preserve the presence of the wrapper, not its full flags
    // or environment assignments.
    if classified.prefix.env {
        canonical.insert(0, "env".to_string());
    }
    if classified.prefix.sudo {
        canonical.insert(0, "sudo".to_string());
    }

    canonical
}

pub fn normalize_command_for_persistence(command: &[String]) -> Vec<String> {
    let canonical = canonicalize_command_for_approval(command);
    let (prefix_tokens, rest) = split_prefix_wrappers(&canonical);
    let classified = invocation::classify(command);

    match canonical_approval_command_kind(&canonical) {
        CanonicalApprovalCommandKind::Argv => canonical,
        CanonicalApprovalCommandKind::ShellScript
        | CanonicalApprovalCommandKind::NushellScript
        | CanonicalApprovalCommandKind::ElvishScript
        | CanonicalApprovalCommandKind::CmdScript => {
            let mode = rest.get(1).cloned().unwrap_or_default();
            let script = rest.get(2).cloned().unwrap_or_default();
            let shell = if classified.peeled_argv.first().is_some_and(|s| is_busybox_executable(s))
                && classified
                    .peeled_argv
                    .get(1)
                    .is_some_and(|a| is_shell_applet(a))
            {
                // Preserve the applet name (`busybox sh -c ...`) so persisted
                // patterns are stable and human-readable.
                classified
                    .peeled_argv
                    .get(1)
                    .and_then(|applet| file_name_only(applet))
                    .unwrap_or_else(|| "sh".to_string())
            } else {
                classified
                    .peeled_argv
                    .first()
                    .and_then(|shell| file_name_only(shell))
                    .unwrap_or_else(|| "bash".to_string())
            };
            prefix_tokens
                .into_iter()
                .chain([shell, mode, script])
                .collect()
        }
        CanonicalApprovalCommandKind::PowerShellScript => {
            let script = rest.get(1).cloned().unwrap_or_default();
            let shell = classified
                .peeled_argv
                .first()
                .and_then(|shell| file_name_only(shell))
                .unwrap_or_else(|| "pwsh".to_string());
            prefix_tokens
                .into_iter()
                .chain([shell, "-Command".to_string(), script])
                .collect()
        }
    }
}

fn split_prefix_wrappers(canonical: &[String]) -> (Vec<String>, &[String]) {
    let mut idx = 0;
    while idx < canonical.len() {
        let tok = canonical[idx].as_str();
        if tok.eq_ignore_ascii_case("sudo") || tok.eq_ignore_ascii_case("env") {
            idx += 1;
            continue;
        }
        break;
    }
    (canonical[..idx].to_vec(), &canonical[idx..])
}

fn file_name_only(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .map(ToString::to_string)
}

fn is_busybox_executable(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| name.eq_ignore_ascii_case("busybox") || name.eq_ignore_ascii_case("busybox.exe"))
        .unwrap_or(false)
}

fn is_shell_applet(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| matches!(name.to_ascii_lowercase().as_str(), "sh" | "ash" | "bash" | "zsh" | "dash" | "ksh"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn canonicalizes_word_only_shell_scripts_to_inner_command() {
        let command_a = vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            "cargo test -p code-core".to_string(),
        ];
        let command_b = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cargo   test   -p code-core".to_string(),
        ];

        assert_eq!(
            canonicalize_command_for_approval(&command_a),
            vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "code-core".to_string(),
            ]
        );
        assert_eq!(
            canonicalize_command_for_approval(&command_a),
            canonicalize_command_for_approval(&command_b)
        );
    }

    #[test]
    fn canonicalizes_shell_scripts_wrapped_in_rc_source_to_inner_command() {
        let script = "source /tmp/.bashrc && (cargo test -p code-core)";
        let command = vec!["bash".to_string(), "-lc".to_string(), script.to_string()];

        assert_eq!(
            canonicalize_command_for_approval(&command),
            vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "code-core".to_string(),
            ]
        );
    }

    #[test]
    fn canonicalizes_heredoc_scripts_to_stable_script_key() {
        let script = "python3 <<'PY'\nprint('hello')\nPY";
        let command_a = vec![
            "/bin/zsh".to_string(),
            "-lc".to_string(),
            script.to_string(),
        ];
        let command_b = vec!["zsh".to_string(), "-lc".to_string(), script.to_string()];

        assert_eq!(
            canonicalize_command_for_approval(&command_a),
            vec![
                "__code_shell_script__".to_string(),
                "-lc".to_string(),
                script.to_string(),
            ]
        );
        assert_eq!(
            canonicalize_command_for_approval(&command_a),
            canonicalize_command_for_approval(&command_b)
        );
    }

    #[test]
    fn canonicalizes_powershell_wrappers_to_stable_script_key() {
        let script = "Write-Host hi";
        let command_a = vec![
            "powershell.exe".to_string(),
            "-NoProfile".to_string(),
            "-Command".to_string(),
            script.to_string(),
        ];
        let command_b = vec![
            "pwsh".to_string(),
            "-Command".to_string(),
            script.to_string(),
        ];

        assert_eq!(
            canonicalize_command_for_approval(&command_a),
            vec!["__code_powershell_script__".to_string(), script.to_string(),]
        );
        assert_eq!(
            canonicalize_command_for_approval(&command_a),
            canonicalize_command_for_approval(&command_b)
        );
    }

    #[test]
    fn preserves_non_shell_commands() {
        let command = vec!["cargo".to_string(), "fmt".to_string()];
        assert_eq!(canonicalize_command_for_approval(&command), command);
    }

    #[test]
    fn normalizes_shell_commands_for_persistence_without_rc_paths() {
        let script = "source /Users/me/.bashrc && (cargo test -p code-core)";
        let command = vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            script.to_string(),
        ];
        assert_eq!(
            normalize_command_for_persistence(&command),
            vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "code-core".to_string(),
            ]
        );
    }

    #[test]
    fn normalizes_complex_shell_scripts_to_shell_wrapper_for_persistence() {
        let script = "source /Users/me/.bashrc && (python3 <<'PY'\nprint('hi')\nPY)";
        let command = vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            script.to_string(),
        ];

        assert_eq!(
            normalize_command_for_persistence(&command),
            vec![
                "bash".to_string(),
                "-lc".to_string(),
                "python3 <<'PY'\nprint('hi')\nPY".to_string(),
            ]
        );
    }

    #[test]
    fn canonicalizes_nushell_word_only_scripts_to_inner_command() {
        let command = vec![
            "nu".to_string(),
            "-c".to_string(),
            "cargo test -p code-core".to_string(),
        ];

        assert_eq!(
            canonicalize_command_for_approval(&command),
            vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "code-core".to_string(),
            ]
        );
    }

    #[test]
    fn canonicalizes_elvish_word_only_scripts_to_inner_command() {
        let command = vec![
            "elvish".to_string(),
            "-c".to_string(),
            "git status".to_string(),
        ];

        assert_eq!(
            canonicalize_command_for_approval(&command),
            vec!["git".to_string(), "status".to_string()]
        );
    }

    #[test]
    fn canonicalizes_cmd_wrappers_to_stable_script_key() {
        let command_a = vec![
            "cmd.exe".to_string(),
            "/c".to_string(),
            "dir".to_string(),
        ];
        let command_b = vec!["cmd".to_string(), "/c".to_string(), "dir".to_string()];

        assert_eq!(
            canonicalize_command_for_approval(&command_a),
            vec![
                "__code_cmd_script__".to_string(),
                "/c".to_string(),
                "dir".to_string(),
            ]
        );
        assert_eq!(
            canonicalize_command_for_approval(&command_a),
            canonicalize_command_for_approval(&command_b)
        );
    }

    #[test]
    fn preserves_sudo_prefix_in_canonicalization() {
        assert_eq!(
            canonicalize_command_for_approval(&["sudo".to_string(), "ls".to_string()]),
            vec!["sudo".to_string(), "ls".to_string()],
        );
        assert_eq!(
            canonicalize_command_for_approval(&[
                "sudo".to_string(),
                "bash".to_string(),
                "-lc".to_string(),
                "ls".to_string(),
            ]),
            vec!["sudo".to_string(), "ls".to_string()],
        );
    }
}
