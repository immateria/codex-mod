use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

pub(crate) fn normalize_shell_command_display(cmd: &str) -> String {
    let first_non_ws = cmd
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(idx, _)| idx);
    let Some(start) = first_non_ws else {
        return cmd.to_string();
    };
    if cmd[start..].starts_with("./") {
        let mut normalized = String::with_capacity(cmd.len().saturating_sub(2));
        normalized.push_str(&cmd[..start]);
        normalized.push_str(&cmd[start + 2..]);
        normalized
    } else {
        cmd.to_string()
    }
}

pub(crate) fn insert_line_breaks_after_double_ampersand(cmd: &str) -> String {
    if !cmd.contains("&&") {
        return cmd.to_string();
    }

    let mut result = String::with_capacity(cmd.len() + 8);
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;

    while i < cmd.len() {
        let Some(ch) = cmd[i..].chars().next() else {
            break;
        };
        let ch_len = ch.len_utf8();

        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                result.push(ch);
                i += ch_len;
                continue;
            }
            '"' if !in_single => {
                in_double = !in_double;
                result.push(ch);
                i += ch_len;
                continue;
            }
            '&' if !in_single && !in_double => {
                let next_idx = i + ch_len;
                if next_idx < cmd.len()
                    && let Some(next_ch) = cmd[next_idx..].chars().next()
                    && next_ch == '&'
                {
                    result.push('&');
                    result.push('&');
                    i = next_idx + next_ch.len_utf8();
                    while i < cmd.len() {
                        let Some(ahead) = cmd[i..].chars().next() else {
                            break;
                        };
                        if ahead.is_whitespace() {
                            i += ahead.len_utf8();
                            continue;
                        }
                        break;
                    }
                    if i < cmd.len() {
                        result.push('\n');
                    }
                    continue;
                }
            }
            _ => {}
        }

        result.push(ch);
        i += ch_len;
    }

    result
}

pub(crate) fn emphasize_shell_command_name(line: &mut Line<'static>) {
    let mut emphasized = false;
    let mut rebuilt: Vec<Span<'static>> = Vec::with_capacity(line.spans.len());

    for span in line.spans.drain(..) {
        if emphasized {
            rebuilt.push(span);
            continue;
        }

        let style = span.style;
        let content_owned = span.content.into_owned();

        if content_owned.trim().is_empty() {
            rebuilt.push(Span::styled(content_owned, style));
            continue;
        }

        let mut token_start: Option<usize> = None;
        for (idx, ch) in content_owned.char_indices() {
            if !ch.is_whitespace() {
                token_start = Some(idx);
                break;
            }
        }

        let Some(start) = token_start else {
            rebuilt.push(Span::styled(content_owned, style));
            continue;
        };

        let mut end = content_owned.len();
        for (offset, ch) in content_owned[start..].char_indices() {
            if ch.is_whitespace() {
                end = start + offset;
                break;
            }
        }

        let before = &content_owned[..start];
        let token = &content_owned[start..end];
        let after = &content_owned[end..];

        if !before.is_empty() {
            rebuilt.push(Span::styled(before.to_string(), style));
        }

        if token.chars().count() <= 4 {
            rebuilt.push(Span::styled(token.to_string(), style));
        } else {
            let bright_style = style
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD);
            rebuilt.push(Span::styled(token.to_string(), bright_style));
        }

        if !after.is_empty() {
            rebuilt.push(Span::styled(after.to_string(), style));
        }

        emphasized = true;
    }

    if emphasized || !rebuilt.is_empty() {
        line.spans = rebuilt;
    }
}
