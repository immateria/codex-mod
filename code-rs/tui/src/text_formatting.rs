use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Display width of a string in terminal columns.
pub(crate) fn string_display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Truncate a tool result to fit within the given height and width. If the text is valid JSON, we format it in a
/// compact way before truncating. This is a best-effort approach that may not work perfectly for text where one
/// grapheme spans multiple terminal cells.
/// Format JSON text in a compact single-line format with spaces to improve Ratatui wrapping. Returns `None` if the
/// input is not valid JSON.
pub(crate) fn format_json_compact(text: &str) -> Option<String> {
    let json = serde_json::from_str::<serde_json::Value>(text).ok()?;
    Some(compact_json_string(
        &serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string()),
    ))
}

/// Like [`format_json_compact`] but takes a pre-parsed [`serde_json::Value`],
/// avoiding the redundant parse→serialize→parse round-trip when the caller
/// already holds a `Value`.
pub(crate) fn format_json_value_compact(value: &serde_json::Value) -> String {
    compact_json_string(
        &serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
    )
}

/// Compact a pretty-printed JSON string into a single line with minimal
/// whitespace: only one space after `:` and `,` (but not before `}` or `]`).
fn compact_json_string(json_pretty: &str) -> String {
    let mut result = String::new();
    let mut chars = json_pretty.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if !escape_next => {
                in_string = !in_string;
                result.push(ch);
            }
            '\\' if in_string => {
                escape_next = !escape_next;
                result.push(ch);
            }
            '\n' | '\r' if !in_string => {
                // Skip newlines when not in a string literal.
            }
            ' ' | '\t' if !in_string => {
                if let Some(&next_ch) = chars.peek()
                    && let Some(last_ch) = result.chars().last()
                        && (last_ch == ':' || last_ch == ',') && !matches!(next_ch, '}' | ']') {
                            result.push(' ');
                        }
            }
            _ => {
                if escape_next && in_string {
                    escape_next = false;
                }
                result.push(ch);
            }
        }
    }

    result
}

/// Truncate `text` to at most `max_graphemes` graphemes, avoiding partial graphemes and adding an ellipsis when there
/// is enough space.
/// Truncate by character count and append a single ellipsis when truncated.
///
/// Guarantees returned character count is at most `max_chars` unless
/// `max_chars == 0`, in which case an empty string is returned.
pub(crate) fn truncate_chars_with_ellipsis(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    if max_chars == 1 {
        return "…".to_string();
    }

    let keep = max_chars.saturating_sub(1);
    let mut out = String::with_capacity(max_chars);
    out.extend(text.chars().take(keep));
    out.push('…');
    out
}

/// Truncate by character count and append an ellipsis *after* the kept text.
///
/// This differs from [`truncate_chars_with_ellipsis`] by keeping up to
/// `max_chars` characters before appending `…`, so the result can be one
/// character longer than `max_chars`.
pub(crate) fn truncate_chars_append_ellipsis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

/// Truncate to terminal display width (Unicode-aware), without padding.
pub(crate) fn truncate_to_display_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }

    let mut out = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if width + ch_width > max_width {
            break;
        }
        out.push(ch);
        width += ch_width;
        if width == max_width {
            break;
        }
    }
    out
}

/// Truncate to display width, appending `suffix` when truncation occurs.
///
/// If `max_width` is too small to include `suffix`, this falls back to plain
/// width truncation without suffix.
pub(crate) fn truncate_to_display_width_with_suffix(
    text: &str,
    max_width: usize,
    suffix: &str,
) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }

    let suffix_width = UnicodeWidthStr::width(suffix);
    if suffix_width == 0 || max_width <= suffix_width {
        return truncate_to_display_width(text, max_width);
    }

    let mut out = truncate_to_display_width(text, max_width.saturating_sub(suffix_width));
    out.push_str(suffix);
    out
}

/// Pad a string with spaces so its display width is exactly `width`.
///
/// If the input is wider than `width`, it is truncated first.
pub(crate) fn pad_to_display_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let truncated = if UnicodeWidthStr::width(text) > width {
        truncate_to_display_width(text, width)
    } else {
        text.to_string()
    };
    let current = UnicodeWidthStr::width(truncated.as_str());
    if current >= width {
        return truncated;
    }

    let mut out = truncated;
    out.extend(std::iter::repeat_n(' ', width - current));
    out
}

