use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::layout::Rect;

use crate::colors;

use super::line_runs::{
    render_selectable_runs,
    selection_id_at as selection_id_at_runs,
    SelectableLineRun,
};
use super::rows::StyledText;

#[derive(Clone, Debug)]
pub(crate) struct SettingsMenuRow<'a, Id> {
    pub(crate) id: Id,
    pub(crate) label: Cow<'a, str>,
    pub(crate) value: Option<StyledText<'a>>,
    pub(crate) detail: Option<StyledText<'a>>,
    pub(crate) selected_hint: Option<Cow<'a, str>>,
    pub(crate) enabled: bool,
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
        }
    }

    #[allow(dead_code)]
    pub(crate) fn disabled(mut self) -> Self {
        self.enabled = false;
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

impl<'a, Id> SettingsMenuRow<'a, Id>
where
    Id: Copy + PartialEq,
{
    pub(crate) fn to_run(&'a self, selected_id: Option<Id>) -> SelectableLineRun<'a, Id> {
        menu_row_run(self, selected_id)
    }

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

        let mut spans = vec![
            Span::styled(if selected { "› " } else { "  " }, arrow_style),
            Span::styled(self.label, label_style),
        ];

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

fn menu_row_run<'a, Id: Copy + PartialEq>(
    row: &'a SettingsMenuRow<'a, Id>,
    selected_id: Option<Id>,
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

    let mut spans = vec![
        Span::styled(if selected { "› " } else { "  " }, arrow_style),
        Span::styled(row.label.as_ref(), label_style),
    ];

    if let Some(value) = &row.value {
        spans.push(Span::raw("  "));
        let mut value_style = value.style;
        if !row.enabled {
            value_style = value_style.fg(colors::dim());
        }
        spans.push(Span::styled(value.text.as_ref(), value_style));
    }

    if let Some(detail) = &row.detail {
        spans.push(Span::raw("  "));
        let mut detail_style = detail.style;
        if !row.enabled {
            detail_style = detail_style.fg(colors::dim());
        }
        spans.push(Span::styled(detail.text.as_ref(), detail_style));
    }

    if selected
        && let Some(hint) = row.selected_hint.as_deref()
    {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(hint, Style::new().fg(colors::text_dim())));
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
    out_rects: &mut Vec<(Id, Rect)>,
) {
    let runs = rows
        .iter()
        .map(|row| row.to_run(selected_id))
        .collect::<Vec<_>>();
    render_selectable_runs(area, buf, scroll_top, &runs, base_style, out_rects);
}

pub(crate) fn selection_id_at<Id: Copy + PartialEq>(
    body: Rect,
    x: u16,
    y: u16,
    scroll_top: usize,
    rows: &[SettingsMenuRow<'_, Id>],
) -> Option<Id> {
    let runs = rows
        .iter()
        .map(|row| row.to_run(None))
        .collect::<Vec<_>>();
    selection_id_at_runs(body, x, y, scroll_top, &runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_skips_disabled_rows() {
        let area = Rect::new(0, 0, 20, 3);
        let mut first = SettingsMenuRow::new(1usize, "First");
        first.enabled = false;
        let rows = vec![first, SettingsMenuRow::new(2usize, "Second")];
        assert_eq!(selection_id_at(area, 1, 0, 0, &rows), None);
        assert_eq!(selection_id_at(area, 1, 1, 0, &rows), Some(2));
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
        render_menu_rows(
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
}
