use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;

use crate::bottom_pane::chrome::ChromeMode;
use crate::colors;
use crate::ui_interaction::split_header_body_footer;
use crate::util::buffer::{fill_rect, write_line};

use super::line_runs::{
    render_selectable_runs,
    SelectableLineRun,
};
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

pub(crate) struct SettingsMenuPageFramed<'p, 'a> {
    page: &'p SettingsMenuPage<'a>,
}

pub(crate) struct SettingsMenuPageContentOnly<'p, 'a> {
    page: &'p SettingsMenuPage<'a>,
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

    pub(crate) fn framed(&self) -> SettingsMenuPageFramed<'_, 'a> {
        SettingsMenuPageFramed { page: self }
    }

    pub(crate) fn content_only(&self) -> SettingsMenuPageContentOnly<'_, 'a> {
        SettingsMenuPageContentOnly { page: self }
    }

    fn layout_framed(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        self.panel.layout(area)
    }

    fn layout_content(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        split_header_body_footer(area, self.header_lines.len(), self.footer_lines.len(), 1)
            .map(Into::into)
    }

    fn render_shell_framed(
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

    fn render_shell_content_only(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.layout_content(area)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        fill_rect(buf, area, Some(' '), base);
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

    pub(crate) fn layout_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
    ) -> Option<SettingsSectionedPanelLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().layout(area),
            ChromeMode::ContentOnly => self.content_only().layout(area),
        }
    }

    pub(crate) fn render_shell_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().render_shell(area, buf),
            ChromeMode::ContentOnly => self.content_only().render_shell(area, buf),
        }
    }

    pub(crate) fn render_menu_rows_in_chrome<Id: Copy + PartialEq>(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        selected_id: Option<Id>,
        rows: &[SettingsMenuRow<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        match chrome {
            ChromeMode::Framed => {
                self.framed()
                    .render_menu_rows(area, buf, scroll_top, selected_id, rows)
            }
            ChromeMode::ContentOnly => self
                .content_only()
                .render_menu_rows(area, buf, scroll_top, selected_id, rows),
        }
    }

    pub(crate) fn render_runs_in_chrome<Id: Copy>(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        runs: &[SelectableLineRun<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().render_runs(area, buf, scroll_top, runs),
            ChromeMode::ContentOnly => self
                .content_only()
                .render_runs(area, buf, scroll_top, runs),
        }
    }
}

impl<'p, 'a> SettingsMenuPageFramed<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        self.page.layout_framed(area)
    }

    pub(crate) fn render_shell(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        self.page.render_shell_framed(area, buf)
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
        render_menu_rows(
            layout.body,
            buf,
            scroll_top,
            selected_id,
            rows,
            base,
        );
        Some(layout)
    }

    pub(crate) fn render_runs<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        runs: &[SelectableLineRun<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.render_shell(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        render_selectable_runs(layout.body, buf, scroll_top, runs, base);
        Some(layout)
    }
}

impl<'p, 'a> SettingsMenuPageContentOnly<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        self.page.layout_content(area)
    }

    pub(crate) fn render_shell(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        self.page.render_shell_content_only(area, buf)
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
        render_menu_rows(
            layout.body,
            buf,
            scroll_top,
            selected_id,
            rows,
            base,
        );
        Some(layout)
    }

    pub(crate) fn render_runs<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        scroll_top: usize,
        runs: &[SelectableLineRun<'_, Id>],
    ) -> Option<SettingsSectionedPanelLayout> {
        let layout = self.render_shell(area, buf)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        render_selectable_runs(layout.body, buf, scroll_top, runs, base);
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
        let framed = page.framed();
        let layout = framed.layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let rendered = framed.render_shell(area, &mut buf).expect("render");
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
        let layout = page.framed().layout(area).expect("layout");
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

    #[test]
    fn shell_less_layout_matches_header_body_footer_math() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 8);
        let layout = page.content_only().layout(area).expect("layout");

        assert_eq!(layout.header, Rect::new(0, 0, 30, 1));
        assert_eq!(layout.body, Rect::new(0, 1, 30, 6));
        assert_eq!(layout.footer, Rect::new(0, 7, 30, 1));
    }

    #[test]
    fn render_content_menu_rows_renders_without_outer_frame() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let rows = vec![SettingsMenuRow::new(1usize, "row")];
        let area = Rect::new(0, 0, 20, 4);
        let mut buf = Buffer::empty(area);

        let layout = page
            .content_only()
            .render_menu_rows(area, &mut buf, 0, Some(1usize), &rows)
            .expect("render");

        assert_eq!(layout.header, Rect::new(0, 0, 20, 1));
        assert_eq!(buf[(0, 0)].symbol(), "h");
        assert_eq!(buf[(0, 1)].symbol(), "›");
    }

    #[test]
    fn render_shell_in_chrome_matches_concrete_impls() {
        let page = SettingsMenuPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);

        let mut framed_buf = Buffer::empty(area);
        let mut framed_expected_buf = Buffer::empty(area);
        assert_eq!(
            page.render_shell_in_chrome(ChromeMode::Framed, area, &mut framed_buf),
            page.framed().render_shell(area, &mut framed_expected_buf)
        );

        let mut content_buf = Buffer::empty(area);
        let mut content_expected_buf = Buffer::empty(area);
        assert_eq!(
            page.render_shell_in_chrome(ChromeMode::ContentOnly, area, &mut content_buf),
            page.content_only()
                .render_shell(area, &mut content_expected_buf)
        );
    }
}
