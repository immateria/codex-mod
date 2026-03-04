use crate::command_safety::is_safe_command::is_safe_to_call_with_exec;
use crate::invocation;

pub(super) fn is_safe_cmd_script(mode: &str, script: &str) -> bool {
    let mode_lc = mode.to_ascii_lowercase();
    if !matches!(mode_lc.as_str(), "/c" | "/r" | "-c") {
        // Reject `/k` because it leaves an interactive shell.
        return false;
    }

    let Some(segments) = invocation::split_cmd_script_into_segments(script) else {
        return false;
    };

    !segments.is_empty() && segments.iter().all(|seg| is_safe_cmd_segment(seg))
}

fn is_safe_cmd_segment(segment: &[String]) -> bool {
    let Some(cmd0) = segment.first().map(std::string::String::as_str) else {
        return false;
    };

    // Minimal read-only built-in set (cmd.exe).
    if matches!(cmd0.to_ascii_lowercase().as_str(), "dir" | "type" | "where" | "findstr") {
        return true;
    }

    is_safe_to_call_with_exec(segment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_safe_allows_minimal_read_only_builtins() {
        assert!(is_safe_cmd_script("/c", "dir"));
        assert!(is_safe_cmd_script("/c", "where git"));
    }

    #[test]
    fn cmd_safe_rejects_interactive_k() {
        assert!(!is_safe_cmd_script("/k", "dir"));
    }

    #[test]
    fn cmd_safe_rejects_redirection() {
        assert!(!is_safe_cmd_script("/c", "dir > out.txt"));
    }
}

