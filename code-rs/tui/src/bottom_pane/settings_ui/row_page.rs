use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;

use super::frame::{SettingsFrame, SettingsFrameLayout};
use super::rows::{selection_index_at as row_selection_index_at, render_kv_rows, KeyValueRow};

#[derive(Clone, Debug)]
pub(crate) struct SettingsRowPage<'a> {
    frame: SettingsFrame<'a>,
}

pub(crate) struct SettingsRowPageFramed<'p, 'a> {
    page: &'p SettingsRowPage<'a>,
}

pub(crate) struct SettingsRowPageContentOnly<'p, 'a> {
    page: &'p SettingsRowPage<'a>,
}

impl<'a> SettingsRowPage<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        header_lines: Vec<Line<'static>>,
        footer_lines: Vec<Line<'static>>,
    ) -> Self {
        Self {
            frame: SettingsFrame::new(title, header_lines, footer_lines),
        }
    }

    pub(crate) fn framed(&self) -> SettingsRowPageFramed<'_, 'a> {
        SettingsRowPageFramed { page: self }
    }

    pub(crate) fn content_only(&self) -> SettingsRowPageContentOnly<'_, 'a> {
        SettingsRowPageContentOnly { page: self }
    }

    pub(crate) fn selection_index_at(
        body: Rect,
        x: u16,
        y: u16,
        scroll_top: usize,
        total: usize,
    ) -> Option<usize> {
        row_selection_index_at(body, x, y, scroll_top, total)
    }
}

impl<'p, 'a> SettingsRowPageFramed<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsFrameLayout> {
        self.page.frame.layout(area)
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        selected_idx: Option<usize>,
        rows: &[KeyValueRow<'_>],
    ) -> Option<SettingsFrameLayout> {
        let layout = self.page.frame.render(area, buf)?;
        render_kv_rows(layout.body, buf, scroll_top, selected_idx, rows);
        Some(layout)
    }
}

impl<'p, 'a> SettingsRowPageContentOnly<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsFrameLayout> {
        self.page.frame.layout_content(area)
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        selected_idx: Option<usize>,
        rows: &[KeyValueRow<'_>],
    ) -> Option<SettingsFrameLayout> {
        let layout = self.page.frame.render_content_shell(area, buf)?;
        render_kv_rows(layout.body, buf, scroll_top, selected_idx, rows);
        Some(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_index_uses_frame_body_rect() {
        let page = SettingsRowPage::new(
            "Test",
            vec![Line::from("header"), Line::from("")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);
        let layout = page.framed().layout(area).expect("layout");

        assert_eq!(
            SettingsRowPage::selection_index_at(layout.body, layout.body.x, layout.body.y, 3, 10),
            Some(3)
        );
        assert_eq!(
            SettingsRowPage::selection_index_at(
                layout.body,
                layout.body.x,
                layout.body.y.saturating_add(2),
                3,
                10,
            ),
            Some(5)
        );
        assert_eq!(
            SettingsRowPage::selection_index_at(layout.body, layout.header.x, layout.header.y, 3, 10),
            None
        );
    }

    #[test]
    fn render_layout_matches_expected_geometry() {
        let page = SettingsRowPage::new("Test", vec![Line::from("header")], vec![]);
        let area = Rect::new(0, 0, 24, 8);
        let rows = vec![KeyValueRow::new("Row")];
        let mut buf = Buffer::empty(area);
        let rendered = page
            .framed()
            .render(area, &mut buf, 0, Some(0), &rows)
            .expect("render");

        assert_eq!(rendered.header, Rect::new(1, 1, 22, 1));
        assert_eq!(rendered.body, Rect::new(1, 2, 22, 5));
        assert_eq!(rendered.footer, Rect::new(1, 7, 22, 0));
    }
}
