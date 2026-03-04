pub(crate) fn script_contains_redirection(script: &str) -> bool {
    script.contains('>') || script.contains('<')
}

/// Split a `cmd.exe` script string into word-only command segments.
///
/// This is intentionally conservative:
/// - rejects if the script contains redirections (`>` or `<`)
/// - splits on `&`, `&&`, `||`, `|`
/// - tokenizes with a simple whitespace+quotes scanner (not full CMD parsing)
pub(crate) fn split_cmd_script_into_segments(script: &str) -> Option<Vec<Vec<String>>> {
    let script = script.trim();
    if script.is_empty() {
        return None;
    }
    if script_contains_redirection(script) {
        return None;
    }

    let tokens = tokenize_cmd_script(script)?;
    if tokens.is_empty() {
        return None;
    }

    const SEPS: &[&str] = &["&", "&&", "|", "||"];
    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for tok in tokens {
        if SEPS.contains(&tok.as_str()) {
            if current.is_empty() {
                return None;
            }
            segments.push(std::mem::take(&mut current));
        } else {
            current.push(tok);
        }
    }
    if current.is_empty() {
        return None;
    }
    segments.push(current);
    Some(segments)
}

fn tokenize_cmd_script(script: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;

    let chars: Vec<char> = script.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                cur.push(ch);
            }
            i += 1;
            continue;
        }

        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                i += 1;
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                i += 1;
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
            }
            '&' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                if i + 1 < chars.len() && chars[i + 1] == '&' {
                    out.push("&&".to_string());
                    i += 2;
                } else {
                    out.push("&".to_string());
                    i += 1;
                }
            }
            '|' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                if i + 1 < chars.len() && chars[i + 1] == '|' {
                    out.push("||".to_string());
                    i += 2;
                } else {
                    out.push("|".to_string());
                    i += 1;
                }
            }
            _ => {
                cur.push(ch);
                i += 1;
            }
        }
    }

    if quote.is_some() {
        // Unbalanced quotes -> reject rather than guessing.
        return None;
    }

    if !cur.is_empty() {
        out.push(cur);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_connectors() {
        assert_eq!(
            split_cmd_script_into_segments("dir && where git"),
            Some(vec![
                vec!["dir".to_string()],
                vec!["where".to_string(), "git".to_string()],
            ])
        );
    }

    #[test]
    fn rejects_redirection() {
        assert_eq!(split_cmd_script_into_segments("dir > out.txt"), None);
    }
}
