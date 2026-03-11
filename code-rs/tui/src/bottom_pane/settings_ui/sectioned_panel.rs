use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ui_interaction::split_header_body_footer;

use super::panel::{SettingsPanel, SettingsPanelStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsSectionedPanelLayout {
    pub(crate) header: Rect,
    pub(crate) body: Rect,
    pub(crate) footer: Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsSectionedPanel<'a> {
    panel: SettingsPanel<'a>,
    header_rows: usize,
    footer_rows: usize,
    min_body_rows: usize,
}

impl From<crate::ui_interaction::HeaderBodyFooterLayout> for SettingsSectionedPanelLayout {
    fn from(layout: crate::ui_interaction::HeaderBodyFooterLayout) -> Self {
        Self {
            header: layout.header,
            body: layout.body,
            footer: layout.footer,
        }
    }
}

impl<'a> SettingsSectionedPanel<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        style: SettingsPanelStyle,
        header_rows: usize,
        footer_rows: usize,
    ) -> Self {
        Self {
            panel: SettingsPanel::new(title, style),
            header_rows,
            footer_rows,
            min_body_rows: 1,
        }
    }

    /// Minimum body height is clamped to at least 1 row.
    pub(crate) fn with_min_body_rows(mut self, min_body_rows: usize) -> Self {
        self.min_body_rows = min_body_rows.max(1);
        self
    }

    fn layout_from_content(&self, content: Rect) -> Option<SettingsSectionedPanelLayout> {
        split_header_body_footer(
            content,
            self.header_rows,
            self.footer_rows,
            self.min_body_rows.min(u16::MAX as usize) as u16,
        )
        .map(Into::into)
    }

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsSectionedPanelLayout> {
        let panel = self.panel.layout(area)?;
        self.layout_from_content(panel.content)
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsSectionedPanelLayout> {
        let panel = self.panel.render(area, buf)?;
        self.layout_from_content(panel.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Margin;

    #[test]
    fn render_produces_expected_section_rects() {
        let panel = SettingsSectionedPanel::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            1,
            1,
        );
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        let rendered = panel.render(area, &mut buf).expect("render");
        assert_eq!(rendered.header, Rect::new(2, 1, 36, 1));
        assert_eq!(rendered.body, Rect::new(2, 2, 36, 4));
        assert_eq!(rendered.footer, Rect::new(2, 6, 36, 1));
    }

    #[test]
    fn margin_applies_before_section_split() {
        let panel = SettingsSectionedPanel::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            1,
            1,
        );
        let area = Rect::new(0, 0, 20, 8);
        let layout = panel.layout(area).expect("layout");
        assert_eq!(layout.header, Rect::new(2, 1, 16, 1));
        assert_eq!(layout.body, Rect::new(2, 2, 16, 2));
        assert_eq!(layout.footer, Rect::new(2, 4, 16, 1));
    }

    #[test]
    fn tiny_area_returns_none() {
        let panel = SettingsSectionedPanel::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            1,
            1,
        );
        assert!(panel.layout(Rect::new(0, 0, 0, 8)).is_none());
        assert!(panel.layout(Rect::new(0, 0, 4, 4)).is_none());
    }
}
