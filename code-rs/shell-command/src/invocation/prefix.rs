use std::path::Path;

use super::PrefixWrappers;

/// Peel a small set of common prefix wrappers (best-effort) so safety and
/// summarization logic can analyze the underlying command.
///
/// This is intentionally conservative and only handles simple, common forms.
pub(crate) fn peel_prefix_wrappers(command: &[String]) -> (PrefixWrappers, Vec<String>) {
    let mut meta = PrefixWrappers::default();
    let mut current: &[String] = command;

    loop {
        if let Some(next) = peel_env(current) {
            meta.env = true;
            current = next;
            continue;
        }
        if let Some(next) = peel_sudo(current) {
            meta.sudo = true;
            current = next;
            continue;
        }
        break;
    }

    (meta, current.to_vec())
}

fn peel_env(command: &[String]) -> Option<&[String]> {
    let (first, rest) = command.split_first()?;
    if !is_env_executable(first) {
        return None;
    }
    if rest.is_empty() {
        return None;
    }

    let mut idx = 0;
    while idx < rest.len() {
        let tok = &rest[idx];
        if tok == "--" {
            idx += 1;
            break;
        }

        // env supports a range of flags; peel only a small, common subset.
        match tok.as_str() {
            "-i" | "--ignore-environment" => {
                idx += 1;
                continue;
            }
            "-u" => {
                // `env -u NAME ...`
                if idx + 1 >= rest.len() {
                    return None;
                }
                idx += 2;
                continue;
            }
            _ => {}
        }
        if tok.starts_with("--unset=") {
            idx += 1;
            continue;
        }

        // Unknown flags: don't peel rather than guessing.
        if tok.starts_with('-') {
            return None;
        }

        if tok.contains('=') && !tok.starts_with('=') {
            idx += 1;
            continue;
        }

        break;
    }

    rest.get(idx..).filter(|remaining| !remaining.is_empty())
}

fn peel_sudo(command: &[String]) -> Option<&[String]> {
    let (first, rest) = command.split_first()?;
    if !is_sudo_executable(first) {
        return None;
    }

    let mut idx = 0;
    while idx < rest.len() {
        let tok = &rest[idx];

        if tok == "--" {
            idx += 1;
            break;
        }

        // Long options: support a small subset that commonly appears in logs.
        // We only need enough correctness to avoid mis-identifying the command.
        if tok.starts_with("--") {
            if is_sudo_long_flag_with_inline_value(tok) {
                idx += 1;
                continue;
            }
            if is_sudo_long_flag_with_value(tok) {
                idx += 1;
                if idx < rest.len() {
                    idx += 1;
                }
                continue;
            }

            // Unknown long flag: treat as no-arg and continue scanning.
            idx += 1;
            continue;
        }

        if !tok.starts_with('-') || tok == "-" {
            break;
        }

        // Flags that take a value.
        if is_sudo_flag_with_value(tok) {
            // Inline value form like `-uuser` consumes only this token.
            if tok.len() > 2 {
                idx += 1;
                continue;
            }

            // Split value form like `-u user` consumes this token and the next.
            idx += 1;
            if idx < rest.len() {
                idx += 1;
            }
            continue;
        }

        // Common no-arg flags.
        idx += 1;
    }

    // Even if `sudo` is provided without an inner command, treat it as present
    // so safety logic can conservatively classify it.
    rest.get(idx..)
}

fn is_sudo_flag_with_value(tok: &str) -> bool {
    matches!(tok, "-u" | "-g" | "-h" | "-p" | "-C")
        || tok.starts_with("-u")
        || tok.starts_with("-g")
        || tok.starts_with("-h")
        || tok.starts_with("-p")
        || tok.starts_with("-C")
}

fn is_sudo_long_flag_with_value(tok: &str) -> bool {
    matches!(
        tok,
        "--user"
            | "--group"
            | "--host"
            | "--prompt"
            | "--close-from"
            | "--command"
            | "--chdir"
            | "--login-class"
            | "--other-user"
    )
}

fn is_sudo_long_flag_with_inline_value(tok: &str) -> bool {
    matches!(
        tok,
        s if s.starts_with("--user=")
            || s.starts_with("--group=")
            || s.starts_with("--host=")
            || s.starts_with("--prompt=")
            || s.starts_with("--close-from=")
            || s.starts_with("--command=")
            || s.starts_with("--chdir=")
            || s.starts_with("--login-class=")
            || s.starts_with("--other-user=")
    )
}

fn is_env_executable(exe: &str) -> bool {
    let executable_name = Path::new(exe)
        .file_name()
        .and_then(|osstr| osstr.to_str())
        .unwrap_or(exe)
        .to_ascii_lowercase();
    matches!(executable_name.as_str(), "env" | "env.exe")
}

fn is_sudo_executable(exe: &str) -> bool {
    let executable_name = Path::new(exe)
        .file_name()
        .and_then(|osstr| osstr.to_str())
        .unwrap_or(exe)
        .to_ascii_lowercase();
    matches!(executable_name.as_str(), "sudo" | "sudo.exe")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_str(args: &[&str]) -> Vec<String> {
        args.iter().map(std::string::ToString::to_string).collect()
    }

    #[test]
    fn peel_env_handles_common_flags_and_assignments() {
        let (meta, peeled) = peel_prefix_wrappers(&vec_str(&["env", "-i", "FOO=bar", "ls"]));
        assert!(meta.env);
        assert_eq!(peeled, vec_str(&["ls"]));
    }

    #[test]
    fn peel_env_refuses_unknown_flags() {
        let original = vec_str(&["env", "-Z", "FOO=bar", "ls"]);
        let (meta, peeled) = peel_prefix_wrappers(&original);
        assert!(!meta.env);
        assert_eq!(peeled, original);
    }

    #[test]
    fn peel_sudo_handles_long_user_flag() {
        let (meta, peeled) = peel_prefix_wrappers(&vec_str(&["sudo", "--user", "bob", "ls"]));
        assert!(meta.sudo);
        assert_eq!(peeled, vec_str(&["ls"]));
    }

    #[test]
    fn peel_sudo_is_still_detected_without_inner_command() {
        let (meta, peeled) = peel_prefix_wrappers(&vec_str(&["sudo"]));
        assert!(meta.sudo);
        assert!(peeled.is_empty());
    }
}
