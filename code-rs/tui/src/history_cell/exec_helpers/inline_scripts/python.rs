use std::path::Path;

use shlex::Shlex;

pub(super) fn try_format_inline_python(command_escaped: &str) -> Option<String> {
    if let Some(formatted) = format_python_dash_c(command_escaped) {
        return Some(formatted);
    }
    if let Some(formatted) = format_python_heredoc(command_escaped) {
        return Some(formatted);
    }
    None
}

fn format_python_dash_c(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 3 {
        return None;
    }

    let python_idx = tokens
        .iter()
        .position(|token| is_python_invocation_token(token))?;

    let c_idx = tokens
        .iter()
        .enumerate()
        .skip(python_idx + 1)
        .find_map(|(idx, token)| if token == "-c" { Some(idx) } else { None })?;

    let script_idx = c_idx + 1;
    if script_idx >= tokens.len() {
        return None;
    }

    let script_raw = tokens[script_idx].as_str();
    if script_raw.is_empty() {
        return None;
    }

    let script_block = build_python_script_block(script_raw)?;

    let mut parts: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        if idx == script_idx {
            parts.push(script_block.clone());
        } else {
            parts.push(super::escape_token_for_display(token));
        }
    }

    Some(parts.join(" "))
}

fn build_python_script_block(script: &str) -> Option<String> {
    let normalized = script.replace("\r\n", "\n");
    let lines: Vec<String> = if normalized.contains('\n') {
        normalized
            .lines()
            .map(|line| line.trim_end().to_string())
            .collect()
    } else if script_has_semicolon_outside_quotes(&normalized) {
        split_semicolon_statements(&normalized)
    } else {
        return None;
    };

    let meaningful: Vec<String> = merge_from_import_lines(lines)
        .into_iter()
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.trim().is_empty())
        .collect();

    if meaningful.len() <= 1 {
        return None;
    }

    let indented = indent_python_lines(meaningful);

    let mut block = String::from("'\n");
    for line in indented {
        block.push_str("    ");
        block.push_str(line.as_str());
        block.push('\n');
    }
    block.push('\'');
    Some(block)
}

fn format_python_heredoc(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 3 {
        return None;
    }

    let python_idx = tokens
        .iter()
        .position(|token| is_python_invocation_token(token))?;

    let heredoc_idx = tokens
        .iter()
        .enumerate()
        .skip(python_idx + 1)
        .find_map(|(idx, token)| heredoc_delimiter(token).map(|delim| (idx, delim)))?;

    let (marker_idx, terminator) = heredoc_idx;
    let closing_idx = tokens
        .iter()
        .enumerate()
        .skip(marker_idx + 1)
        .rev()
        .find_map(|(idx, token)| (token == &terminator).then_some(idx))?;

    if closing_idx <= marker_idx + 1 {
        return None;
    }

    let script_tokens = &tokens[marker_idx + 1..closing_idx];
    if script_tokens.is_empty() {
        return None;
    }

    let script_lines = split_heredoc_script_lines(script_tokens);
    if script_lines.is_empty() {
        return None;
    }

    let script_lines = indent_python_lines(merge_from_import_lines(script_lines));

    let header_tokens: Vec<String> = tokens[..=marker_idx]
        .iter()
        .map(|t| super::escape_token_for_display(t))
        .collect();

    let mut result = header_tokens.join(" ");
    if !result.ends_with('\n') {
        result.push('\n');
    }

    for line in script_lines {
        result.push_str("    ");
        result.push_str(line.trim_end());
        result.push('\n');
    }

    result.push_str(&super::escape_token_for_display(&tokens[closing_idx]));

    if closing_idx + 1 < tokens.len() {
        let tail: Vec<String> = tokens[closing_idx + 1..]
            .iter()
            .map(|t| super::escape_token_for_display(t))
            .collect();
        if !tail.is_empty() {
            result.push(' ');
            result.push_str(&tail.join(" "));
        }
    }

    Some(result)
}

