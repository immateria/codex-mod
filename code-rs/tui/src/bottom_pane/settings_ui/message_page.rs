use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::colors;
use crate::bottom_pane::chrome::ChromeMode;

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

pub(crate) struct SettingsMessagePageFramed<'p, 'a> {
    page: &'p SettingsMessagePage<'a>,
}

pub(crate) struct SettingsMessagePageContentOnly<'p, 'a> {
    page: &'p SettingsMessagePage<'a>,
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

    pub(crate) fn framed(&self) -> SettingsMessagePageFramed<'_, 'a> {
        SettingsMessagePageFramed { page: self }
    }

    pub(crate) fn content_only(&self) -> SettingsMessagePageContentOnly<'_, 'a> {
        SettingsMessagePageContentOnly { page: self }
    }

    #[allow(dead_code)]
    pub(crate) fn layout_in_chrome(&self, chrome: ChromeMode, area: Rect) -> Option<SettingsMessagePageLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().layout(area),
            ChromeMode::ContentOnly => self.content_only().layout(area),
        }
    }

    pub(crate) fn render_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsMessagePageLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().render(area, buf),
            ChromeMode::ContentOnly => self.content_only().render(area, buf),
        }
    }

    fn layout_from_page(&self, page: SettingsActionPageLayout) -> SettingsMessagePageLayout {
        SettingsMessagePageLayout {
            header: page.header,
            body: page.body,
            footer: page.footer,
        }
    }

    fn layout_framed(&self, area: Rect) -> Option<SettingsMessagePageLayout> {
        let page = self.page.framed().layout(area)?;
        Some(self.layout_from_page(page))
    }

    #[allow(dead_code)]
    fn layout_content_only(&self, area: Rect) -> Option<SettingsMessagePageLayout> {
        let page = self.page.content_only().layout(area)?;
        Some(self.layout_from_page(page))
    }

    fn render_framed(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsMessagePageLayout> {
        let page = self.page.framed().render_shell(area, buf)?;
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

    fn render_content_only(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsMessagePageLayout> {
        let page = self.page.content_only().render_shell(area, buf)?;
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

impl<'p, 'a> SettingsMessagePageFramed<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsMessagePageLayout> {
        self.page.layout_framed(area)
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsMessagePageLayout> {
        self.page.render_framed(area, buf)
    }
}

impl<'p, 'a> SettingsMessagePageContentOnly<'p, 'a> {
    #[allow(dead_code)]
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsMessagePageLayout> {
        self.page.layout_content_only(area)
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsMessagePageLayout> {
        self.page.render_content_only(area, buf)
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
        let layout = page.framed().layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let rendered = page.framed().render(area, &mut buf).expect("render");
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
        assert!(page.framed().render(area, &mut buf).is_some());
        assert!(page.content_only().render(area, &mut buf).is_some());
    }

    #[test]
    fn content_only_layout_and_render_agree() {
        let page = SettingsMessagePage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("body")],
            vec![Line::from("footer")],
        )
        .with_min_body_rows(3);
        let area = Rect::new(0, 0, 30, 10);
        let layout = page.content_only().layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let rendered = page.content_only().render(area, &mut buf).expect("render");
        assert_eq!(layout, rendered);
    }

    #[test]
    fn chrome_helpers_match_concrete_helpers() {
        let page = SettingsMessagePage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![Line::from("header")],
            vec![Line::from("body")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);

        assert_eq!(
            page.layout_in_chrome(ChromeMode::Framed, area),
            page.framed().layout(area)
        );
        assert_eq!(
            page.layout_in_chrome(ChromeMode::ContentOnly, area),
            page.content_only().layout(area)
        );

        let mut chrome_buf = Buffer::empty(area);
        let mut concrete_buf = Buffer::empty(area);
        assert_eq!(
            page.render_in_chrome(ChromeMode::Framed, area, &mut chrome_buf),
            page.framed().render(area, &mut concrete_buf)
        );

        let mut chrome_buf = Buffer::empty(area);
        let mut concrete_buf = Buffer::empty(area);
        assert_eq!(
            page.render_in_chrome(ChromeMode::ContentOnly, area, &mut chrome_buf),
            page.content_only().render(area, &mut concrete_buf)
        );
    }
}
