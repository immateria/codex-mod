use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::colors;
use crate::live_wrap::take_prefix_by_width;

use super::line_runs::{
    render_selectable_runs,
    render_selectable_runs_with_rects,
    SelectableLineRun,
};
use super::hit_test::line_has_non_whitespace_at;
use super::rows::StyledText;

const SPACES: &str = "                                                                ";
const _: () = assert!(SPACES.len() == 64);

#[derive(Clone, Debug)]
pub(crate) struct SettingsMenuRow<'a, Id> {
    pub(crate) id: Id,
    pub(crate) label: Cow<'a, str>,
    pub(crate) value: Option<StyledText<'a>>,
    pub(crate) detail: Option<StyledText<'a>>,
    pub(crate) selected_hint: Option<Cow<'a, str>>,
    pub(crate) enabled: bool,
    pub(crate) indent_cols: u16,
    pub(crate) label_pad_cols: Option<u16>,
}

impl<'a, Id> SettingsMenuRow<'a, Id> {
    pub(crate) fn new(id: Id, label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            id,
            label: label.into(),
            value: None,
            detail: None,
            selected_hint: None,
            enabled: true,
            indent_cols: 0,
            label_pad_cols: None,
        }
    }

    pub(crate) fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub(crate) fn with_indent_cols(mut self, cols: u16) -> Self {
        self.indent_cols = cols;
        self
    }

    pub(crate) fn with_label_pad_cols(mut self, cols: u16) -> Self {
        self.label_pad_cols = Some(cols);
        self
    }

    pub(crate) fn with_value(mut self, value: StyledText<'a>) -> Self {
        self.value = Some(value);
        self
    }

    pub(crate) fn with_detail(mut self, detail: StyledText<'a>) -> Self {
        self.detail = Some(detail);
        self
    }

    pub(crate) fn with_selected_hint(mut self, hint: impl Into<Cow<'a, str>>) -> Self {
        self.selected_hint = Some(hint.into());
        self
    }
}

fn push_spaces<'a>(spans: &mut Vec<Span<'a>>, mut cols: u16) {
    while cols > 0 {
        let chunk = cols.min(SPACES.len() as u16) as usize;
        spans.push(Span::raw(&SPACES[..chunk]));
        cols = cols.saturating_sub(chunk as u16);
    }
}

fn push_spaces_clipped<'a>(
    spans: &mut Vec<Span<'a>>,
    used_cols: &mut usize,
    max_cols: usize,
    cols: u16,
) {
    if *used_cols >= max_cols {
        return;
    }
    let remaining = max_cols.saturating_sub(*used_cols);
    let clipped = usize::from(cols).min(remaining);
    if clipped == 0 {
        return;
    }
    push_spaces(spans, u16::try_from(clipped).unwrap_or(u16::MAX));
    *used_cols = used_cols.saturating_add(clipped);
}

fn push_raw_clipped<'a>(
    spans: &mut Vec<Span<'a>>,
    used_cols: &mut usize,
    max_cols: usize,
    text: &'static str,
) {
    if *used_cols >= max_cols || text.is_empty() {
        return;
    }

    let remaining = max_cols.saturating_sub(*used_cols);
    let span_width = UnicodeWidthStr::width(text);
    if span_width <= remaining {
        spans.push(Span::raw(text));
        *used_cols = used_cols.saturating_add(span_width);
        return;
    }

    let (prefix, _, taken) = take_prefix_by_width(text, remaining);
    if taken > 0 {
        spans.push(Span::raw(prefix));
        *used_cols = used_cols.saturating_add(taken);
    }
}