/// Truncate to a UTF-8 byte budget while preserving valid char boundaries.
///
/// Appends a Unicode ellipsis when truncation occurs and there is enough room.
pub(crate) fn truncate_utf8_bytes_with_ellipsis(text: &str, max_bytes: usize) -> String {
    const ELLIPSIS: &str = "…";
    let ellipsis_bytes = ELLIPSIS.len();

    if max_bytes == 0 {
        return String::new();
    }
    if text.len() <= max_bytes {
        return text.to_string();
    }

    let slice_limit = max_bytes.saturating_sub(ellipsis_bytes);
    let safe_boundary = text
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(text.len()))
        .take_while(|idx| *idx <= slice_limit)
        .last()
        .unwrap_or(0);

    let safe_slice = text.get(..safe_boundary).unwrap_or("");
    if max_bytes < ellipsis_bytes {
        safe_slice.to_string()
    } else {
        format!("{safe_slice}{ELLIPSIS}")
    }
}

/// Format a model identifier for display (e.g. "gpt-4o-mini" → "GPT-4o-Mini").
///
/// Strips the internal "code-" prefix from agent models so user-facing labels
/// display the canonical model name.
pub(crate) fn format_model_label(model: &str) -> String {
    let model = if model.to_ascii_lowercase().starts_with("code-") {
        &model[5..]
    } else {
        model
    };

    let mut parts = Vec::new();
    for (idx, part) in model.split('-').enumerate() {
        if idx == 0 {
            parts.push(part.to_ascii_uppercase());
            continue;
        }
        let mut chars = part.chars();
        let formatted = match chars.next() {
            Some(first) if first.is_ascii_alphabetic() => {
                let mut s = String::new();
                s.push(first.to_ascii_uppercase());
                s.push_str(chars.as_str());
                s
            }
            Some(first) => {
                let mut s = String::new();
                s.push(first);
                s.push_str(chars.as_str());
                s
            }
            None => String::new(),
        };
        parts.push(formatted);
    }
    parts.join("-")
}

/// Format a list of paths as newline-separated strings.
pub(crate) fn format_path_list(paths: &[std::path::PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Human-readable label for a reasoning effort level.
pub(crate) fn reasoning_effort_label(effort: code_core::config_types::ReasoningEffort) -> &'static str {
    use code_core::config_types::ReasoningEffort;
    match effort {
        ReasoningEffort::XHigh => "XHigh",
        ReasoningEffort::High => "High",
        ReasoningEffort::Medium => "Medium",
        ReasoningEffort::Low => "Low",
        ReasoningEffort::Minimal => "Minimal",
        ReasoningEffort::None => "None",
    }
}

/// Parse newline-separated text into a list of paths, trimming and filtering empties.
pub(crate) fn parse_path_list(text: &str) -> Vec<std::path::PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(std::path::PathBuf::from)
        .collect()
}

/// Parse newline-separated text into a list of strings, trimming and filtering empties.
pub(crate) fn parse_string_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// Split a single long word into chunks that each fit within `width` display
/// columns. Uses pre-allocated buffers for efficiency.
pub(crate) fn split_long_word(word: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut parts = Vec::new();
    let mut current = String::with_capacity(width * 4);
    let mut current_width = 0;

    for ch in word.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if current_width + ch_width > width && !current.is_empty() {
            parts.push(std::mem::replace(&mut current, String::with_capacity(width * 4)));
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if !current.is_empty() {
        parts.push(current);
    }

    if parts.is_empty() {
        parts.push(String::new());
    }
    parts
}

/// Word-wrap `text` to fit within `width` display columns. Long words that
/// exceed `width` are split character-by-character.
pub(crate) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);
        let parts: Vec<String> = if word_width > width {
            split_long_word(word, width)
        } else {
            vec![word.to_string()]
        };

        for part in parts {
            let part_width = UnicodeWidthStr::width(part.as_str());
            if current.is_empty() {
                current.push_str(&part);
                current_width = part_width;
            } else if current_width + 1 + part_width > width {
                lines.push(std::mem::replace(&mut current, part));
                current_width = part_width;
            } else {
                current.push(' ');
                current.push_str(&part);
                current_width += 1 + part_width;
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Word-wrap `text` into lines that fit within `body_width` minus the given
/// `indent_cols` and `right_padding`.  Used by card renderers (image, browser)
/// to wrap body text inside bordered cards.
pub(crate) fn wrap_card_lines(
    text: &str,
    body_width: usize,
    indent_cols: usize,
    right_padding: usize,
) -> Vec<String> {
    let available = body_width
        .saturating_sub(indent_cols)
        .saturating_sub(right_padding);
    if available == 0 {
        return vec![String::new()];
    }
    wrap_text(text, available)
}
