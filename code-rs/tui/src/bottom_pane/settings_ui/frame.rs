use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::colors;

use super::layout::DEFAULT_FOOTER_GAP_LINES;

#[derive(Clone, Debug)]
pub(crate) struct SettingsFrame<'a> {
    pub(crate) title: Cow<'a, str>,
    pub(crate) header_lines: Vec<Line<'static>>,
    pub(crate) footer_lines: Vec<Line<'static>>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SettingsFrameLayout {
    pub(crate) header: Rect,
    pub(crate) body: Rect,
    pub(crate) footer: Rect,
}

fn clamp_u16(value: usize) -> u16 {
    value.min(u16::MAX as usize) as u16
}

impl<'a> SettingsFrame<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        header_lines: Vec<Line<'static>>,
        footer_lines: Vec<Line<'static>>,
    ) -> Self {
        Self {
            title: title.into(),
            header_lines,
            footer_lines,
        }
    }

    fn make_block(&self) -> Block<'_> {
        Block::bordered()
            .border_style(Style::new().fg(colors::border()))
            .style(Style::new().bg(colors::background()).fg(colors::text()))
            .title_top(Line::from(self.title.as_ref()).centered())
    }

    fn inner_for_block(block: &Block<'_>, area: Rect) -> Option<Rect> {
        if area.width == 0 || area.height == 0 {
            return None;
        }

        let inner = block.inner(area);
        if inner.width == 0 || inner.height == 0 {
            None
        } else {
            Some(inner)
        }
    }

    fn layout_from_inner(&self, inner: Rect) -> SettingsFrameLayout {
        let available_height = inner.height as usize;
        let header_height = self.header_lines.len().min(available_height);
        let footer_reserved = if self.footer_lines.is_empty() || available_height <= header_height {
            0
        } else {
            DEFAULT_FOOTER_GAP_LINES + self.footer_lines.len()
        };
        let body_height = available_height.saturating_sub(header_height + footer_reserved);
        let header_y = inner.y;
        let body_y = inner.y.saturating_add(clamp_u16(header_height));
        let footer_y = body_y.saturating_add(clamp_u16(body_height));
        let footer_content_y = footer_y.saturating_add(clamp_u16(DEFAULT_FOOTER_GAP_LINES));

        SettingsFrameLayout {
            header: Rect::new(
                inner.x,
                header_y,
                inner.width,
                clamp_u16(header_height),
            ),
            body: Rect::new(
                inner.x,
                body_y,
                inner.width,
                clamp_u16(body_height),
            ),
            footer: Rect::new(
                inner.x,
                footer_content_y,
                inner.width,
                clamp_u16(self.footer_lines.len()),
            ),
        }
    }

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsFrameLayout> {
        let block = self.make_block();
        let inner = Self::inner_for_block(&block, area)?;
        Some(self.layout_from_inner(inner))
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsFrameLayout> {
        let block = self.make_block();
        let inner = Self::inner_for_block(&block, area)?;
        let layout = self.layout_from_inner(inner);
        Clear.render(area, buf);
        block.render(area, buf);
        let base = Style::new().bg(colors::background()).fg(colors::text());

        if layout.header.height > 0 {
            Paragraph::new(self.header_lines.clone())
                .style(base)
                .render(layout.header, buf);
        }

        if !self.footer_lines.is_empty() && layout.footer.height > 0 {
            Paragraph::new(self.footer_lines.clone())
                .style(base)
                .render(layout.footer, buf);
        }

        Some(layout)
    }
}

impl SettingsFrameLayout {
    pub(crate) fn visible_rows(&self) -> usize {
        self.body.height as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_frame_layout_splits_header_body_and_footer() {
        let area = Rect::new(0, 0, 40, 12);
        let layout = SettingsFrame::new(" Test ", vec![Line::from("a"), Line::from("b")], vec![
            Line::from("c"),
            Line::from("d"),
        ])
        .layout(area)
        .expect("layout");

        assert_eq!(layout.header.height as usize, 2);
        assert_eq!(layout.header.y, 1);
        assert_eq!(layout.header.height, 2);
        assert_eq!(layout.body.y, 3);
        assert_eq!(layout.footer.y, 9);
        assert_eq!(layout.footer.height, 2);
        assert_eq!(layout.visible_rows(), 5);
    }

    #[test]
    fn compute_frame_layout_allows_zero_visible_rows_when_body_is_exhausted() {
        let area = Rect::new(0, 0, 20, 4);
        let layout = SettingsFrame::new(" Test ", vec![Line::from("a"), Line::from("b")], vec![
            Line::from("c"),
            Line::from("d"),
        ])
        .layout(area)
        .expect("layout");

        assert_eq!(layout.body.height, 0);
        assert_eq!(layout.visible_rows(), 0);
    }
}