fn push_text_clipped<'a>(
    spans: &mut Vec<Span<'a>>,
    used_cols: &mut usize,
    max_cols: usize,
    text: &str,
    style: Style,
) -> bool {
    if *used_cols >= max_cols || text.is_empty() {
        return false;
    }

    let remaining = max_cols.saturating_sub(*used_cols);
    if remaining == 0 {
        return false;
    }

    let span_width = UnicodeWidthStr::width(text);
    if span_width <= remaining {
        spans.push(Span::styled(text.to_string(), style));
        *used_cols = used_cols.saturating_add(span_width);
        return false;
    }

    let (prefix, _, taken) = take_prefix_by_width(text, remaining);
    if taken == 0 {
        return true;
    }

    spans.push(Span::styled(prefix, style));
    *used_cols = used_cols.saturating_add(taken);
    true
}

impl<'a, Id> SettingsMenuRow<'a, Id>
where
    Id: Copy + PartialEq,
{
    pub(crate) fn into_run(self, selected_id: Option<Id>) -> SelectableLineRun<'a, Id> {
        let selected = selected_id == Some(self.id) && self.enabled;
        let base = if selected {
            Style::new().bg(colors::selection()).fg(colors::text())
        } else {
            Style::new().bg(colors::background()).fg(colors::text())
        };
        let arrow_style = if selected {
            Style::new().bg(colors::selection()).fg(colors::primary())
        } else {
            Style::new().bg(colors::background()).fg(colors::text_dim())
        };
        let mut label_style = if self.enabled {
            Style::new().fg(colors::text())
        } else {
            Style::new().fg(colors::dim())
        };
        if selected {
            label_style = label_style.bold();
        }

        let label_cols = self
            .label_pad_cols
            .map(|_| u16::try_from(self.label.as_ref().width()).unwrap_or(u16::MAX));

        let mut spans = vec![
            Span::styled(if selected { "› " } else { "  " }, arrow_style),
        ];
        if self.indent_cols > 0 {
            push_spaces(&mut spans, self.indent_cols);
        }
        spans.push(Span::styled(self.label, label_style));
        if let Some(pad_cols) = self.label_pad_cols
            && let Some(label_cols) = label_cols
            && label_cols < pad_cols
        {
            push_spaces(&mut spans, pad_cols.saturating_sub(label_cols));
        }

        if let Some(value) = self.value {
            spans.push(Span::raw("  "));
            let mut value_style = value.style;
            if !self.enabled {
                value_style = value_style.fg(colors::dim());
            }
            spans.push(Span::styled(value.text, value_style));
        }

        if let Some(detail) = self.detail {
            spans.push(Span::raw("  "));
            let mut detail_style = detail.style;
            if !self.enabled {
                detail_style = detail_style.fg(colors::dim());
            }
            spans.push(Span::styled(detail.text, detail_style));
        }

        if selected
            && let Some(hint) = self.selected_hint
        {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(hint, Style::new().fg(colors::text_dim())));
        }

        let run = if self.enabled {
            SelectableLineRun::selectable(self.id, vec![Line::from(spans)])
        } else {
            SelectableLineRun::plain(vec![Line::from(spans)])
        };
        run.with_style(base)
    }
}

