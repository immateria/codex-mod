use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Widget};

use crate::colors;
use crate::util::buffer::fill_rect;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SettingsPanelStyle {
    pub(crate) title_alignment: Alignment,
    pub(crate) title_style: Style,
    pub(crate) border_style: Style,
    pub(crate) background_style: Style,
    pub(crate) content_margin: Margin,
    pub(crate) clear_background: bool,
    pub(crate) fill_inner: bool,
}

impl SettingsPanelStyle {
    pub(crate) fn overlay() -> Self {
        Self {
            title_alignment: Alignment::Left,
            title_style: Style::new().fg(colors::text()).bold(),
            border_style: Style::new()
                .fg(colors::border())
                .bg(colors::background()),
            background_style: Style::new()
                .bg(colors::background())
                .fg(colors::text()),
            content_margin: Margin::new(0, 0),
            clear_background: true,
            fill_inner: true,
        }
    }

    pub(crate) fn bottom_pane() -> Self {
        Self {
            title_alignment: Alignment::Center,
            ..Self::overlay()
        }
    }

    pub(crate) fn with_margin(mut self, margin: Margin) -> Self {
        self.content_margin = margin;
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsPanel<'a> {
    pub(crate) title: Cow<'a, str>,
    pub(crate) style: SettingsPanelStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsPanelLayout {
    pub(crate) inner: Rect,
    pub(crate) content: Rect,
}

impl<'a> SettingsPanel<'a> {
    pub(crate) fn new(title: impl Into<Cow<'a, str>>, style: SettingsPanelStyle) -> Self {
        Self {
            title: title.into(),
            style,
        }
    }

    fn block(&self) -> Block<'_> {
        let mut block = Block::bordered()
            .border_style(self.style.border_style)
            .style(self.style.background_style)
            .title_alignment(self.style.title_alignment);

        if !self.title.is_empty() {
            let title_span = Span::styled(format!(" {} ", self.title), self.style.title_style);
            block = block.title(Line::from(vec![title_span]));
        }

        block
    }

    fn layout_from_block(&self, block: &Block<'_>, area: Rect) -> Option<SettingsPanelLayout> {
        if area.width == 0 || area.height == 0 {
            return None;
        }

        let inner = block.inner(area);
        if inner.width == 0 || inner.height == 0 {
            return None;
        }

        let content = inner.inner(self.style.content_margin);
        if content.width == 0 || content.height == 0 {
            return None;
        }

        Some(SettingsPanelLayout { inner, content })
    }

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsPanelLayout> {
        let block = self.block();
        self.layout_from_block(&block, area)
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) -> Option<SettingsPanelLayout> {
        let block = self.block();
        let layout = self.layout_from_block(&block, area)?;

        if self.style.clear_background {
            Clear.render(area, buf);
        }

        block.render(area, buf);

        if self.style.fill_inner {
            fill_rect(buf, layout.inner, Some(' '), self.style.background_style);
        }

        Some(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Cell;

    #[test]
    fn layout_and_render_match_content_rect() {
        let panel = SettingsPanel::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
        );
        let area = Rect::new(0, 0, 20, 6);
        let layout = panel.layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let rendered = panel.render(area, &mut buf).expect("render");
        assert_eq!(layout, rendered);
    }

    #[test]
    fn with_margin_applies_once_to_content_rect() {
        let panel = SettingsPanel::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
        );
        let area = Rect::new(0, 0, 20, 6);
        let layout = panel.layout(area).expect("layout");
        assert_eq!(layout.inner, Rect::new(1, 1, 18, 4));
        assert_eq!(layout.content, Rect::new(2, 1, 16, 4));
    }

    #[test]
    fn zero_or_border_only_areas_do_not_render() {
        let panel = SettingsPanel::new("Test", SettingsPanelStyle::bottom_pane());
        assert!(panel.layout(Rect::new(0, 0, 0, 5)).is_none());
        assert!(panel.layout(Rect::new(0, 0, 5, 0)).is_none());
        assert!(panel.layout(Rect::new(0, 0, 2, 2)).is_none());
    }

    #[test]
    fn render_fills_inner_when_configured() {
        let panel = SettingsPanel::new("Test", SettingsPanelStyle::bottom_pane());
        let area = Rect::new(0, 0, 10, 4);
        let mut buf = Buffer::filled(area, Cell::new("x"));
        let layout = panel.render(area, &mut buf).expect("render");
        assert_eq!(buf[(layout.inner.x, layout.inner.y)].symbol(), " ");
    }
}