fn heredoc_delimiter(token: &str) -> Option<String> {
    if !token.starts_with("<<") {
        return None;
    }
    let mut delim = token.trim_start_matches("<<").to_string();
    if delim.is_empty() {
        return None;
    }
    if delim.len() >= 2
        && let Some(inner) = delim.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
            .or_else(|| delim.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
    {
        delim = inner.to_string();
    }
    if delim.is_empty() {
        None
    } else {
        Some(delim)
    }
}

fn split_heredoc_script_lines(script_tokens: &[String]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut current_has_assignment = false;

    for (idx, token) in script_tokens.iter().enumerate() {
        if !current.is_empty()
            && paren_depth == 0
            && bracket_depth == 0
            && brace_depth == 0
        {
            let token_lower = token.to_ascii_lowercase();
            let current_first = current.first().map(|s| s.to_ascii_lowercase());
            let should_flush_before = is_statement_boundary_token(token)
                && !(token_lower == "import" && current_first.as_deref() == Some("from"));
            if should_flush_before {
                let line = current.join(" ");
                lines.push(line.trim().to_string());
                current.clear();
                current_has_assignment = false;
            }
        }

        current.push(token.clone());
        adjust_bracket_depth(token, &mut paren_depth, &mut bracket_depth, &mut brace_depth);

        if is_assignment_operator(token) {
            current_has_assignment = true;
        }

        let next = script_tokens.get(idx + 1);
        let mut should_break = false;
        let mut break_here = false;

        if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            if next.is_none() {
                should_break = true;
            } else {
                let Some(next_token) = next else {
                    continue;
                };
                if is_statement_boundary_token(next_token) {
                    should_break = true;
                } else if current
                    .first()
                    .map(|s| s.as_str() == "import" || s.as_str() == "from")
                    .unwrap_or(false)
                {
                    if current.len() > 1 && next_token != "as" && next_token != "," {
                        should_break = true;
                    }
                } else if current_has_assignment
                    && !is_assignment_operator(token)
                    && next_token
                        .chars()
                        .next()
                        .map(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                        .unwrap_or(false)
                    && !next_token.contains('(')
                {
                    should_break = true;
                }

                let token_trimmed = token.trim_matches(|c| c == ')' || c == ']' || c == '}');
                if token_trimmed.ends_with(':') {
                    break_here = true;
                }

                let lowered = token.trim().to_ascii_lowercase();
                if matches!(lowered.as_str(), "return" | "break" | "continue" | "pass") {
                    break_here = true;
                }

                if let Some(next_token) = next {
                    let next_str = next_token.as_str();
                    if token.ends_with(')')
                        && (next_str.contains('.')
                            || next_str.contains('=')
                            || next_str.starts_with("print"))
                    {
                        break_here = true;
                    }
                }
            }
        }

        if break_here {
            let line = current.join(" ");
            lines.push(line.trim().to_string());
            current.clear();
            current_has_assignment = false;
            continue;
        }

        if should_break {
            let line = current.join(" ");
            lines.push(line.trim().to_string());
            current.clear();
            current_has_assignment = false;
        }
    }

    if !current.is_empty() {
        let line = current.join(" ");
        lines.push(line.trim().to_string());
    }

    lines
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect()
}

fn is_statement_boundary_token(token: &str) -> bool {
    matches!(
        token,
        "import"
            | "from"
            | "def"
            | "class"
            | "if"
            | "elif"
            | "else"
            | "for"
            | "while"
            | "try"
            | "except"
            | "with"
            | "return"
            | "raise"
            | "pass"
            | "continue"
            | "break"
    ) || token.starts_with("print")
}

fn indent_python_lines(lines: Vec<String>) -> Vec<String> {
    let mut indented: Vec<String> = Vec::with_capacity(lines.len());
    let mut indent_level: usize = 0;
    let mut pending_dedent_after_flow = false;

    for raw in lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            indented.push(String::new());
            continue;
        }

        let lowered_first = trimmed
            .split_whitespace()
            .next()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        if pending_dedent_after_flow
            && !matches!(
                lowered_first.as_str(),
                "elif" | "else" | "except" | "finally",
            )
        {
            indent_level = indent_level.saturating_sub(1);
        }
        pending_dedent_after_flow = false;

        if matches!(
            lowered_first.as_str(),
            "elif" | "else" | "except" | "finally",
        ) {
            indent_level = indent_level.saturating_sub(1);
        }

        let mut line = String::with_capacity(trimmed.len() + indent_level * 4);
        for _ in 0..indent_level {
            line.push_str("    ");
        }
        line.push_str(trimmed);
        indented.push(line);

        if trimmed.ends_with(':')
            && !matches!(
                lowered_first.as_str(),
                "return" | "break" | "continue" | "pass" | "raise",
            )
        {
            indent_level += 1;
        } else if matches!(
            lowered_first.as_str(),
            "return" | "break" | "continue" | "pass" | "raise",
        ) {
            pending_dedent_after_flow = true;
        }
    }

    indented
}

fn merge_from_import_lines(lines: Vec<String>) -> Vec<String> {
    let mut merged: Vec<String> = Vec::with_capacity(lines.len());
    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx].trim().to_string();
        if line.starts_with("from ")
            && idx + 1 < lines.len()
            && lines[idx + 1].trim_start().starts_with("import ")
        {
            let combined = format!("{} {}", line.trim_end(), lines[idx + 1].trim_start());
            merged.push(combined);
            idx += 2;
        } else {
            merged.push(line);
            idx += 1;
        }
    }
    merged
}

fn is_assignment_operator(token: &str) -> bool {
    matches!(
        token,
        "="
            | "+="
            | "-="
            | "*="
            | "/="
            | "//="
            | "%="
            | "^="
            | "|="
            | "&="
            | "**="
            | "<<="
            | ">>=",
    )
}

fn adjust_bracket_depth(token: &str, paren: &mut i32, bracket: &mut i32, brace: &mut i32) {
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    for ch in token.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double => {
                escape = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            '(' if !(in_single || in_double) => *paren += 1,
            ')' if !(in_single || in_double) => *paren -= 1,
            '[' if !(in_single || in_double) => *bracket += 1,
            ']' if !(in_single || in_double) => *bracket -= 1,
            '{' if !(in_single || in_double) => *brace += 1,
            '}' if !(in_single || in_double) => *brace -= 1,
            _ => {}
        }
    }
    *paren = (*paren).max(0);
    *bracket = (*bracket).max(0);
    *brace = (*brace).max(0);
}

fn is_python_invocation_token(token: &str) -> bool {
    if token.is_empty() || token.contains('=') {
        return false;
    }

    let trimmed = token.trim_matches(|c| c == '\'' || c == '"');
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();

    if !base.starts_with("python") {
        return false;
    }

    let suffix = &base["python".len()..];
    suffix.is_empty()
        || suffix
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == 'w')
}

fn script_has_semicolon_outside_quotes(script: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in script.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double => {
                escape = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            ';' if !in_single && !in_double => return true,
            _ => {}
        }
    }

    false
}

fn split_semicolon_statements(script: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in script.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_single || in_double => {
                escape = true;
                current.push(ch);
            }
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
            }
            ';' if !in_single && !in_double => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    segments.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }

    segments
}

