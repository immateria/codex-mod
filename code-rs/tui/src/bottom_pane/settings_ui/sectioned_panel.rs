use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

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
        if content.width == 0 || content.height == 0 {
            return None;
        }

        let header_h = u16::try_from(self.header_rows).unwrap_or(u16::MAX);
        let footer_h = u16::try_from(self.footer_rows).unwrap_or(u16::MAX);
        let min_body = u16::try_from(self.min_body_rows).unwrap_or(u16::MAX).max(1);

        let header_h = header_h.min(content.height);
        let max_bottom = content.height.saturating_sub(header_h);
        let footer_h = footer_h.min(max_bottom);

        // Prefer keeping a little padding under the footer in framed panels,
        // but always prioritize having enough body space.
        let desired_reserved_bottom = if footer_h == 0 {
            0
        } else {
            let pad = 3u16.saturating_sub(footer_h);
            footer_h.saturating_add(pad).min(max_bottom)
        };

        // When the body is already forcing a larger minimum height, keep a
        // spacer above the footer if we can afford it.
        let desired_gap = if footer_h > 0 && min_body > 1 { 1 } else { 0 };

        let mut gap = desired_gap;
        let mut max_reserved_bottom = content
            .height
            .saturating_sub(header_h)
            .saturating_sub(min_body)
            .saturating_sub(gap);
        if max_reserved_bottom < footer_h {
            gap = 0;
            max_reserved_bottom = content
                .height
                .saturating_sub(header_h)
                .saturating_sub(min_body);
        }
        if max_reserved_bottom < footer_h {
            return None;
        }

        let reserved_bottom = desired_reserved_bottom
            .min(max_reserved_bottom)
            .max(footer_h);

        // If we already had to drop bottom padding to keep the body usable,
        // don't also spend a row on a spacer above the footer.
        if reserved_bottom == footer_h {
            gap = 0;
        }

        let footer_y = content
            .y
            .saturating_add(content.height)
            .saturating_sub(reserved_bottom);

        let body_y = content.y.saturating_add(header_h);
        let available_body = footer_y.saturating_sub(body_y);
        let body_h = available_body.saturating_sub(gap);

        if body_h < min_body {
            return None;
        }

        Some(SettingsSectionedPanelLayout {
            header: Rect::new(content.x, content.y, content.width, header_h),
            body: Rect::new(content.x, body_y, content.width, body_h),
            footer: Rect::new(content.x, footer_y, content.width, footer_h),
        })
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