fn menu_row_run_with_width<'a, Id: Copy + PartialEq>(
    row: &'a SettingsMenuRow<'a, Id>,
    selected_id: Option<Id>,
    max_width: u16,
    include_detail: bool,
) -> SelectableLineRun<'a, Id> {
    let selected = selected_id == Some(row.id) && row.enabled;
    let base = if selected {
        Style::new().bg(colors::selection()).fg(colors::text())
    } else {
        Style::new().bg(colors::background()).fg(colors::text())
    };
    let arrow_style = if selected {
        Style::new().bg(colors::selection()).fg(colors::primary())
    } else {
        Style::new().bg(colors::background()).fg(colors::text_dim())
    };
    let mut label_style = if row.enabled {
        Style::new().fg(colors::text())
    } else {
        Style::new().fg(colors::dim())
    };
    if selected {
        label_style = label_style.bold();
    }

    let max_cols = usize::from(max_width);
    let mut used_cols = 0usize;
    let mut spans = Vec::new();

    let _ = push_text_clipped(
        &mut spans,
        &mut used_cols,
        max_cols,
        if selected { "› " } else { "  " },
        arrow_style,
    );
    if row.indent_cols > 0 {
        push_spaces_clipped(&mut spans, &mut used_cols, max_cols, row.indent_cols);
    }
    let _ = push_text_clipped(
        &mut spans,
        &mut used_cols,
        max_cols,
        row.label.as_ref(),
        label_style,
    );
    if let Some(pad_cols) = row.label_pad_cols {
        let label_cols = u16::try_from(row.label.as_ref().width()).unwrap_or(u16::MAX);
        if label_cols < pad_cols {
            push_spaces_clipped(
                &mut spans,
                &mut used_cols,
                max_cols,
                pad_cols.saturating_sub(label_cols),
            );
        }
    }

    let mut truncated = false;

    if let Some(value) = &row.value {
        push_raw_clipped(&mut spans, &mut used_cols, max_cols, "  ");
        let mut value_style = value.style;
        if !row.enabled {
            value_style = value_style.fg(colors::dim());
        }
        truncated = push_text_clipped(
            &mut spans,
            &mut used_cols,
            max_cols,
            value.text.as_ref(),
            value_style,
        );
    }

    if !truncated
        && include_detail
        && let Some(detail) = &row.detail
    {
        push_raw_clipped(&mut spans, &mut used_cols, max_cols, "  ");
        let mut detail_style = detail.style;
        if !row.enabled {
            detail_style = detail_style.fg(colors::dim());
        }
        truncated = push_text_clipped(
            &mut spans,
            &mut used_cols,
            max_cols,
            detail.text.as_ref(),
            detail_style,
        );
    }

    if !truncated
        && selected
        && let Some(hint) = row.selected_hint.as_deref()
    {
        push_raw_clipped(&mut spans, &mut used_cols, max_cols, "  ");
        let _ = push_text_clipped(
            &mut spans,
            &mut used_cols,
            max_cols,
            hint,
            Style::new().fg(colors::text_dim()),
        );
    }

    let run = if row.enabled {
        SelectableLineRun::selectable(row.id, vec![Line::from(spans)])
    } else {
        SelectableLineRun::plain(vec![Line::from(spans)])
    };
    run.with_style(base)
}

