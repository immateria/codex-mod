use std::path::Path;

use shlex::Shlex;

pub(super) fn format_inline_shell_for_display(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 3 {
        return None;
    }

    let shell_idx = tokens
        .iter()
        .position(|t| is_shell_invocation_token(t))?;

    let flag_idx = shell_idx + 1;
    if flag_idx >= tokens.len() {
        return None;
    }

    let flag = tokens[flag_idx].as_str();
    if flag != "-c" && flag != "-lc" {
        return None;
    }

    let script_idx = flag_idx + 1;
    if script_idx >= tokens.len() {
        return None;
    }

    format_shell_script(&tokens, script_idx, tokens[script_idx].as_str())
}

fn is_shell_invocation_token(token: &str) -> bool {
    is_shell_executable(token)
}

fn is_shell_executable(token: &str) -> bool {
    let trimmed = token.trim_matches(|c| c == '\'' || c == '"');
    let lowered = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(
        lowered.as_str(),
        "bash"
            | "bash.exe"
            | "sh"
            | "sh.exe"
            | "dash"
            | "dash.exe"
            | "zsh"
            | "zsh.exe"
            | "ksh"
            | "ksh.exe"
            | "busybox",
    )
}

fn format_shell_script(tokens: &[String], script_idx: usize, script: &str) -> Option<String> {
    let block = build_shell_script_block(script)?;
    let mut parts: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        if idx == script_idx {
            parts.push(block.clone());
        } else {
            parts.push(super::escape_token_for_display(token));
        }
    }
    Some(parts.join(" "))
}

fn build_shell_script_block(script: &str) -> Option<String> {
    let normalized = script.replace("\r\n", "\n");
    let segments = split_shell_statements(&normalized);
    let meaningful: Vec<String> = segments
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    if meaningful.len() <= 1 {
        return None;
    }
    let indented = indent_shell_lines(meaningful);
    let mut block = String::from("'\n");
    for line in indented {
        block.push_str("    ");
        block.push_str(line.as_str());
        block.push('\n');
    }
    block.push('\'');
    Some(block)
}

fn split_shell_statements(script: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    let chars: Vec<char> = script.chars().collect();
    let mut idx = 0;
    while idx < chars.len() {
        let ch = chars[idx];
        if escape {
            current.push(ch);
            escape = false;
            idx += 1;
            continue;
        }
        match ch {
            '\\' if in_single || in_double => {
                escape = true;
                current.push(ch);
                idx += 1;
                continue;
            }
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
                idx += 1;
                continue;
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
                idx += 1;
                continue;
            }
            ';' if !(in_single || in_double) => {
                current.push(ch);
                segments.push(current.trim().to_string());
                current.clear();
                idx += 1;
                continue;
            }
            '&' | '|' if !(in_single || in_double) => {
                let current_op = ch;
                if idx + 1 < chars.len() && chars[idx + 1] == current_op {
                    if !current.trim().is_empty() {
                        segments.push(current.trim().to_string());
                    }
                    segments.push(format!("{current_op}{current_op}"));
                    current.clear();
                    idx += 2;
                    continue;
                }
            }
            '\n' if !(in_single || in_double) => {
                segments.push(current.trim().to_string());
                current.clear();
                idx += 1;
                continue;
            }
            _ => {}
        }
        current.push(ch);
        idx += 1;
    }

    if !current.trim().is_empty() {
        segments.push(current.trim().to_string());
    }

    segments
}

fn indent_shell_lines(lines: Vec<String>) -> Vec<String> {
    let mut indented: Vec<String> = Vec::with_capacity(lines.len());
    let mut indent_level: usize = 0;

    for raw in lines {
        if raw == "&&" || raw == "||" {
            let mut line = String::new();
            for _ in 0..indent_level {
                line.push_str("    ");
            }
            line.push_str(raw.as_str());
            indented.push(line);
            continue;
        }

        let trimmed = raw.trim();
        if trimmed.is_empty() {
            indented.push(String::new());
            continue;
        }

        if trimmed.starts_with("fi")
            || trimmed.starts_with("done")
            || trimmed.starts_with("esac")
        {
            indent_level = indent_level.saturating_sub(1);
        }

        let mut line = String::new();
        for _ in 0..indent_level {
            line.push_str("    ");
        }
        line.push_str(trimmed);
        indented.push(line);

        if trimmed.ends_with("do")
            || trimmed.ends_with("then")
            || trimmed.ends_with('{')
            || trimmed.starts_with("case ")
        {
            indent_level += 1;
        }
    }

    indented
}

