use std::path::Path;

use shlex::Shlex;

pub(super) fn format_inline_node_for_display(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 2 {
        return None;
    }

    let node_idx = tokens
        .iter()
        .position(|token| is_node_invocation_token(token))?;

    let mut idx = node_idx + 1;
    while idx < tokens.len() {
        match tokens[idx].as_str() {
            "-e" | "--eval" | "-p" | "--print" => {
                let script_idx = idx + 1;
                if script_idx >= tokens.len() {
                    return None;
                }
                return format_node_script(&tokens, script_idx, tokens[script_idx].as_str());
            }
            "--" => break,
            _ => idx += 1,
        }
    }

    None
}

fn is_node_invocation_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|c| c == '\'' || c == '"');
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(base.as_str(), "node" | "node.exe" | "nodejs" | "nodejs.exe")
}

fn format_node_script(tokens: &[String], script_idx: usize, script: &str) -> Option<String> {
    let block = build_js_script_block(script)?;
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

fn build_js_script_block(script: &str) -> Option<String> {
    let normalized = script.replace("\r\n", "\n");
    let lines: Vec<String> = if normalized.contains('\n') {
        normalized
            .lines()
            .map(|line| line.trim_end().to_string())
            .collect()
    } else {
        split_js_statements(&normalized)
    };

    let meaningful: Vec<String> = lines
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    if meaningful.len() <= 1 {
        return None;
    }

    let indented = indent_js_lines(meaningful);
    let mut block = String::from("'\n");
    for line in indented {
        block.push_str("    ");
        block.push_str(line.as_str());
        block.push('\n');
    }
    block.push('\'');
    Some(block)
}

fn split_js_statements(script: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for ch in script.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
                current.push(ch);
                continue;
            }
            '\'' if !in_double && !in_backtick => {
                in_single = !in_single;
                current.push(ch);
                continue;
            }
            '"' if !in_single && !in_backtick => {
                in_double = !in_double;
                current.push(ch);
                continue;
            }
            '`' if !in_single && !in_double => {
                in_backtick = !in_backtick;
                current.push(ch);
                continue;
            }
            _ => {}
        }

        if !(in_single || in_double || in_backtick) {
            match ch {
                '{' => brace_depth += 1,
                '}' => {
                    if brace_depth > 0 {
                        brace_depth -= 1;
                    }
                }
                '(' => paren_depth += 1,
                ')' => {
                    if paren_depth > 0 {
                        paren_depth -= 1;
                    }
                }
                '[' => bracket_depth += 1,
                ']' => {
                    if bracket_depth > 0 {
                        bracket_depth -= 1;
                    }
                }
                ';' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                    current.push(ch);
                    let seg = current.trim().to_string();
                    if !seg.is_empty() {
                        segments.push(seg);
                    }
                    current.clear();
                    continue;
                }
                '\n' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                    let seg = current.trim().to_string();
                    if !seg.is_empty() {
                        segments.push(seg);
                    }
                    current.clear();
                    continue;
                }
                _ => {}
            }
        }

        current.push(ch);
    }

    let seg = current.trim().to_string();
    if !seg.is_empty() {
        segments.push(seg);
    }
    segments
}

fn indent_js_lines(lines: Vec<String>) -> Vec<String> {
    let mut indented: Vec<String> = Vec::with_capacity(lines.len());
    let mut indent_level: usize = 0;

    for raw in lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            indented.push(String::new());
            continue;
        }

        let mut leading_closers = 0usize;
        let mut cut = trimmed.len();
        for (idx, ch) in trimmed.char_indices() {
            match ch {
                '}' | ']' => {
                    leading_closers += 1;
                    cut = idx + ch.len_utf8();
                    continue;
                }
                _ => {
                    cut = idx;
                    break;
                }
            }
        }

        if leading_closers > 0 && cut >= trimmed.len() {
            cut = trimmed.len();
        }

        if leading_closers > 0 {
            indent_level = indent_level.saturating_sub(leading_closers);
        }

        let remainder = trimmed[cut..].trim_start();
        let mut line = String::with_capacity(remainder.len() + indent_level * 4);
        for _ in 0..indent_level {
            line.push_str("    ");
        }
        if remainder.is_empty() && cut < trimmed.len() {
            line.push_str(trimmed);
        } else {
            line.push_str(remainder);
        }
        indented.push(line);

        let (opens, closes) = js_brace_deltas(trimmed);
        indent_level += opens;
        indent_level = indent_level.saturating_sub(closes);
    }

    indented
}

fn js_brace_deltas(line: &str) -> (usize, usize) {
    let mut opens = 0usize;
    let mut closes = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;

    for ch in line.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
            }
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            '{' if !(in_single || in_double || in_backtick) => opens += 1,
            '}' if !(in_single || in_double || in_backtick) => closes += 1,
            _ => {}
        }
    }

    (opens, closes)
}

