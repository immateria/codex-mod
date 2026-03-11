use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

pub(crate) fn wrap_spans(spans: Vec<Span<'static>>, max_width: usize) -> Vec<Line<'static>> {
    let max_width = max_width.max(1);
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut width = 0usize;

    for span in spans {
        let span_width = span.content.as_ref().width();
        if !current.is_empty() && width + span_width > max_width {
            lines.push(Line::from(current));
            current = Vec::new();
            width = 0;
        }
        current.push(span);
        width = width.saturating_add(span_width);
    }

    if current.is_empty() {
        current.push(Span::raw(""));
    }
    lines.push(Line::from(current));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_spans_by_span_width() {
        let lines = wrap_spans(
            vec![Span::raw("aaa"), Span::raw("bbb")],
            5,
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].content, "aaa");
        assert_eq!(lines[1].spans[0].content, "bbb");
    }

    #[test]
    fn empty_span_list_produces_single_blank_line() {
        let lines = wrap_spans(Vec::new(), 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 1);
    }
}

