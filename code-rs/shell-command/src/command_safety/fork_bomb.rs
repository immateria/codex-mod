// High-confidence heuristics for detecting classic POSIX fork bombs.
//
// This is intentionally conservative: it focuses on patterns that are strongly
// associated with fork bombs and avoids trying to fully parse shell syntax.

fn strip_quoted_regions(script: &str) -> String {
    let mut out = String::with_capacity(script.len());
    let mut chars = script.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(ch) = chars.next() {
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }

        if in_double {
            if ch == '\\' {
                // Skip escaped character inside double quotes.
                let _ = chars.next();
                continue;
            }
            if ch == '"' {
                in_double = false;
                continue;
            }
            continue;
        }

        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            _ => out.push(ch),
        }
    }

    out
}

fn normalize_for_match(script: &str) -> String {
    strip_quoted_regions(script)
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect()
}

fn is_name_byte(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b':')
}

fn is_name_char(ch: char) -> bool {
    ch.is_ascii() && is_name_byte(ch as u8)
}

fn contains_standalone_invocation(text: &str, name: &str) -> bool {
    text.split(|ch: char| !is_name_char(ch))
        .any(|tok| tok == name)
}

fn find_matching_brace(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth: i32 = 1;
    let mut idx = start;
    while idx < bytes.len() {
        match bytes[idx] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
                if depth < 0 {
                    // Malformed: more closing braces than opening braces.
                    return None;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    None
}

fn body_looks_like_self_fanout(body: &str, name: &str) -> bool {
    // Common fork-bomb fanout patterns:
    // - self recursion via pipe: `name|name`
    // - self recursion via background: `name&name` (often repeated)
    //
    // This remains a heuristic; we intentionally keep it simple and focus on
    // high-confidence matches.
    let mut self_pipe = String::with_capacity(name.len().saturating_mul(2).saturating_add(1));
    self_pipe.push_str(name);
    self_pipe.push('|');
    self_pipe.push_str(name);

    let mut self_bg = String::with_capacity(name.len().saturating_mul(2).saturating_add(1));
    self_bg.push_str(name);
    self_bg.push('&');
    self_bg.push_str(name);

    body.contains(&self_pipe) || body.contains(&self_bg)
}

fn contains_function_fork_bomb(normalized: &str) -> bool {
    let bytes = normalized.as_bytes();
    let mut search_from = 0;

    while search_from < normalized.len() {
        let Some(rel) = normalized[search_from..].find("(){") else {
            return false;
        };
        let pos = search_from + rel;

        // Identify the function name immediately preceding the "(){".
        let mut name_start = pos;
        while name_start > 0 && is_name_byte(bytes[name_start - 1]) {
            name_start -= 1;
        }
        if name_start == pos {
            search_from = pos.saturating_add(3);
            continue;
        }
        let name = &normalized[name_start..pos];

        // Scan forward to find the matching closing brace for the function body.
        let Some(body_end) = find_matching_brace(bytes, pos + 3) else {
            return false;
        };

        let body = &normalized[pos + 3..body_end];
        let rest = normalized.get(body_end + 1..).unwrap_or_default();

        // Fork-bomb-like behavior: the function fans out by invoking itself
        // in a pipeline or in the background, then the script invokes the
        // function again after the definition.
        if body_looks_like_self_fanout(body, name) && contains_standalone_invocation(rest, name) {
            return true;
        }

        search_from = body_end.saturating_add(1);
    }

    false
}

fn contains_function_keyword_fork_bomb(normalized: &str) -> bool {
    let bytes = normalized.as_bytes();
    let mut search_from = 0;
    const KW: &str = "function";

    while search_from < normalized.len() {
        let Some(rel) = normalized[search_from..].find(KW) else {
            return false;
        };
        let pos = search_from + rel;

        // Require a token boundary before `function` to avoid matching "dysfunction".
        if pos > 0 && is_name_byte(bytes[pos - 1]) {
            search_from = pos.saturating_add(KW.len());
            continue;
        }

        let name_start = pos + KW.len();
        if name_start >= normalized.len() {
            return false;
        }

        // Parse the function name immediately after "function".
        let mut name_end = name_start;
        while name_end < normalized.len() && is_name_byte(bytes[name_end]) {
            name_end += 1;
        }
        if name_end == name_start {
            search_from = pos.saturating_add(KW.len());
            continue;
        }
        let name = &normalized[name_start..name_end];

        // Support both:
        // - `function name { ... }`
        // - `function name(){ ... }`
        let (body_start, continue_from) = if normalized[name_end..].starts_with("(){") {
            (name_end + 3, name_end + 3)
        } else if bytes.get(name_end) == Some(&b'{') {
            (name_end + 1, name_end + 1)
        } else {
            search_from = pos.saturating_add(KW.len());
            continue;
        };

        let Some(body_end) = find_matching_brace(bytes, body_start) else {
            return false;
        };

        let body = &normalized[body_start..body_end];
        let rest = normalized.get(body_end + 1..).unwrap_or_default();
        if body_looks_like_self_fanout(body, name) && contains_standalone_invocation(rest, name) {
            return true;
        }

        search_from = continue_from.max(body_end.saturating_add(1));
    }

    false
}

pub(crate) fn looks_like_posix_fork_bomb(script: &str) -> bool {
    let normalized = normalize_for_match(script);
    if normalized.contains(":(){:|:&};:") {
        return true;
    }
    contains_function_fork_bomb(&normalized) || contains_function_keyword_fork_bomb(&normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_classic_colon_fork_bomb() {
        let script = ":(){ :|:& };:\n";
        assert!(looks_like_posix_fork_bomb(script));
    }

    #[test]
    fn detects_named_function_fork_bomb() {
        let script = "bomb(){bomb|bomb&};bomb";
        assert!(looks_like_posix_fork_bomb(script));
    }

    #[test]
    fn detects_background_only_fork_bomb() {
        let script = "bomb(){ bomb& bomb& }; bomb";
        assert!(looks_like_posix_fork_bomb(script));
    }

    #[test]
    fn detects_function_keyword_form() {
        let script = "function bomb { bomb|bomb& }; bomb";
        assert!(looks_like_posix_fork_bomb(script));
    }

    #[test]
    fn does_not_trigger_on_quoted_pattern() {
        let script = "echo ':(){ :|:& };:'";
        assert!(!looks_like_posix_fork_bomb(script));
    }

    #[test]
    fn does_not_trigger_on_benign_function() {
        let script = "foo(){ echo hi; }; foo";
        assert!(!looks_like_posix_fork_bomb(script));
    }
}
