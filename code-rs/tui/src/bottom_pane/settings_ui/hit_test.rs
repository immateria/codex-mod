use ratatui::text::Line;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

pub(crate) fn line_has_non_whitespace_at(
    line: &Line<'_>,
    origin_x: u16,
    max_width: u16,
    x: u16,
) -> bool {
    if max_width == 0 {
        return false;
    }
    if x < origin_x || x >= origin_x.saturating_add(max_width) {
        return false;
    }

    let mut cursor_x = origin_x;
    for span in &line.spans {
        let used = cursor_x.saturating_sub(origin_x);
        let remaining = max_width.saturating_sub(used);
        if remaining == 0 {
            break;
        }

        let text = span.content.as_ref();
        if text.is_empty() {
            continue;
        }
        let span_width = UnicodeWidthStr::width(text);
        if span_width == 0 {
            continue;
        }

        let leading_ws = text
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum::<usize>();
        let trailing_ws = text
            .chars()
            .rev()
            .take_while(|ch| ch.is_whitespace())
            .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum::<usize>();

        let leading_ws = leading_ws.min(span_width);
        let trailing_ws = trailing_ws.min(span_width.saturating_sub(leading_ws));
        let trimmed_width = span_width.saturating_sub(leading_ws).saturating_sub(trailing_ws);

        if trimmed_width > 0 {
            let start = cursor_x.saturating_add(leading_ws as u16);
            let end = cursor_x.saturating_add((leading_ws + trimmed_width) as u16);
            if x >= start && x < end {
                return true;
            }
        }

        cursor_x = cursor_x.saturating_add(span_width.min(remaining as usize) as u16);
    }

    false
}