pub(crate) fn render_menu_rows<Id: Copy + PartialEq>(
    area: Rect,
    buf: &mut Buffer,
    scroll_top: usize,
    selected_id: Option<Id>,
    rows: &[SettingsMenuRow<'_, Id>],
    base_style: Style,
) {
    let max_width = area.width;
    let runs = rows
        .iter()
        .map(|row| menu_row_run_with_width(row, selected_id, max_width, true))
        .collect::<Vec<_>>();
    render_selectable_runs(area, buf, scroll_top, &runs, base_style);
}

pub(crate) fn render_menu_rows_compact<Id: Copy + PartialEq>(
    area: Rect,
    buf: &mut Buffer,
    scroll_top: usize,
    selected_id: Option<Id>,
    rows: &[SettingsMenuRow<'_, Id>],
    base_style: Style,
) {
    let max_width = area.width;
    let runs = rows
        .iter()
        .map(|row| menu_row_run_with_width(row, selected_id, max_width, false))
        .collect::<Vec<_>>();
    render_selectable_runs(area, buf, scroll_top, &runs, base_style);
}

#[allow(dead_code)]
pub(crate) fn render_menu_rows_with_rects<Id: Copy + PartialEq>(
    area: Rect,
    buf: &mut Buffer,
    scroll_top: usize,
    selected_id: Option<Id>,
    rows: &[SettingsMenuRow<'_, Id>],
    base_style: Style,
    out_rects: &mut Vec<(Id, Rect)>,
) {
    let max_width = area.width;
    let runs = rows
        .iter()
        .map(|row| menu_row_run_with_width(row, selected_id, max_width, true))
        .collect::<Vec<_>>();
    render_selectable_runs_with_rects(area, buf, scroll_top, &runs, base_style, out_rects);
}

pub(crate) fn selection_id_at<Id: Copy + PartialEq>(
    body: Rect,
    x: u16,
    y: u16,
    scroll_top: usize,
    rows: &[SettingsMenuRow<'_, Id>],
) -> Option<Id> {
    if !body.contains(Position { x, y }) {
        return None;
    }

    let rel = y.saturating_sub(body.y) as usize;
    let idx = scroll_top.saturating_add(rel);
    let row = rows.get(idx)?;
    if !row.enabled {
        return None;
    }

    // Only consider the row hit when the pointer is over the visible text content,
    // not just anywhere in the row's padded background.
    let max_width = body.width;
    if max_width == 0 {
        return None;
    }

    let run = menu_row_run_with_width(row, None, max_width, true);
    let line = run.lines.first()?;
    if !line_has_non_whitespace_at(line, body.x, max_width, x) {
        return None;
    }
    Some(row.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn selection_skips_disabled_rows() {
        let area = Rect::new(0, 0, 20, 3);
        let mut first = SettingsMenuRow::new(1usize, "First");
        first.enabled = false;
        let rows = vec![first, SettingsMenuRow::new(2usize, "Second")];
        assert_eq!(selection_id_at(area, 1, 0, 0, &rows), None);
        assert_eq!(selection_id_at(area, 2, 1, 0, &rows), Some(2));
        assert_eq!(selection_id_at(area, 2, 0, 1, &rows), Some(2));
    }

    #[test]
    fn menu_row_indent_inserts_spaces_after_arrow() {
        let run = SettingsMenuRow::new(1usize, "Child")
            .with_indent_cols(2)
            .into_run(Some(1));
        assert_eq!(line_text(&run.lines[0]), "›   Child");
    }

    #[test]
    fn menu_row_label_padding_aligns_value_column() {
        let run = SettingsMenuRow::new(1usize, "A")
            .with_label_pad_cols(4)
            .with_value(StyledText::new("v", Style::new()))
            .into_run(Some(1));
        assert_eq!(line_text(&run.lines[0]), "› A     v");
    }

    #[test]
    fn menu_row_detail_is_clipped_when_truncated() {
        let row = SettingsMenuRow::new(1usize, "Feature")
            .with_value(StyledText::new("off", Style::new()))
            .with_detail(StyledText::new(
                "This is a very long description that should truncate",
                Style::new(),
            ));
        let run = menu_row_run_with_width(&row, Some(1), 20, true);
        let text = line_text(&run.lines[0]);
        assert!(UnicodeWidthStr::width(text.as_str()) <= 20);
    }

    #[test]
    fn menu_row_value_is_clipped_when_truncated() {
        let row = SettingsMenuRow::new(1usize, "Path").with_value(StyledText::new(
            "some/really/long/value/that/should/truncate",
            Style::new(),
        ));
        let run = menu_row_run_with_width(&row, Some(1), 16, true);
        let text = line_text(&run.lines[0]);
        assert!(UnicodeWidthStr::width(text.as_str()) <= 16);
    }

    #[test]
    fn render_menu_rows_returns_visible_rects() {
        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::empty(area);
        let rows = vec![
            SettingsMenuRow::new(1usize, "First"),
            SettingsMenuRow::new(2usize, "Second"),
        ];
        let mut rects = Vec::new();
        render_menu_rows_with_rects(
            area,
            &mut buf,
            1,
            Some(2),
            &rows,
            Style::new(),
            &mut rects,
        );
        assert_eq!(rects, vec![(2, Rect::new(0, 0, 20, 1))]);
    }

    #[test]
    fn selection_requires_pointer_over_visible_text() {
        let area = Rect::new(0, 0, 20, 1);
        let rows = vec![SettingsMenuRow::new(1usize, "A")];
        // The arrow/indicator column is whitespace for unselected rows; the label begins at x=2.
        assert_eq!(selection_id_at(area, 0, 0, 0, &rows), None);
        assert_eq!(selection_id_at(area, 2, 0, 0, &rows), Some(1));
        assert_eq!(selection_id_at(area, 19, 0, 0, &rows), None);
    }
}
