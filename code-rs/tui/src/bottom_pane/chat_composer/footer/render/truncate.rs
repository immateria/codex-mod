use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

use super::{span_width, FOOTER_TRAILING_PAD};

pub(super) fn truncate_left_if_needed(
    total_width: usize,
    right_len: usize,
    mut left_spans: Vec<Span<'static>>,
    mut left_len: usize,
) -> (Vec<Span<'static>>, usize) {
    if left_len + right_len + FOOTER_TRAILING_PAD > total_width {
        let mut remaining = total_width.saturating_sub(right_len + FOOTER_TRAILING_PAD);
        if remaining == 0 {
            left_spans.clear();
        } else {
            let mut truncated: Vec<Span> = Vec::new();
            for span in &left_spans {
                if remaining == 0 {
                    break;
                }
                let span_len = UnicodeWidthStr::width(span.content.as_ref());
                if span_len <= remaining {
                    truncated.push(span.clone());
                    remaining -= span_len;
                    continue;
                }

                if span.content.trim().is_empty() {
                    truncated.push(Span::from(" ".repeat(remaining)).style(span.style));
                    remaining = 0;
                } else if remaining <= 1 {
                    truncated.push(Span::from("…").style(span.style));
                    remaining = 0;
                } else {
                    let collected = crate::text_formatting::truncate_to_display_width_with_suffix(
                        span.content.as_ref(),
                        remaining,
                        "…",
                    );
                    truncated.push(Span::from(collected).style(span.style));
                    remaining = 0;
                }
            }
            if truncated.is_empty() {
                truncated.push(Span::from("  "));
            }
            left_spans = truncated;
        }
        left_len = span_width(&left_spans);
    }

    (left_spans, left_len)
}

