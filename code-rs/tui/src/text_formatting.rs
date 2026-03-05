use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Truncate a tool result to fit within the given height and width. If the text is valid JSON, we format it in a
/// compact way before truncating. This is a best-effort approach that may not work perfectly for text where one
/// grapheme spans multiple terminal cells.
#[allow(dead_code)]
pub(crate) fn format_and_truncate_tool_result(
    text: &str,
    max_lines: usize,
    line_width: usize,
) -> String {
    // Work out the maximum number of graphemes we can display for a result. It's not guaranteed that one grapheme
    // equals one cell, so we subtract 1 per line as a conservative buffer.
    let max_graphemes = (max_lines * line_width).saturating_sub(max_lines);

    if let Some(formatted_json) = format_json_compact(text) {
        truncate_text(&formatted_json, max_graphemes)
    } else {
        truncate_text(text, max_graphemes)
    }
}

/// Format JSON text in a compact single-line format with spaces to improve Ratatui wrapping. Returns `None` if the
/// input is not valid JSON.
pub(crate) fn format_json_compact(text: &str) -> Option<String> {
    let json = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let json_pretty = serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string());

    // Convert multi-line pretty JSON to compact single-line format by removing newlines and redundant whitespace.
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

    Some(result)
}

/// Truncate `text` to at most `max_graphemes` graphemes, avoiding partial graphemes and adding an ellipsis when there
/// is enough space.
#[allow(dead_code)]
pub(crate) fn truncate_text(text: &str, max_graphemes: usize) -> String {
    let mut graphemes = text.grapheme_indices(true);

    if let Some((byte_index, _)) = graphemes.nth(max_graphemes) {
        if max_graphemes >= 3 {
            let mut truncate_graphemes = text.grapheme_indices(true);
            if let Some((truncate_byte_index, _)) = truncate_graphemes.nth(max_graphemes - 3) {
                let truncated = &text[..truncate_byte_index];
                return format!("{truncated}...");
            }
        }
        text[..byte_index].to_string()
    } else {
        text.to_string()
    }
}

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
    out.push_str(&" ".repeat(width - current));
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
