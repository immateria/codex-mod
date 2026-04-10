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
    body_scroll: u16,
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
            body_scroll: 0,
        }
    }

    pub(crate) fn with_min_body_rows(mut self, min_body_rows: usize) -> Self {
        self.page = self.page.with_min_body_rows(min_body_rows);
        self
    }

    /// Set the vertical scroll offset (in lines) for the body content.
    #[allow(dead_code)] // used in tests
    pub(crate) fn with_body_scroll(mut self, scroll: u16) -> Self {
        self.body_scroll = scroll;
        self
    }

    /// Return the number of wrapped body lines that overflow the given
    /// `body_height`.  Returns 0 when all content fits.
    #[allow(dead_code)] // used in tests
    pub(crate) fn body_overflow(&self, body_width: u16, body_height: u16) -> usize {
        if self.body_lines.is_empty() || body_width == 0 || body_height == 0 {
            return 0;
        }
        let mut paragraph = Paragraph::new(self.body_lines.clone());
        if self.body_wrap {
            paragraph = paragraph.wrap(Wrap { trim: true });
        }
        let total = paragraph.line_count(body_width);
        total.saturating_sub(body_height as usize)
    }

    pub(crate) fn framed(&self) -> SettingsMessagePageFramed<'_, 'a> {
        SettingsMessagePageFramed { page: self }
    }

    pub(crate) fn content_only(&self) -> SettingsMessagePageContentOnly<'_, 'a> {
        SettingsMessagePageContentOnly { page: self }
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

    fn render_body_into(&self, layout: &SettingsMessagePageLayout, buf: &mut Buffer) {
        if layout.body.width > 0 && layout.body.height > 0 && !self.body_lines.is_empty() {
            let mut paragraph = Paragraph::new(self.body_lines.clone())
                .alignment(Alignment::Left)
                .style(self.body_style);
            if self.body_wrap {
                paragraph = paragraph.wrap(Wrap { trim: true });
            }
            // Clamp scroll so we never scroll past the last line of content.
            let total = paragraph.line_count(layout.body.width);
            let max_scroll = total.saturating_sub(layout.body.height as usize) as u16;
            let clamped = self.body_scroll.min(max_scroll);
            paragraph.scroll((clamped, 0)).render(layout.body, buf);
        }
    }

    fn render_framed(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsMessagePageLayout> {
        let page = self.page.framed().render_shell(area, buf)?;
        let layout = self.layout_from_page(page);
        self.render_body_into(&layout, buf);
        Some(layout)
    }

    fn render_content_only(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsMessagePageLayout> {
        let page = self.page.content_only().render_shell(area, buf)?;
        let layout = self.layout_from_page(page);
        self.render_body_into(&layout, buf);
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
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsMessagePageLayout> {
        self.page.render_content_only(area, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_and_render_agree() {
        let page = SettingsMessagePage::new(
            "Test",
            SettingsPanelStyle::bottom_pane_padded(),
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
    fn render_in_chrome_matches_concrete_helpers() {
        let page = SettingsMessagePage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![Line::from("header")],
            vec![Line::from("body")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);

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

    #[test]
    fn body_scroll_and_overflow() {
        // Create body content with 10 lines in a small area that can show 3.
        let body_lines: Vec<Line<'static>> = (0..10)
            .map(|i| Line::from(format!("line {i}")))
            .collect();
        let page = SettingsMessagePage::new(
            "Scroll",
            SettingsPanelStyle::bottom_pane_padded(),
            vec![Line::from("hdr")],
            body_lines,
            vec![Line::from("ftr")],
        )
        .with_min_body_rows(3);

        let area = Rect::new(0, 0, 30, 10);
        let layout = page.framed().layout(area).expect("layout");
        let overflow = page.body_overflow(layout.body.width, layout.body.height);
        assert!(overflow > 0, "10 body lines should overflow a 3-row body");

        // Render without scroll — first line visible.
        let mut buf = Buffer::empty(area);
        page.framed().render(area, &mut buf);
        let first_row = (layout.body.x..layout.body.x + layout.body.width)
            .map(|x| buf.cell((x, layout.body.y)).unwrap().symbol().to_string())
            .collect::<String>();
        assert!(first_row.contains("line 0"), "first visible row should be line 0");

        // Render with scroll offset — "line 0" should no longer be first.
        let scrolled = page.with_body_scroll(5);
        let mut buf2 = Buffer::empty(area);
        scrolled.framed().render(area, &mut buf2);
        let scrolled_row = (layout.body.x..layout.body.x + layout.body.width)
            .map(|x| buf2.cell((x, layout.body.y)).unwrap().symbol().to_string())
            .collect::<String>();
        assert!(scrolled_row.contains("line 5"), "after scroll=5, first visible row should be line 5");
    }
}
