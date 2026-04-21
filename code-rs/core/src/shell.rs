use serde::Deserialize;
use serde::Serialize;
use shlex;
use std::path::PathBuf;

use crate::config_types::ShellScriptStyle;
use crate::config_types::ShellConfig;
use crate::util::is_shell_like_executable;

/// Extract the canonical shell name from a potentially full path.
///
/// Strips directory components, `.exe` suffix, trailing version suffixes
/// (e.g. `-5.2`), and leading/trailing quotes, then lowercases.
pub(crate) fn shell_basename(path: &str) -> String {
    let trimmed = path.trim_matches('"').trim_matches('\'');
    let base = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed);
    // Strip version suffix before .exe so that `bash.exe-5.2` → `bash.exe` → `bash`.
    let base = strip_version_suffix(base);
    let base = base.strip_suffix(".exe").unwrap_or(base);
    base.to_ascii_lowercase()
}

/// Strip a trailing version suffix like `-5`, `-5.2`, or `-5.2.1`.
///
/// Only strips when the part after the last `-` is purely digits and dots
/// (so `oil-shell` is left alone, but `bash-5.2` becomes `bash`).
fn strip_version_suffix(name: &str) -> &str {
    let mut idx = name.len();
    while let Some(dash) = name[..idx].rfind('-') {
        let suffix = &name[dash + 1..idx];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit() || c == '.') {
            idx = dash;
        } else {
            break;
        }
    }
    &name[..idx]
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ZshShell {
    pub(crate) shell_path: String,
    pub(crate) zshrc_path: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct BashShell {
    pub(crate) shell_path: String,
    pub(crate) bashrc_path: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct PowerShellConfig {
    pub(crate) exe: String, // Executable name or path, e.g. "pwsh" or "powershell.exe".
    pub(crate) bash_exe_fallback: Option<PathBuf>, // In case the model generates a bash command.
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct GenericShell {
    pub(crate) command: Vec<String>,
    pub(crate) script_style: Option<ShellScriptStyle>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Shell {
    Zsh(ZshShell),
    Bash(BashShell),
    PowerShell(PowerShellConfig),
    Generic(GenericShell),
    Unknown,
}

impl Shell {
    pub fn script_style(&self) -> Option<ShellScriptStyle> {
        match self {
            Shell::Zsh(_) => Some(ShellScriptStyle::Zsh),
            Shell::Bash(_) => Some(ShellScriptStyle::BashZshCompatible),
            Shell::Generic(generic) => generic
                .script_style
                .or_else(|| {
                    generic
                        .command
                        .first()
                        .and_then(|program| ShellScriptStyle::infer_from_shell_program(program))
                }),
            Shell::PowerShell(_) => Some(ShellScriptStyle::PowerShell),
            Shell::Unknown => None,
        }
    }

    pub fn format_default_shell_invocation(&self, command: Vec<String>) -> Option<Vec<String>> {
        match self {
            Shell::Zsh(zsh) => format_shell_invocation_with_rc(
                command.as_slice(),
                &zsh.shell_path,
                &zsh.zshrc_path,
            ),
            Shell::Bash(bash) => format_shell_invocation_with_rc(
                command.as_slice(),
                &bash.shell_path,
                &bash.bashrc_path,
            ),
            Shell::PowerShell(ps) => {
                // If model generated a bash command, prefer a detected bash fallback
                if let Some(script) = extract_script_argument(command.as_slice()) {
                    return match &ps.bash_exe_fallback {
                        Some(bash) => Some(vec![
                            bash.to_string_lossy().into_owned(),
                            "-lc".to_owned(),
                            script,
                        ]),

                        // No bash fallback → run the script under PowerShell.
                        // It will likely fail (except for some simple commands), but the error
                        // should give a clue to the model to fix upon retry that it's running under PowerShell.
                        None => Some(vec![
                            ps.exe.clone(),
                            "-NoProfile".to_owned(),
                            "-Command".to_owned(),
                            script,
                        ]),
                    };
                }

                // Not a bash command. If model did not generate a PowerShell command,
                // turn it into a PowerShell command.
                let first = command.first().map(String::as_str);
                if first != Some(ps.exe.as_str()) {
                    let script = shlex::try_join(command.iter().map(String::as_str))
                        .unwrap_or_else(|_| command.join(" "));
                    // Use -EncodedCommand for scripts containing characters that
                    // break PowerShell's -Command argument parser: newlines,
                    // dollar signs (variable expansion), backticks (PS escape),
                    // double quotes, and non-ASCII.
                    if needs_powershell_encoding(&script) {
                        return Some(encode_powershell_command(&ps.exe, &script));
                    }

                    return Some(vec![
                        ps.exe.clone(),
                        "-NoProfile".to_owned(),
                        "-Command".to_owned(),
                        script,
                    ]);
                }

                // Model generated a PowerShell command. Run it.
                Some(command)
            }
            Shell::Generic(generic) => {
                // For generic shells that execute scripts via -c/-lc, pass the
                // model command as a single script argument.
                if generic_shell_expects_script_argument(generic.command.as_slice()) {
                    let script = extract_script_argument(command.as_slice())
                        .or_else(|| shlex::try_join(command.iter().map(String::as_str)).ok())?;
                    let mut invocation = generic.command.clone();
                    invocation.push(script);
                    return Some(invocation);
                }

                // Other generic shells execute command/argv directly.
                let mut invocation = generic.command.clone();
                invocation.extend(command);
                Some(invocation)
            }
            Shell::Unknown => None,
        }
    }

    pub fn name(&self) -> Option<String> {
        match self {
            Shell::Zsh(zsh) => std::path::Path::new(&zsh.shell_path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned()),
            Shell::Bash(bash) => std::path::Path::new(&bash.shell_path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned()),
            Shell::PowerShell(ps) => Some(ps.exe.clone()),
            Shell::Generic(generic) => generic.command.first().cloned(),
            Shell::Unknown => None,
        }
    }

    /// Format a raw shell script into a full command invocation using this
    /// shell. Falls back to a platform default (e.g. `sh -c`) if the shell
    /// type cannot produce an invocation.
    pub fn shell_script_invocation_or_default(
        &self,
        command: String,
        use_login_shell: bool,
    ) -> Vec<String> {
        self.format_shell_script_invocation(command.clone(), use_login_shell)
            .unwrap_or_else(|| default_shell_script_invocation(command, use_login_shell))
    }

    fn format_shell_script_invocation(
        &self,
        command: String,
        use_login_shell: bool,
    ) -> Option<Vec<String>> {
        match self {
            Shell::Zsh(zsh) => format_shell_script_invocation_with_rc(
                &command,
                &zsh.shell_path,
                &zsh.zshrc_path,
                use_login_shell,
            ),
            Shell::Bash(bash) => format_shell_script_invocation_with_rc(
                &command,
                &bash.shell_path,
                &bash.bashrc_path,
                use_login_shell,
            ),
            Shell::PowerShell(ps) => {
                let mut args = vec![ps.exe.clone()];
                let _ = use_login_shell;
                args.push("-NoProfile".to_string());
                args.push("-Command".to_string());
                args.push(command);
                Some(args)
            }
            Shell::Generic(_) | Shell::Unknown => None,
        }
    }
}

fn format_shell_invocation_with_rc(
    command: &[String],
    shell_path: &str,
    rc_path: &str,
) -> Option<Vec<String>> {
    let joined = extract_script_argument(command)
        .or_else(|| shlex::try_join(command.iter().map(String::as_str)).ok())?;

    let rc_command = if std::path::Path::new(rc_path).exists() {
        format!("source {rc_path} && ({joined})")
    } else {
        joined
    };

    Some(vec![shell_path.to_owned(), "-lc".to_owned(), rc_command])
}

/// Extract the script text from a shell invocation of the form
/// `[shell, "-c"|"-lc", script]` where `shell` is any recognized shell
/// executable.
fn extract_script_argument(command: &[String]) -> Option<String> {
    match command {
        [first, flag, third]
            if is_bash_like(first) && matches!(flag.as_str(), "-c" | "-lc") =>
        {
            Some(third.clone())
        }
        _ => None,
    }
}

fn is_bash_like(cmd: &str) -> bool {
    is_shell_like_executable(cmd)
}

/// Format a raw script string into a shell invocation, optionally sourcing
/// the shell's RC file when `use_login_shell` is true.
fn format_shell_script_invocation_with_rc(
    command: &str,
    shell_path: &str,
    rc_path: &str,
    use_login_shell: bool,
) -> Option<Vec<String>> {
    let shell_flag = if use_login_shell { "-lc" } else { "-c" };
    let rc_command = if use_login_shell && std::path::Path::new(rc_path).exists() {
        format_command_with_rc(rc_path, command)
    } else {
        command.to_string()
    };
    Some(vec![shell_path.to_string(), shell_flag.to_string(), rc_command])
}

fn format_command_with_rc(rc_path: &str, command: &str) -> String {
    if command.contains('\n') || command.contains('\r') {
        format!("source {rc_path} && {{\n{command}\n}}")
    } else {
        format!("source {rc_path} && ({command})")
    }
}

#[cfg(unix)]
fn default_shell_script_invocation(command: String, use_login_shell: bool) -> Vec<String> {
    let shell_flag = if use_login_shell { "-lc" } else { "-c" };
    vec!["sh".to_string(), shell_flag.to_string(), command]
}

#[cfg(target_os = "windows")]
fn default_shell_script_invocation(command: String, _use_login_shell: bool) -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-Command".to_string(),
        command,
    ]
}

#[cfg(all(not(unix), not(target_os = "windows")))]
fn default_shell_script_invocation(command: String, _use_login_shell: bool) -> Vec<String> {
    vec![command]
}

fn generic_shell_expects_script_argument(shell_command: &[String]) -> bool {
    matches!(
        shell_command.last().map(String::as_str),
        Some("-c" | "-lc")
    )
}

/// Returns `true` when a script contains characters that break PowerShell's
/// `-Command` argument parser and should use `-EncodedCommand` instead.
fn needs_powershell_encoding(script: &str) -> bool {
    script.bytes().any(|b| matches!(b, b'\n' | b'\r' | b'$' | b'`' | b'"'))
        || !script.is_ascii()
}

/// Encode a PowerShell script as a `-EncodedCommand` invocation.
///
/// PowerShell's `-EncodedCommand` parameter accepts a Base64-encoded UTF-16LE
/// string, which safely transports multiline scripts, embedded quotes, and all
/// special characters without any shell escaping.
fn encode_powershell_command(exe: &str, script: &str) -> Vec<String> {
    use base64::Engine as _;
    let utf16le: Vec<u8> = script
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&utf16le);
    vec![
        exe.to_owned(),
        "-NoProfile".to_owned(),
        "-EncodedCommand".to_owned(),
        encoded,
    ]
}

#[cfg(unix)]
fn detect_default_user_shell() -> Shell {
    use libc::getpwuid;
    use libc::getuid;
    use std::ffi::CStr;

    unsafe {
        let uid = getuid();
        let pw = getpwuid(uid);

        if !pw.is_null() {
            let shell_path = CStr::from_ptr((*pw).pw_shell)
                .to_string_lossy()
                .into_owned();
            let home_path = CStr::from_ptr((*pw).pw_dir).to_string_lossy().into_owned();
            let base = shell_basename(&shell_path);

            if base == "zsh" {
                return Shell::Zsh(ZshShell {
                    shell_path,
                    zshrc_path: format!("{home_path}/.zshrc"),
                });
            }

            if base == "bash" {
                return Shell::Bash(BashShell {
                    shell_path,
                    bashrc_path: format!("{home_path}/.bashrc"),
                });
            }

            // For non-Bash/Zsh shells, prefer a generic `-c` invocation with a
            // best-effort script style so downstream UX (instructions, safety)
            // can be shell-aware.
            let style = match base.as_str() {
                "sh" | "dash" | "ash" | "ksh" => Some(ShellScriptStyle::PosixSh),
                "nu" => Some(ShellScriptStyle::Nushell),
                "elvish" => Some(ShellScriptStyle::Elvish),
                "fish" => Some(ShellScriptStyle::Fish),
                "xonsh" => Some(ShellScriptStyle::Xonsh),
                "osh" | "oil" => Some(ShellScriptStyle::Oil),
                _ => None,
            };
            if let Some(script_style) = style {
                return Shell::Generic(GenericShell {
                    command: vec![shell_path, "-c".to_owned()],
                    script_style: Some(script_style),
                });
            }
        }
    }
    Shell::Unknown
}

#[cfg(unix)]
pub async fn default_user_shell() -> Shell {
    detect_default_user_shell()
}

#[cfg(target_os = "windows")]
pub async fn default_user_shell() -> Shell {
    use tokio::process::Command;

    // Prefer PowerShell 7+ (`pwsh`) if available, otherwise fall back to Windows PowerShell.
    let has_pwsh = Command::new("pwsh")
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-Command")
        .arg("$PSVersionTable.PSVersion.Major")
        .output()
        .await
        .is_some_and(|o| o.status.success());
    let bash_exe = if Command::new("bash.exe")
        .arg("--version")
        .output()
        .await
        .ok()
        .is_some_and(|o| o.status.success())
    {
        which::which("bash.exe").ok()
    } else {
        None
    };

    if has_pwsh {
        Shell::PowerShell(PowerShellConfig {
            exe: "pwsh.exe".to_string(),
            bash_exe_fallback: bash_exe,
        })
    } else {
        Shell::PowerShell(PowerShellConfig {
            exe: "powershell.exe".to_string(),
            bash_exe_fallback: bash_exe,
        })
    }
}

#[cfg(all(not(target_os = "windows"), not(unix)))]
pub async fn default_user_shell() -> Shell {
    Shell::Unknown
}

/// Resolve the default zshrc path, preferring `$ZDOTDIR` over `$HOME`.
pub(crate) fn default_zshrc_path() -> String {
    if let Ok(zdotdir) = std::env::var("ZDOTDIR") {
        if !zdotdir.is_empty() {
            return format!("{zdotdir}/.zshrc");
        }
    }
    format!("{}/.zshrc", home_dir_path())
}

pub(crate) fn default_bashrc_path() -> String {
    format!("{}/.bashrc", home_dir_path())
}

fn home_dir_path() -> String {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return h;
        }
    }
    // Termux fallback: $TERMUX_PREFIX/../home
    if let Ok(prefix) = std::env::var("TERMUX_PREFIX") {
        let termux_home = std::path::Path::new(&prefix)
            .parent()
            .and_then(|p| p.join("home").to_str().map(str::to_owned));
        if let Some(h) = termux_home {
            tracing::debug!("HOME unset; using Termux home: {h}");
            return h;
        }
    }
    tracing::warn!("HOME is unset and TERMUX_PREFIX fallback failed; RC path will be empty");
    String::new()
}

