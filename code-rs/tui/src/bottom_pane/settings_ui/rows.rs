use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::colors;

pub(crate) struct StyledText<'a> {
    pub(crate) text: Cow<'a, str>,
    pub(crate) style: Style,
}

impl<'a> StyledText<'a> {
    pub(crate) fn new(text: impl Into<Cow<'a, str>>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

pub(crate) fn selection_index_at(
    body: Rect,
    x: u16,
    y: u16,
    scroll_top: usize,
    total: usize,
) -> Option<usize> {
    if !body.contains(Position { x, y }) {
        return None;
    }
    let rel = y.saturating_sub(body.y) as usize;
    let idx = scroll_top.saturating_add(rel);
    (idx < total).then_some(idx)
}

pub(crate) fn row_style(selected: bool) -> Style {
    if selected {
        Style::default().bg(colors::selection()).fg(colors::text())
    } else {
        Style::default().bg(colors::background()).fg(colors::text())
    }
}

pub(crate) fn arrow_span(selected: bool) -> Span<'static> {
    Span::styled(
        if selected { "› " } else { "  " },
        Style::default().fg(if selected {
            colors::primary()
        } else {
            colors::text_dim()
        }),
    )
}

pub(crate) fn render_kv_row(
    row_area: Rect,
    buf: &mut Buffer,
    selected: bool,
    label: &str,
    value: Option<StyledText<'_>>,
    detail: Option<StyledText<'_>>,
    selected_hint: Option<&str>,
) {
    let mut spans = vec![arrow_span(selected)];
    let label_style = Style::default()
        .fg(colors::text())
        .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() });
    spans.push(Span::styled(label, label_style));
    spans.push(Span::styled(": ", label_style));

    if let Some(value) = value {
        spans.push(Span::styled(value.text, value.style));
    }
    if let Some(detail) = detail {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(detail.text, detail.style));
    }
    if selected && let Some(hint) = selected_hint {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(hint, Style::default().fg(colors::text_dim())));
    }

    Paragraph::new(Line::from(spans))
        .style(row_style(selected))
        .render(row_area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_index_accounts_for_scroll_offset() {
        let body = Rect::new(2, 5, 20, 4);
        assert_eq!(selection_index_at(body, 3, 5, 7, 20), Some(7));
        assert_eq!(selection_index_at(body, 3, 8, 7, 20), Some(10));
        assert_eq!(selection_index_at(body, 30, 8, 7, 20), None);
    }
}
