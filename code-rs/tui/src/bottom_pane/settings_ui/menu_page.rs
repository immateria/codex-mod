use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;

use crate::colors;
use crate::util::buffer::{fill_rect, write_line};

use super::line_runs::{render_selectable_runs, SelectableLineRun};
use super::menu_rows::{
    render_menu_rows,
    selection_id_at as selection_menu_id_at,
    SettingsMenuRow,
};
use super::panel::SettingsPanelStyle;
use super::sectioned_panel::{SettingsSectionedPanel, SettingsSectionedPanelLayout};

#[derive(Clone, Debug)]
pub(crate) struct SettingsMenuPage<'a> {
    panel: SettingsSectionedPanel<'a>,
    header_lines: Vec<Line<'static>>,
    footer_lines: Vec<Line<'static>>,
}

impl<'a> SettingsMenuPage<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        style: SettingsPanelStyle,
        header_lines: Vec<Line<'static>>,
        footer_lines: Vec<Line<'static>>,
    ) -> Self {
        let panel = SettingsSectionedPanel::new(
            title,
            style,
            header_lines.len(),
            footer_lines.len(),
        )
        .with_min_body_rows(1);

        Self {
            panel,
            header_lines,
            footer_lines,
        }
    }

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        self.panel.layout(area)
    }

    pub(crate) fn render_shell(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.panel.render(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        render_lines(layout.header, buf, &self.header_lines, base);
        render_lines(layout.footer, buf, &self.footer_lines, base);
        Some(layout)
    }

    pub(crate) fn selection_menu_id_in_body<Id: Copy + PartialEq>(
        body: Rect,
        x: u16,
        y: u16,
        scroll_top: usize,
        rows: &[SettingsMenuRow<'_, Id>],
    ) -> Option<Id> {
        selection_menu_id_at(body, x, y, scroll_top, rows)
    }

    pub(crate) fn render_menu_rows<Id: Copy + PartialEq>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        selected_id: Option<Id>,
        rows: &[SettingsMenuRow<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.render_shell(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        let mut rects = Vec::new();
        render_menu_rows(
            layout.body,
            buf,
            scroll_top,
            selected_id,
            rows,
            base,
            &mut rects,
        );
        Some(layout)
    }

    pub(crate) fn render_runs<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        runs: &[SelectableLineRun<'_, Id>],
        out_rects: &mut Vec<(Id, Rect)>,
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.render_shell(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        render_selectable_runs(layout.body, buf, scroll_top, runs, base, out_rects);
        Some(layout)
    }
}

fn render_lines(area: Rect, buf: &mut Buffer, lines: &[Line<'_>], base_style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    fill_rect(buf, area, Some(' '), base_style);
    for (idx, line) in lines.iter().enumerate().take(area.height as usize) {
        let y = area.y.saturating_add(idx as u16);
        write_line(buf, area.x, y, area.width, line, base_style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Margin;

    #[test]
    fn render_shell_and_layout_agree() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);
        let layout = page.layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let rendered = page.render_shell(area, &mut buf).expect("render");
        assert_eq!(layout, rendered);
    }

    #[test]
    fn selection_menu_id_uses_body_rect() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);
        let layout = page.layout(area).expect("layout");
        let rows = vec![SettingsMenuRow::new(7usize, "row")];

        assert_eq!(
            SettingsMenuPage::selection_menu_id_in_body(
                layout.body,
                layout.body.x,
                layout.body.y,
                0,
                &rows,
            ),
            Some(7)
        );
        assert_eq!(
            SettingsMenuPage::selection_menu_id_in_body(
                layout.body,
                layout.header.x,
                layout.header.y,
                0,
                &rows,
            ),
            None
        );
    }
}