pub async fn default_user_shell_with_override(shell_override: Option<&ShellConfig>) -> Shell {
    let Some(shell_config) = shell_override else {
        return default_user_shell().await;
    };
    match shell_basename(&shell_config.path).as_str() {
        "zsh" => Shell::Zsh(ZshShell {
            shell_path: shell_config.path.clone(),
            zshrc_path: shell_config.rc_path.clone()
                .unwrap_or_else(default_zshrc_path),
        }),
        "bash" => Shell::Bash(BashShell {
            shell_path: shell_config.path.clone(),
            bashrc_path: shell_config.rc_path.clone()
                .unwrap_or_else(default_bashrc_path),
        }),
        "pwsh" | "powershell" => Shell::PowerShell(PowerShellConfig {
            exe: shell_config.path.clone(),
            bash_exe_fallback: None,
        }),
        _ => Shell::Generic(GenericShell {
            command: std::iter::once(shell_config.path.clone())
                .chain(shell_config.args.clone())
                .collect(),
            script_style: shell_config.script_style,
        }),
    }
}

#[cfg(test)]
mod tests_common {
    use super::*;

    #[test]
    fn shell_basename_handles_versioned_paths() {
        assert_eq!(shell_basename("/usr/local/bin/bash-5.2"), "bash");
        assert_eq!(shell_basename("/opt/homebrew/bin/zsh-5.9.1"), "zsh");
        assert_eq!(shell_basename("fish.exe"), "fish");
        assert_eq!(shell_basename(r#""zsh""#), "zsh");
        assert_eq!(shell_basename("oil-shell"), "oil-shell");
        assert_eq!(shell_basename("/data/data/com.termux/files/usr/bin/bash"), "bash");
        assert_eq!(shell_basename("ZSH"), "zsh");
        assert_eq!(shell_basename("/usr/bin/env"), "env");
        // Versioned Windows executables: strip version first, then .exe
        assert_eq!(shell_basename("bash.exe-5.2"), "bash");
        assert_eq!(shell_basename("zsh.exe-6"), "zsh");
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::string::ToString;

    #[tokio::test]
    async fn test_current_shell_detects_zsh() {
        let shell = Command::new("sh")
            .arg("-c")
            .arg("echo $SHELL")
            .output()
            .unwrap();

        let home = std::env::var("HOME").unwrap();
        let shell_path = String::from_utf8_lossy(&shell.stdout).trim().to_string();
        if shell_path.ends_with("/zsh") {
            assert_eq!(
                default_user_shell().await,
                Shell::Zsh(ZshShell {
                    shell_path: shell_path.clone(),
                    zshrc_path: format!("{home}/.zshrc",),
                })
            );
        }
    }

    #[tokio::test]
    async fn test_run_with_profile_zshrc_not_exists() {
        let shell = Shell::Zsh(ZshShell {
            shell_path: "/bin/zsh".to_string(),
            zshrc_path: "/does/not/exist/.zshrc".to_string(),
        });
        let actual_cmd = shell.format_default_shell_invocation(vec!["myecho".to_string()]);
        assert_eq!(
            actual_cmd,
            Some(vec![
                "/bin/zsh".to_string(),
                "-lc".to_string(),
                "myecho".to_string()
            ])
        );
    }

    #[tokio::test]
    async fn test_run_with_profile_bashrc_not_exists() {
        let shell = Shell::Bash(BashShell {
            shell_path: "/bin/bash".to_string(),
            bashrc_path: "/does/not/exist/.bashrc".to_string(),
        });
        let actual_cmd = shell.format_default_shell_invocation(vec!["myecho".to_string()]);
        assert_eq!(
            actual_cmd,
            Some(vec![
                "/bin/bash".to_string(),
                "-lc".to_string(),
                "myecho".to_string()
            ])
        );
    }

    #[tokio::test]
    async fn test_run_with_profile_bash_escaping_and_execution() {
        let shell_path = "/bin/bash";

        let cases = vec![
            (
                vec!["myecho"],
                vec![shell_path, "-lc", "source BASHRC_PATH && (myecho)"],
                Some("It works!\n"),
            ),
            (
                vec!["bash", "-lc", "echo 'single' \"double\""],
                vec![
                    shell_path,
                    "-lc",
                    "source BASHRC_PATH && (echo 'single' \"double\")",
                ],
                Some("single double\n"),
            ),
        ];

        for (input, expected_cmd, expected_output) in cases {
            use std::collections::HashMap;

            use crate::exec::ExecParams;
            use crate::exec::SandboxType;
            use crate::exec::process_exec_tool_call;
            use crate::protocol::SandboxPolicy;

            let temp_home = tempfile::tempdir().unwrap();
            let bashrc_path = temp_home.path().join(".bashrc");
            std::fs::write(
                &bashrc_path,
                r#"
                    set -x
                    function myecho {
                        echo 'It works!'
                    }
                    "#,
            )
            .unwrap();
            let shell = Shell::Bash(BashShell {
                shell_path: shell_path.to_string(),
                bashrc_path: bashrc_path.to_str().unwrap().to_string(),
            });

            let actual_cmd = shell
                .format_default_shell_invocation(input.iter().map(ToString::to_string).collect());
            let expected_cmd = expected_cmd
                .iter()
                .map(|s| {
                    s.replace("BASHRC_PATH", bashrc_path.to_str().unwrap())
                        
                })
                .collect();

            assert_eq!(actual_cmd, Some(expected_cmd));

            let output = process_exec_tool_call(
                ExecParams {
                    command: actual_cmd.unwrap(),
                    shell_script: None,
                    cwd: PathBuf::from(temp_home.path()),
                    timeout_ms: None,
                    env: HashMap::from([(
                        "HOME".to_string(),
                        temp_home.path().to_str().unwrap().to_string(),
                    )]),
                    sandbox_permissions: Default::default(),
                    additional_permissions: None,
                    justification: None,
                },
                SandboxType::None,
                &SandboxPolicy::DangerFullAccess,
                temp_home.path(),
                &None,
                None,
            )
            .await
            .unwrap();

            assert_eq!(output.exit_code, 0, "input: {input:?} output: {output:?}");
            if let Some(expected) = expected_output {
                assert_eq!(
                    output.stdout.text, expected,
                    "input: {input:?} output: {output:?}"
                );
            }
        }
    }

    #[test]
    fn test_generic_shell_with_lc_wraps_as_single_script_argument() {
        let shell = Shell::Generic(GenericShell {
            command: vec!["bash".to_string(), "-lc".to_string()],
            script_style: None,
        });

        let invocation = shell.format_default_shell_invocation(vec![
            "bash".to_string(),
            "-lc".to_string(),
            "ls -l".to_string(),
        ]);

        assert_eq!(
            invocation,
            Some(vec![
                "bash".to_string(),
                "-lc".to_string(),
                "ls -l".to_string(),
            ])
        );
    }

    #[test]
    fn test_generic_shell_without_c_extends_argv() {
        let shell = Shell::Generic(GenericShell {
            command: vec!["my-shell".to_string(), "--noprofile".to_string()],
            script_style: None,
        });

        let invocation = shell.format_default_shell_invocation(vec![
            "echo".to_string(),
            "hello".to_string(),
        ]);

        assert_eq!(
            invocation,
            Some(vec![
                "my-shell".to_string(),
                "--noprofile".to_string(),
                "echo".to_string(),
                "hello".to_string(),
            ])
        );
    }

    #[test]
    fn test_shell_script_style_prefers_explicit_generic_style() {
        let shell = Shell::Generic(GenericShell {
            command: vec!["bash".to_string(), "-lc".to_string()],
            script_style: Some(ShellScriptStyle::PosixSh),
        });

        assert_eq!(shell.script_style(), Some(ShellScriptStyle::PosixSh));
    }

    #[test]
    fn test_shell_script_style_infers_from_generic_program() {
        let shell = Shell::Generic(GenericShell {
            command: vec!["/bin/zsh".to_string()],
            script_style: None,
        });

        assert_eq!(shell.script_style(), Some(ShellScriptStyle::Zsh));
    }
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod macos_tests {
    use super::*;
    use std::string::ToString;

    #[tokio::test]
    async fn test_run_with_profile_escaping_and_execution() {
        let shell_path = "/bin/zsh";

        let cases = vec![
            (
                vec!["myecho"],
                vec![shell_path, "-lc", "source ZSHRC_PATH && (myecho)"],
                Some("It works!\n"),
            ),
            (
                vec!["myecho"],
                vec![shell_path, "-lc", "source ZSHRC_PATH && (myecho)"],
                Some("It works!\n"),
            ),
            (
                vec!["bash", "-c", "echo 'single' \"double\""],
                vec![
                    shell_path,
                    "-lc",
                    "source ZSHRC_PATH && (echo 'single' \"double\")",
                ],
                Some("single double\n"),
            ),
            (
                vec!["bash", "-lc", "echo 'single' \"double\""],
                vec![
                    shell_path,
                    "-lc",
                    "source ZSHRC_PATH && (echo 'single' \"double\")",
                ],
                Some("single double\n"),
            ),
        ];
        for (input, expected_cmd, expected_output) in cases {
            use std::collections::HashMap;
            use std::path::PathBuf;

            use crate::exec::ExecParams;
            use crate::exec::SandboxType;
            use crate::exec::process_exec_tool_call;
            use crate::protocol::SandboxPolicy;

            // create a temp directory with a zshrc file in it
            let temp_home = tempfile::tempdir().unwrap();
            let zshrc_path = temp_home.path().join(".zshrc");
            std::fs::write(
                &zshrc_path,
                r#"
                    set -x
                    function myecho {
                        echo 'It works!'
                    }
                    "#,
            )
            .unwrap();
            let shell = Shell::Zsh(ZshShell {
                shell_path: shell_path.to_string(),
                zshrc_path: zshrc_path.to_str().unwrap().to_string(),
            });

            let actual_cmd = shell
                .format_default_shell_invocation(input.iter().map(ToString::to_string).collect());
            let expected_cmd = expected_cmd
                .iter()
                .map(|s| {
                    s.replace("ZSHRC_PATH", zshrc_path.to_str().unwrap())
                        
                })
                .collect();

            assert_eq!(actual_cmd, Some(expected_cmd));
            // Actually run the command and check output/exit code
            let output = process_exec_tool_call(
                ExecParams {
                    command: actual_cmd.unwrap(),
                    shell_script: None,
                    cwd: PathBuf::from(temp_home.path()),
                    timeout_ms: None,
                    env: HashMap::from([(
                        "HOME".to_string(),
                        temp_home.path().to_str().unwrap().to_string(),
                    )]),
                    sandbox_permissions: Default::default(),
                    additional_permissions: None,
                    justification: None,
                },
                SandboxType::None,
                &SandboxPolicy::DangerFullAccess,
                temp_home.path(),
                &None,
                None,
            )
            .await
            .unwrap();

            assert_eq!(output.exit_code, 0, "input: {input:?} output: {output:?}");
            if let Some(expected) = expected_output {
                assert_eq!(
                    output.stdout.text, expected,
                    "input: {input:?} output: {output:?}"
                );
            }
        }
    }
}

#[cfg(test)]
#[cfg(target_os = "windows")]
mod tests_windows {
    use super::*;

    #[test]
    fn test_format_default_shell_invocation_powershell() {
        let cases = vec![
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: None,
                }),
                vec!["bash", "-lc", "echo hello"],
                vec!["pwsh.exe", "-NoProfile", "-Command", "echo hello"],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "powershell.exe".to_string(),
                    bash_exe_fallback: None,
                }),
                vec!["bash", "-lc", "echo hello"],
                vec!["powershell.exe", "-NoProfile", "-Command", "echo hello"],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec!["bash", "-lc", "echo hello"],
                vec!["bash.exe", "-lc", "echo hello"],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec![
                    "bash",
                    "-lc",
                    "apply_patch <<'EOF'\n*** Begin Patch\n*** Update File: destination_file.txt\n-original content\n+modified content\n*** End Patch\nEOF",
                ],
                vec![
                    "bash.exe",
                    "-lc",
                    "apply_patch <<'EOF'\n*** Begin Patch\n*** Update File: destination_file.txt\n-original content\n+modified content\n*** End Patch\nEOF",
                ],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec!["echo", "hello"],
                vec!["pwsh.exe", "-NoProfile", "-Command", "echo hello"],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec!["pwsh.exe", "-NoProfile", "-Command", "echo hello"],
                vec!["pwsh.exe", "-NoProfile", "-Command", "echo hello"],
            ),
            (
                // TODO (CODEX_2900): Handle escaping newlines for powershell invocation.
                Shell::PowerShell(PowerShellConfig {
                    exe: "powershell.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec![
                    "code-mcp-server.exe",
                    "--codex-run-as-apply-patch",
                    "*** Begin Patch\n*** Update File: C:\\Users\\person\\destination_file.txt\n-original content\n+modified content\n*** End Patch",
                ],
                vec![
                    "code-mcp-server.exe",
                    "--codex-run-as-apply-patch",
                    "*** Begin Patch\n*** Update File: C:\\Users\\person\\destination_file.txt\n-original content\n+modified content\n*** End Patch",
                ],
            ),
        ];

        for (shell, input, expected_cmd) in cases {
            let actual_cmd = shell
                .format_default_shell_invocation(input.iter().map(|s| (*s).to_string()).collect());
            assert_eq!(
                actual_cmd,
                Some(expected_cmd.iter().map(|s| (*s).to_string()).collect())
            );
        }
    }
}
