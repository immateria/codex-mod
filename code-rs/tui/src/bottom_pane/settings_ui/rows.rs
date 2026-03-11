use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::colors;

#[derive(Clone, Debug)]
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

pub(crate) struct KeyValueRow<'a> {
    pub(crate) label: Cow<'a, str>,
    pub(crate) value: Option<StyledText<'a>>,
    pub(crate) detail: Option<StyledText<'a>>,
    pub(crate) selected_hint: Option<Cow<'a, str>>,
}

impl<'a> KeyValueRow<'a> {
    pub(crate) fn new(label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            label: label.into(),
            value: None,
            detail: None,
            selected_hint: None,
        }
    }

    pub(crate) fn with_value(mut self, value: StyledText<'a>) -> Self {
        self.value = Some(value);
        self
    }

    pub(crate) fn with_selected_hint(mut self, hint: impl Into<Cow<'a, str>>) -> Self {
        self.selected_hint = Some(hint.into());
        self
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
        Style::new().bg(colors::selection()).fg(colors::text())
    } else {
        Style::new().bg(colors::background()).fg(colors::text())
    }
}

pub(crate) fn arrow_span(selected: bool) -> Span<'static> {
    Span::styled(
        if selected { "› " } else { "  " },
        Style::new().fg(if selected {
            colors::primary()
        } else {
            colors::text_dim()
        }),
    )
}

pub(crate) fn row_area(body: Rect, rel_idx: usize) -> Rect {
    Rect::new(
        body.x,
        body.y.saturating_add(rel_idx as u16),
        body.width,
        1,
    )
}

fn render_kv_row_parts(
    row_area: Rect,
    buf: &mut Buffer,
    selected: bool,
    row: &KeyValueRow<'_>,
) {
    let mut spans = vec![arrow_span(selected)];
    let label_style = if selected {
        Style::new().fg(colors::text()).bold()
    } else {
        Style::new().fg(colors::text())
    };
    spans.push(Span::styled(row.label.as_ref(), label_style));
    spans.push(Span::styled(": ", label_style));

    if let Some(value) = row.value.as_ref() {
        spans.push(Span::styled(value.text.as_ref(), value.style));
    }
    if let Some(detail) = row.detail.as_ref() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(detail.text.as_ref(), detail.style));
    }
    if selected && let Some(hint) = row.selected_hint.as_deref() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(hint, Style::new().fg(colors::text_dim())));
    }

    Paragraph::new(Line::from(spans))
        .style(row_style(selected))
        .render(row_area, buf);
}

pub(crate) fn render_kv_rows(
    body: Rect,
    buf: &mut Buffer,
    scroll_top: usize,
    selected_idx: Option<usize>,
    rows: &[KeyValueRow<'_>],
) {
    let visible = body.height as usize;
    for rel_idx in 0..visible {
        let abs_idx = scroll_top.saturating_add(rel_idx);
        let area = row_area(body, rel_idx);
        let Some(row) = rows.get(abs_idx) else {
            Paragraph::new(Line::from(""))
                .style(row_style(false))
                .render(area, buf);
            continue;
        };
        render_kv_row_parts(area, buf, selected_idx == Some(abs_idx), row);
    }
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

    #[test]
    fn row_area_tracks_relative_row_position() {
        let body = Rect::new(4, 7, 30, 5);
        assert_eq!(row_area(body, 0), Rect::new(4, 7, 30, 1));
        assert_eq!(row_area(body, 3), Rect::new(4, 10, 30, 1));
    }
}
