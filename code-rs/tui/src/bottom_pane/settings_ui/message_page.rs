use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::colors;

use super::action_page::{SettingsActionPage, SettingsActionPageLayout};
use super::panel::SettingsPanelStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsMessagePageLayout {
    pub(crate) header: Rect,
    pub(crate) body: Rect,
    pub(crate) footer: Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsMessagePage<'a> {
    page: SettingsActionPage<'a>,
    body_lines: Vec<Line<'static>>,
    body_wrap: bool,
    body_style: Style,
}

impl<'a> SettingsMessagePage<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        style: SettingsPanelStyle,
        header_lines: Vec<Line<'static>>,
        body_lines: Vec<Line<'static>>,
        footer_lines: Vec<Line<'static>>,
    ) -> Self {
        Self {
            page: SettingsActionPage::new(title, style, header_lines, footer_lines)
                .with_action_rows(0),
            body_lines,
            body_wrap: true,
            body_style: Style::new().bg(colors::background()).fg(colors::text()),
        }
    }

    pub(crate) fn with_min_body_rows(mut self, min_body_rows: usize) -> Self {
        self.page = self.page.with_min_body_rows(min_body_rows);
        self
    }

    fn layout_from_page(&self, page: SettingsActionPageLayout) -> SettingsMessagePageLayout {
        SettingsMessagePageLayout {
            header: page.header,
            body: page.body,
            footer: page.footer,
        }
    }

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsMessagePageLayout> {
        let page = self.page.layout(area)?;
        Some(self.layout_from_page(page))
    }

    pub(crate) fn layout_content(&self, area: Rect) -> Option<SettingsMessagePageLayout> {
        let page = self.page.layout_content(area)?;
        Some(self.layout_from_page(page))
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsMessagePageLayout> {
        let page = self.page.render(area, buf)?;
        let layout = self.layout_from_page(page);
        if layout.body.width > 0 && layout.body.height > 0 && !self.body_lines.is_empty() {
            let mut paragraph = Paragraph::new(self.body_lines.clone())
                .alignment(Alignment::Left)
                .style(self.body_style);
            if self.body_wrap {
                paragraph = paragraph.wrap(Wrap { trim: true });
            }
            paragraph.render(layout.body, buf);
        }
        Some(layout)
    }

    pub(crate) fn render_content(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsMessagePageLayout> {
        let page = self.page.render_content_shell(area, buf)?;
        let layout = self.layout_from_page(page);
        if layout.body.width > 0 && layout.body.height > 0 && !self.body_lines.is_empty() {
            let mut paragraph = Paragraph::new(self.body_lines.clone())
                .alignment(Alignment::Left)
                .style(self.body_style);
            if self.body_wrap {
                paragraph = paragraph.wrap(Wrap { trim: true });
            }
            paragraph.render(layout.body, buf);
        }
        Some(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Margin;

    #[test]
    fn layout_and_render_agree() {
        let page = SettingsMessagePage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("body")],
            vec![Line::from("footer")],
        )
        .with_min_body_rows(3);
        let area = Rect::new(0, 0, 30, 10);
        let layout = page.layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let rendered = page.render(area, &mut buf).expect("render");
        assert_eq!(layout, rendered);
        assert_eq!(layout.body, Rect::new(2, 2, 26, 3));
        assert_eq!(layout.footer, Rect::new(2, 6, 26, 1));
    }

    #[test]
    fn renders_without_wrapping_configuration() {
        let page = SettingsMessagePage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![],
            vec![Line::from("body")],
            vec![],
        );
        let area = Rect::new(0, 0, 20, 6);
        let mut buf = Buffer::empty(area);
        assert!(page.render(area, &mut buf).is_some());
    }
}
