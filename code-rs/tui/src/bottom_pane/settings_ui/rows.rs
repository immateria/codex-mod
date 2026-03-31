use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::colors;

use super::hit_test::line_has_non_whitespace_at;

const SPACES: &str = "                                                                ";
const _: () = assert!(SPACES.len() == 64);

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
    pub(crate) label_pad_cols: Option<u16>,
}

impl<'a> KeyValueRow<'a> {
    pub(crate) fn new(label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            label: label.into(),
            value: None,
            detail: None,
            selected_hint: None,
            label_pad_cols: None,
        }
    }

    pub(crate) fn with_value(mut self, value: StyledText<'a>) -> Self {
        self.value = Some(value);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn with_detail(mut self, detail: StyledText<'a>) -> Self {
        self.detail = Some(detail);
        self
    }

    pub(crate) fn with_selected_hint(mut self, hint: impl Into<Cow<'a, str>>) -> Self {
        self.selected_hint = Some(hint.into());
        self
    }

    #[allow(dead_code)]
    pub(crate) fn with_label_pad_cols(mut self, cols: u16) -> Self {
        self.label_pad_cols = Some(cols);
        self
    }
}

pub(crate) fn selection_index_at_over_text(
    body: Rect,
    x: u16,
    y: u16,
    scroll_top: usize,
    rows: &[KeyValueRow<'_>],
) -> Option<usize> {
    if !body.contains(Position { x, y }) {
        return None;
    }
    let rel = y.saturating_sub(body.y) as usize;
    let idx = scroll_top.saturating_add(rel);
    let row = rows.get(idx)?;

    let default_pad_cols = default_label_pad_cols(rows);
    let line = kv_row_line(false, row, default_pad_cols);
    if !line_has_non_whitespace_at(&line, body.x, body.width, x) {
        return None;
    }
    Some(idx)
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
        body.y.saturating_add(clamp_u16(rel_idx)),
        body.width,
        1,
    )
}

fn clamp_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn push_spaces<'a>(spans: &mut Vec<Span<'a>>, mut cols: u16) {
    while cols > 0 {
        let chunk = cols.min(SPACES.len() as u16) as usize;
        spans.push(Span::raw(&SPACES[..chunk]));
        cols = cols.saturating_sub(chunk as u16);
    }
}

fn label_width(row: &KeyValueRow<'_>) -> u16 {
    clamp_u16(row.label.as_ref().width())
}

fn default_label_pad_cols(rows: &[KeyValueRow<'_>]) -> u16 {
    rows.iter()
        .filter(|row| row.value.is_some() || row.detail.is_some())
        .map(label_width)
        .max()
        .unwrap_or(0)
}

fn kv_row_line<'a, 'r>(
    selected: bool,
    row: &'r KeyValueRow<'a>,
    default_pad_cols: u16,
) -> Line<'r> {
    let mut spans = vec![arrow_span(selected)];
    let label_style = if selected {
        Style::new().fg(colors::text()).bold()
    } else {
        Style::new().fg(colors::text())
    };
    spans.push(Span::styled(row.label.as_ref(), label_style));

    if row.value.is_some() || row.detail.is_some() {
        let pad_cols = row.label_pad_cols.unwrap_or(default_pad_cols);
        let label_cols = label_width(row);
        if label_cols < pad_cols {
            push_spaces(&mut spans, pad_cols.saturating_sub(label_cols));
        }
        spans.push(Span::styled(": ", label_style));

        if let Some(value) = row.value.as_ref() {
            spans.push(Span::styled(value.text.as_ref(), value.style));
        }
        if let Some(detail) = row.detail.as_ref() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(detail.text.as_ref(), detail.style));
        }
    }
    if selected
        && let Some(hint) = row.selected_hint.as_deref()
    {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(hint, Style::new().fg(colors::text_dim())));
    }

    Line::from(spans)
}

