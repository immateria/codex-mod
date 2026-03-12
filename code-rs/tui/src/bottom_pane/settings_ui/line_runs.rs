use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;

use crate::ui_interaction::scroll_top_to_keep_visible;
use crate::util::buffer::{fill_rect, write_line};

#[derive(Clone, Debug)]
pub(crate) struct SelectableLineRun<'a, Id> {
    pub(crate) id: Option<Id>,
    pub(crate) lines: Vec<Line<'a>>,
    pub(crate) style: Style,
}

impl<'a, Id> SelectableLineRun<'a, Id> {
    pub(crate) fn plain(lines: Vec<Line<'a>>) -> Self {
        Self {
            id: None,
            lines,
            style: Style::new(),
        }
    }

    pub(crate) fn selectable(id: Id, lines: Vec<Line<'a>>) -> Self {
        Self {
            id: Some(id),
            lines,
            style: Style::new(),
        }
    }

    pub(crate) fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

pub(crate) fn render_selectable_runs<Id: Copy>(
    area: Rect,
    buf: &mut Buffer,
    scroll_top: usize,
    runs: &[SelectableLineRun<'_, Id>],
    base_style: Style,
) {
    render_selectable_runs_inner(area, buf, scroll_top, runs, base_style, None);
}

#[allow(dead_code)]
pub(crate) fn render_selectable_runs_with_rects<Id: Copy>(
    area: Rect,
    buf: &mut Buffer,
    scroll_top: usize,
    runs: &[SelectableLineRun<'_, Id>],
    base_style: Style,
    out_rects: &mut Vec<(Id, Rect)>,
) {
    out_rects.clear();
    render_selectable_runs_inner(
        area,
        buf,
        scroll_top,
        runs,
        base_style,
        Some(out_rects),
    );
}

fn render_selectable_runs_inner<Id: Copy>(
    area: Rect,
    buf: &mut Buffer,
    scroll_top: usize,
    runs: &[SelectableLineRun<'_, Id>],
    base_style: Style,
    mut out_rects: Option<&mut Vec<(Id, Rect)>>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    fill_rect(buf, area, Some(' '), base_style);

    let visible_start = scroll_top;
    let visible_end = scroll_top.saturating_add(area.height as usize);
    let mut current_line = 0usize;

    for run in runs {
        let run_start = current_line;
        let run_end = run_start.saturating_add(run.lines.len());
        current_line = run_end;

        if run_end <= visible_start || run_start >= visible_end {
            continue;
        }

        let first_visible = visible_start.max(run_start);
        let last_visible = visible_end.min(run_end);
        let run_style = base_style.patch(run.style);

        for visible_line in first_visible..last_visible {
            let source_idx = visible_line.saturating_sub(run_start);
            let y = area
                .y
                .saturating_add(visible_line.saturating_sub(visible_start) as u16);
            write_line(buf, area.x, y, area.width, &run.lines[source_idx], run_style);
        }

        if let Some(id) = run.id
            && let Some(out_rects) = out_rects.as_deref_mut()
        {
            out_rects.push((
                id,
                Rect::new(
                    area.x,
                    area.y.saturating_add(first_visible.saturating_sub(visible_start) as u16),
                    area.width,
                    last_visible.saturating_sub(first_visible) as u16,
                ),
            ));
        }
    }
}

pub(crate) fn selection_id_at<Id: Copy>(
    area: Rect,
    x: u16,
    y: u16,
    scroll_top: usize,
    runs: &[SelectableLineRun<'_, Id>],
) -> Option<Id> {
    if !area.contains(Position { x, y }) {
        return None;
    }

    let visible_start = scroll_top;
    let visible_end = scroll_top.saturating_add(area.height as usize);
    let mut current_line = 0usize;

    for run in runs {
        let run_start = current_line;
        let run_end = run_start.saturating_add(run.lines.len());
        current_line = run_end;

        if run_end <= visible_start || run_start >= visible_end {
            continue;
        }

        let first_visible = visible_start.max(run_start);
        let last_visible = visible_end.min(run_end);
        let rect = Rect::new(
            area.x,
            area.y
                .saturating_add(first_visible.saturating_sub(visible_start) as u16),
            area.width,
            last_visible.saturating_sub(first_visible) as u16,
        );

        if let Some(id) = run.id
            && rect.contains(Position { x, y })
        {
            return Some(id);
        }
    }

    None
}

pub(crate) fn scroll_top_for_section(
    current_scroll_top: usize,
    body_height: usize,
    selected_line: usize,
    section_start: usize,
    section_end: usize,
) -> usize {
    if body_height == 0 {
        return current_scroll_top;
    }

    let section_end = section_end.max(section_start);
    let selected_line = selected_line.clamp(section_start, section_end);
    let section_len = section_end.saturating_sub(section_start).saturating_add(1);

    // If the full section fits, pin it to the top so the section header is visible.
    if section_len <= body_height {
        return section_start;
    }

    // If we can show the section header while keeping the selection visible, do so.
    if selected_line <= section_start.saturating_add(body_height.saturating_sub(1)) {
        return section_start;
    }

    let max_scroll_top = section_end.saturating_add(1).saturating_sub(body_height);
    let current_scroll_top = current_scroll_top.clamp(section_start, max_scroll_top);
    let next = scroll_top_to_keep_visible(
        current_scroll_top,
        max_scroll_top,
        body_height,
        selected_line,
        1,
    );
    next.clamp(section_start, max_scroll_top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_selectable_runs_clips_and_returns_visible_rects() {
        let area = Rect::new(0, 0, 20, 2);
        let mut buf = Buffer::empty(area);
        let mut rects = Vec::new();
        let runs = vec![
            SelectableLineRun::plain(vec![Line::from("header")]),
            SelectableLineRun::selectable(7usize, vec![Line::from("row a"), Line::from("row b")]),
            SelectableLineRun::selectable(8usize, vec![Line::from("row c")]),
        ];

        render_selectable_runs_with_rects(area, &mut buf, 1, &runs, Style::new(), &mut rects);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0], (7, Rect::new(0, 0, 20, 2)));
    }

    #[test]
    fn render_clips_run_that_starts_before_scroll_top() {
        let area = Rect::new(0, 0, 20, 2);
        let mut buf = Buffer::empty(area);
        let mut rects = Vec::new();
        let runs = vec![SelectableLineRun::selectable(
            1usize,
            vec![
                Line::from("hidden"),
                Line::from("visible a"),
                Line::from("visible b"),
            ],
        )];

        render_selectable_runs_with_rects(area, &mut buf, 1, &runs, Style::new(), &mut rects);
        assert_eq!(rects, vec![(1, Rect::new(0, 0, 20, 2))]);
    }

    #[test]
    fn selection_id_at_matches_visible_run_geometry() {
        let area = Rect::new(2, 4, 20, 3);
        let runs = vec![
            SelectableLineRun::plain(vec![Line::from("header")]),
            SelectableLineRun::selectable(7usize, vec![Line::from("row a"), Line::from("row b")]),
            SelectableLineRun::selectable(8usize, vec![Line::from("row c")]),
        ];

        assert_eq!(selection_id_at(area, 3, 4, 1, &runs), Some(7));
        assert_eq!(selection_id_at(area, 3, 5, 1, &runs), Some(7));
        assert_eq!(selection_id_at(area, 3, 6, 1, &runs), Some(8));
        assert_eq!(selection_id_at(area, 1, 4, 1, &runs), None);
    }

    #[test]
    fn scroll_top_for_section_pins_when_section_fits() {
        assert_eq!(scroll_top_for_section(5, 5, 3, 2, 6), 2);
    }

    #[test]
    fn scroll_top_for_section_keeps_selected_visible() {
        assert_eq!(scroll_top_for_section(10, 3, 18, 10, 19), 16);
    }
}
