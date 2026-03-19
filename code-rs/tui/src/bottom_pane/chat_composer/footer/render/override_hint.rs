use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::WidgetRef;

use super::{span_width, ChatComposer, FOOTER_TRAILING_PAD};

pub(super) fn render_footer_hint_override(
    view: &ChatComposer,
    area: Rect,
    buf: &mut Buffer,
    key_hint_style: Style,
    label_style: Style,
) -> bool {
    let Some(hints) = &view.footer_hint_override else {
        return false;
    };

    let mut left_spans: Vec<Span<'static>> = vec![Span::from("  ")];
    for (idx, (key, label)) in hints.iter().enumerate() {
        if idx > 0 {
            left_spans.push(Span::from("   ").style(label_style));
        }
        if !key.is_empty() {
            left_spans.push(Span::from(key.clone()).style(key_hint_style));
        }
        if !label.is_empty() {
            let prefix = if key.is_empty() {
                String::new()
            } else {
                String::from(" ")
            };
            left_spans.push(Span::from(format!("{prefix}{label}")).style(label_style));
        }
    }

    let token_spans: Vec<Span<'static>> = view.token_usage_spans(label_style);
    let left_len = span_width(&left_spans);
    let right_len = span_width(&token_spans);
    let total_width = area.width as usize;
    let spacer = if total_width > left_len + right_len + FOOTER_TRAILING_PAD {
        " ".repeat(total_width - left_len - right_len - FOOTER_TRAILING_PAD)
    } else {
        String::from(" ")
    };

    let mut line_spans = left_spans;
    line_spans.push(Span::from(spacer));
    line_spans.extend(token_spans);
    line_spans.push(Span::from(" "));

    Line::from(line_spans)
        .style(
            Style::default()
                .fg(crate::colors::text_dim())
                .add_modifier(Modifier::DIM),
        )
        .render_ref(area, buf);
    true
}