fn render_kv_row_parts(
    row_area: Rect,
    buf: &mut Buffer,
    selected: bool,
    row: &KeyValueRow<'_>,
    default_pad_cols: u16,
) {
    Paragraph::new(kv_row_line(selected, row, default_pad_cols))
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
    let default_pad_cols = default_label_pad_cols(rows);
    for rel_idx in 0..visible {
        let abs_idx = scroll_top.saturating_add(rel_idx);
        let area = row_area(body, rel_idx);
        let Some(row) = rows.get(abs_idx) else {
            Paragraph::new(Line::from(""))
                .style(row_style(false))
                .render(area, buf);
            continue;
        };
        render_kv_row_parts(area, buf, selected_idx == Some(abs_idx), row, default_pad_cols);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn selection_index_accounts_for_scroll_offset() {
        let body = Rect::new(2, 5, 20, 4);
        let rows = (0..20).map(|idx| KeyValueRow::new(format!("Row {idx}"))).collect::<Vec<_>>();
        assert_eq!(selection_index_at_over_text(body, 4, 5, 7, &rows), Some(7));
        assert_eq!(selection_index_at_over_text(body, 4, 8, 7, &rows), Some(10));
        assert_eq!(selection_index_at_over_text(body, 30, 8, 7, &rows), None);
    }

    #[test]
    fn row_area_tracks_relative_row_position() {
        let body = Rect::new(4, 7, 30, 5);
        assert_eq!(row_area(body, 0), Rect::new(4, 7, 30, 1));
        assert_eq!(row_area(body, 3), Rect::new(4, 10, 30, 1));
    }

    #[test]
    fn key_value_row_supports_detail_builder() {
        let row = KeyValueRow::new("Label").with_detail(StyledText::new("detail", Style::new()));
        assert_eq!(
            row.detail.expect("detail").text.as_ref(),
            "detail"
        );
    }

    #[test]
    fn kv_row_uses_page_level_label_padding() {
        let rows = [
            KeyValueRow::new("A").with_value(StyledText::new("one", Style::new())),
            KeyValueRow::new("Long label").with_value(StyledText::new("two", Style::new())),
        ];
        let default_pad_cols = default_label_pad_cols(&rows);
        let first = kv_row_line(false, &rows[0], default_pad_cols);

        assert_eq!(line_text(&first), "  A         : one");
    }

    #[test]
    fn kv_row_explicit_label_padding_overrides_page_default() {
        let rows = [
            KeyValueRow::new("A")
                .with_label_pad_cols(4)
                .with_value(StyledText::new("one", Style::new())),
            KeyValueRow::new("Long label").with_value(StyledText::new("two", Style::new())),
        ];
        let default_pad_cols = default_label_pad_cols(&rows);
        let first = kv_row_line(false, &rows[0], default_pad_cols);

        assert_eq!(line_text(&first), "  A   : one");
    }

    #[test]
    fn kv_row_padding_uses_unicode_display_width() {
        let rows = [
            KeyValueRow::new("Ａ").with_value(StyledText::new("one", Style::new())),
            KeyValueRow::new("Wide").with_value(StyledText::new("two", Style::new())),
        ];
        let default_pad_cols = default_label_pad_cols(&rows);
        let first = kv_row_line(false, &rows[0], default_pad_cols);

        assert_eq!(line_text(&first), "  Ａ  : one");
    }

    #[test]
    fn page_level_padding_ignores_rows_without_values() {
        let rows = [
            KeyValueRow::new("Enabled").with_value(StyledText::new("on", Style::new())),
            KeyValueRow::new("Very long action row with no value"),
        ];

        assert_eq!(default_label_pad_cols(&rows), 7);
    }

    #[test]
    fn kv_row_omits_colon_when_no_value_or_detail() {
        let rows = [KeyValueRow::new("Close")];
        let default_pad_cols = default_label_pad_cols(&rows);
        let first = kv_row_line(false, &rows[0], default_pad_cols);

        assert_eq!(line_text(&first), "  Close");
    }
}
