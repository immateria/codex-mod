use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::colors;

use super::buttons::{
    render_text_button_strip_aligned, text_button_at_aligned, TextButton, TextButtonAlign,
};
use super::panel::SettingsPanelStyle;
use super::sectioned_panel::{SettingsSectionedPanel, SettingsSectionedPanelLayout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsActionPageLayout {
    pub(crate) header: Rect,
    pub(crate) body: Rect,
    pub(crate) actions: Rect,
    pub(crate) footer: Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsActionPage<'a> {
    title: Cow<'a, str>,
    style: SettingsPanelStyle,
    header_lines: Vec<Line<'static>>,
    footer_lines: Vec<Line<'static>>,
    action_rows: usize,
    min_body_rows: usize,
    wrap_lines: bool,
}

impl<'a> SettingsActionPage<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        style: SettingsPanelStyle,
        header_lines: Vec<Line<'static>>,
        footer_lines: Vec<Line<'static>>,
    ) -> Self {
        Self {
            title: title.into(),
            style,
            header_lines,
            footer_lines,
            action_rows: 1,
            min_body_rows: 1,
            wrap_lines: false,
        }
    }

    pub(crate) fn with_action_rows(mut self, action_rows: usize) -> Self {
        self.action_rows = action_rows;
        self
    }

    pub(crate) fn with_min_body_rows(mut self, min_body_rows: usize) -> Self {
        self.min_body_rows = min_body_rows.max(1);
        self
    }

    pub(crate) fn with_wrap_lines(mut self, wrap_lines: bool) -> Self {
        self.wrap_lines = wrap_lines;
        self
    }

    fn sectioned_panel(&self) -> SettingsSectionedPanel<'_> {
        SettingsSectionedPanel::new(
            self.title.clone(),
            self.style.clone(),
            self.header_lines.len(),
            self.action_rows.saturating_add(self.footer_lines.len()),
        )
        .with_min_body_rows(self.min_body_rows)
    }

    fn layout_from_sectioned(
        &self,
        layout: SettingsSectionedPanelLayout,
    ) -> SettingsActionPageLayout {
        let action_height = (self.action_rows.min(layout.footer.height as usize)) as u16;
        let actions = Rect::new(
            layout.footer.x,
            layout.footer.y,
            layout.footer.width,
            action_height,
        );
        let footer = Rect::new(
            layout.footer.x,
            layout.footer.y.saturating_add(action_height),
            layout.footer.width,
            layout.footer.height.saturating_sub(action_height),
        );
        SettingsActionPageLayout {
            header: layout.header,
            body: layout.body,
            actions,
            footer,
        }
    }

    fn render_lines(&self, area: Rect, buf: &mut Buffer, lines: &[Line<'static>]) {
        if area.width == 0 || area.height == 0 || lines.is_empty() {
            return;
        }

        let mut paragraph = Paragraph::new(lines.to_vec())
            .style(Style::new().bg(colors::background()).fg(colors::text()));
        if self.wrap_lines {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }
        paragraph.render(area, buf);
    }

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsActionPageLayout> {
        let layout = self.sectioned_panel().layout(area)?;
        Some(self.layout_from_sectioned(layout))
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsActionPageLayout> {
        let layout = self.sectioned_panel().render(area, buf)?;
        let layout = self.layout_from_sectioned(layout);
        self.render_lines(layout.header, buf, &self.header_lines);
        self.render_lines(layout.footer, buf, &self.footer_lines);
        Some(layout)
    }

    pub(crate) fn render_actions<Id>(
        &self,
        layout: &SettingsActionPageLayout,
        buf: &mut Buffer,
        buttons: &[TextButton<'_, Id>],
        align: TextButtonAlign,
    ) {
        if layout.actions.width == 0 || layout.actions.height == 0 || buttons.is_empty() {
            return;
        }
        render_text_button_strip_aligned(layout.actions, buf, buttons, align);
    }

    pub(crate) fn action_at<Id: Copy>(
        &self,
        layout: &SettingsActionPageLayout,
        x: u16,
        y: u16,
        buttons: &[TextButton<'_, Id>],
        align: TextButtonAlign,
    ) -> Option<Id> {
        if layout.actions.width == 0 || layout.actions.height == 0 || buttons.is_empty() {
            return None;
        }
        text_button_at_aligned(x, y, layout.actions, buttons, align)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::buttons::{TextButton, TextButtonAlign};
    use ratatui::layout::Margin;

    #[test]
    fn layout_and_render_match_action_rects() {
        let page = SettingsActionPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        );
        let area = Rect::new(0, 0, 30, 10);
        let layout = page.layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let rendered = page.render(area, &mut buf).expect("render");
        assert_eq!(layout, rendered);
        assert_eq!(layout.actions, Rect::new(2, 6, 26, 1));
        assert_eq!(layout.footer, Rect::new(2, 7, 26, 1));
    }

    #[test]
    fn action_hit_testing_uses_action_row_geometry() {
        let page = SettingsActionPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![],
            vec![],
        );
        let layout = page.layout(Rect::new(0, 0, 30, 7)).expect("layout");
        let buttons = [TextButton::new(7usize, "Save", false, false, Style::new())];
        assert_eq!(
            page.action_at(
                &layout,
                layout.actions.x + layout.actions.width.saturating_sub(4),
                layout.actions.y,
                &buttons,
                TextButtonAlign::End,
            ),
            Some(7)
        );
    }
}
