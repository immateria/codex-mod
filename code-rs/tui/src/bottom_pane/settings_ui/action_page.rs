use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::bottom_pane::chrome::ChromeMode;
use crate::colors;
use crate::ui_interaction::split_header_body_footer;
use crate::util::buffer::fill_rect;

use super::buttons::{
    render_standard_button_strip_aligned, standard_button_at_aligned, StandardButtonSpec,
    TextButtonAlign,
};
use super::panel::SettingsPanelStyle;
use super::sectioned_panel::{SettingsSectionedPanel, SettingsSectionedPanelLayout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsActionPageLayout {
    pub(crate) header: Rect,
    pub(crate) body: Rect,
    pub(crate) status: Rect,
    pub(crate) actions: Rect,
    pub(crate) footer: Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsActionPage<'a> {
    title: Cow<'a, str>,
    style: SettingsPanelStyle,
    header_lines: Vec<Line<'static>>,
    status_lines: Vec<Line<'static>>,
    footer_lines: Vec<Line<'static>>,
    action_rows: usize,
    min_body_rows: usize,
    wrap_lines: bool,
}

pub(crate) struct SettingsActionPageFramed<'p, 'a> {
    page: &'p SettingsActionPage<'a>,
}

pub(crate) struct SettingsActionPageContentOnly<'p, 'a> {
    page: &'p SettingsActionPage<'a>,
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
            status_lines: Vec::new(),
            footer_lines,
            action_rows: 1,
            min_body_rows: 1,
            wrap_lines: false,
        }
    }

    pub(crate) fn framed(&self) -> SettingsActionPageFramed<'_, 'a> {
        SettingsActionPageFramed { page: self }
    }

    pub(crate) fn content_only(&self) -> SettingsActionPageContentOnly<'_, 'a> {
        SettingsActionPageContentOnly { page: self }
    }

    pub(crate) fn layout_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
    ) -> Option<SettingsActionPageLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().layout(area),
            ChromeMode::ContentOnly => self.content_only().layout(area),
        }
    }

    pub(crate) fn render_with_standard_actions_end_in_chrome<Id: Copy>(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        buttons: &[StandardButtonSpec<Id>],
    ) -> Option<SettingsActionPageLayout> {
        match chrome {
            ChromeMode::Framed => self
                .framed()
                .render_with_standard_actions_end(area, buf, buttons),
            ChromeMode::ContentOnly => self
                .content_only()
                .render_with_standard_actions_end(area, buf, buttons),
        }
    }

    pub(crate) fn with_status_lines(mut self, status_lines: Vec<Line<'static>>) -> Self {
        self.status_lines = status_lines;
        self
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
            self.style,
            self.header_lines.len(),
            self.status_lines
                .len()
                .saturating_add(self.action_rows)
                .saturating_add(self.footer_lines.len()),
        )
        .with_min_body_rows(self.min_body_rows)
    }

    fn layout_from_sectioned(
        &self,
        layout: SettingsSectionedPanelLayout,
    ) -> SettingsActionPageLayout {
        let total_h = layout.footer.height;
        let action_height = u16::try_from(self.action_rows)
            .unwrap_or(u16::MAX)
            .min(total_h);
        let remaining = total_h.saturating_sub(action_height);
        let status_height = u16::try_from(self.status_lines.len())
            .unwrap_or(u16::MAX)
            .min(remaining);
        let footer_height = remaining.saturating_sub(status_height);

        let status = Rect::new(layout.footer.x, layout.footer.y, layout.footer.width, status_height);
        let actions = Rect::new(
            layout.footer.x,
            layout.footer.y.saturating_add(status_height),
            layout.footer.width,
            action_height,
        );
        let footer = Rect::new(
            layout.footer.x,
            layout.footer
                .y
                .saturating_add(status_height)
                .saturating_add(action_height),
            layout.footer.width,
            footer_height,
        );
        SettingsActionPageLayout {
            header: layout.header,
            body: layout.body,
            status,
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

    fn layout_framed(&self, area: Rect) -> Option<SettingsActionPageLayout> {
        let layout = self.sectioned_panel().layout(area)?;
        Some(self.layout_from_sectioned(layout))
    }

    fn layout_content_only(&self, area: Rect) -> Option<SettingsActionPageLayout> {
        split_header_body_footer(
            area,
            self.header_lines.len(),
            self.status_lines
                .len()
                .saturating_add(self.action_rows)
                .saturating_add(self.footer_lines.len()),
            u16::try_from(self.min_body_rows).unwrap_or(u16::MAX),
        )
        .map(Into::into)
        .map(|layout| self.layout_from_sectioned(layout))
    }

    fn render_shell_framed(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsActionPageLayout> {
        let layout = self.sectioned_panel().render(area, buf)?;
        let layout = self.layout_from_sectioned(layout);
        self.render_lines(layout.header, buf, &self.header_lines);
        self.render_lines(layout.status, buf, &self.status_lines);
        self.render_lines(layout.footer, buf, &self.footer_lines);
        Some(layout)
    }

    fn render_shell_content_only(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsActionPageLayout> {
        let layout = self.layout_content_only(area)?;
        let base = Style::new().bg(colors::background()).fg(colors::text());
        fill_rect(buf, area, Some(' '), base);
        self.render_lines(layout.header, buf, &self.header_lines);
        self.render_lines(layout.status, buf, &self.status_lines);
        self.render_lines(layout.footer, buf, &self.footer_lines);
        Some(layout)
    }

    pub(crate) fn render_standard_actions<Id: Copy>(
        &self,
        layout: &SettingsActionPageLayout,
        buf: &mut Buffer,
        buttons: &[StandardButtonSpec<Id>],
        align: TextButtonAlign,
    ) {
        if layout.actions.width == 0 || layout.actions.height == 0 || buttons.is_empty() {
            return;
        }
        render_standard_button_strip_aligned(layout.actions, buf, buttons, align);
    }

    pub(crate) fn standard_action_at<Id: Copy>(
        &self,
        layout: &SettingsActionPageLayout,
        x: u16,
        y: u16,
        buttons: &[StandardButtonSpec<Id>],
        align: TextButtonAlign,
    ) -> Option<Id> {
        if layout.actions.width == 0 || layout.actions.height == 0 || buttons.is_empty() {
            return None;
        }
        standard_button_at_aligned(x, y, layout.actions, buttons, align)
    }

    pub(crate) fn standard_action_at_end<Id: Copy>(
        &self,
        layout: &SettingsActionPageLayout,
        x: u16,
        y: u16,
        buttons: &[StandardButtonSpec<Id>],
    ) -> Option<Id> {
        self.standard_action_at(layout, x, y, buttons, TextButtonAlign::End)
    }
}

impl<'p, 'a> SettingsActionPageFramed<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsActionPageLayout> {
        self.page.layout_framed(area)
    }

    pub(crate) fn render_shell(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsActionPageLayout> {
        self.page.render_shell_framed(area, buf)
    }

    pub(crate) fn render_with_standard_actions<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        buttons: &[StandardButtonSpec<Id>],
        align: TextButtonAlign,
    ) -> Option<SettingsActionPageLayout> {
        let layout = self.render_shell(area, buf)?;
        self.page.render_standard_actions(&layout, buf, buttons, align);
        Some(layout)
    }

    pub(crate) fn render_with_standard_actions_end<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        buttons: &[StandardButtonSpec<Id>],
    ) -> Option<SettingsActionPageLayout> {
        self.render_with_standard_actions(area, buf, buttons, TextButtonAlign::End)
    }
}

impl<'p, 'a> SettingsActionPageContentOnly<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsActionPageLayout> {
        self.page.layout_content_only(area)
    }

    pub(crate) fn render_shell(
        &self,
        area: Rect,
        buf: &mut Buffer,
    ) -> Option<SettingsActionPageLayout> {
        self.page.render_shell_content_only(area, buf)
    }

    pub(crate) fn render_with_standard_actions<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        buttons: &[StandardButtonSpec<Id>],
        align: TextButtonAlign,
    ) -> Option<SettingsActionPageLayout> {
        let layout = self.render_shell(area, buf)?;
        self.page.render_standard_actions(&layout, buf, buttons, align);
        Some(layout)
    }

    pub(crate) fn render_with_standard_actions_end<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        buttons: &[StandardButtonSpec<Id>],
    ) -> Option<SettingsActionPageLayout> {
        self.render_with_standard_actions(area, buf, buttons, TextButtonAlign::End)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::buttons::TextButtonAlign;
    use super::super::test_helpers::assert_layout_and_render_shell_agree;
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
        let layout = assert_layout_and_render_shell_agree(
            area,
            |area| page.framed().layout(area),
            |area, buf| page.framed().render_shell(area, buf),
        );
        assert_eq!(layout.status, Rect::new(2, 6, 26, 0));
        assert_eq!(layout.actions, Rect::new(2, 6, 26, 1));
        assert_eq!(layout.footer, Rect::new(2, 7, 26, 1));
    }

    #[test]
    fn layout_places_status_above_actions_and_footer_below() {
        let page = SettingsActionPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![],
            vec![Line::from("footer")],
        )
        .with_status_lines(vec![Line::from("status")]);

        let layout = page.framed().layout(Rect::new(0, 0, 30, 7)).expect("layout");
        assert_eq!(layout.status, Rect::new(2, 3, 26, 1));
        assert_eq!(layout.actions, Rect::new(2, 4, 26, 1));
        assert_eq!(layout.footer, Rect::new(2, 5, 26, 1));
    }

    #[test]
    fn content_shell_layout_and_render_match_action_rects() {
        let page = SettingsActionPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        )
        .with_status_lines(vec![Line::from("status")]);

        let area = Rect::new(0, 0, 30, 8);
        let layout = assert_layout_and_render_shell_agree(
            area,
            |area| page.content_only().layout(area),
            |area, buf| page.content_only().render_shell(area, buf),
        );
        assert_eq!(layout.header, Rect::new(0, 0, 30, 1));
        assert_eq!(layout.body, Rect::new(0, 1, 30, 4));
        assert_eq!(layout.status, Rect::new(0, 5, 30, 1));
        assert_eq!(layout.actions, Rect::new(0, 6, 30, 1));
        assert_eq!(layout.footer, Rect::new(0, 7, 30, 1));
    }

    #[test]
    fn footer_slicing_preserves_actions_when_footer_is_tiny() {
        let page = SettingsActionPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![],
            vec![Line::from("footer")],
        )
        .with_status_lines(vec![Line::from("status")]);

        let sectioned = SettingsSectionedPanelLayout {
            header: Rect::new(0, 0, 20, 0),
            body: Rect::new(0, 0, 20, 0),
            footer: Rect::new(0, 0, 20, 1),
        };
        let layout = page.layout_from_sectioned(sectioned);
        assert_eq!(layout.status.height, 0);
        assert_eq!(layout.actions.height, 1);
        assert_eq!(layout.footer.height, 0);
    }

    #[test]
    fn action_hit_testing_uses_action_row_geometry() {
        let page = SettingsActionPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![],
            vec![],
        );
        let layout = page.framed().layout(Rect::new(0, 0, 30, 7)).expect("layout");
        let buttons = [StandardButtonSpec::new(
            7usize,
            super::super::buttons::SettingsButtonKind::Save,
            false,
            false,
        )];
        assert_eq!(
            page.standard_action_at(
                &layout,
                layout.actions.x + layout.actions.width.saturating_sub(4),
                layout.actions.y,
                &buttons,
                TextButtonAlign::End,
            ),
            Some(7)
        );
    }

    #[test]
    fn layout_in_chrome_matches_concrete_layout() {
        let page = SettingsActionPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![Line::from("header")],
            vec![Line::from("footer")],
        )
        .with_status_lines(vec![Line::from("status")]);
        let area = Rect::new(0, 0, 30, 8);

        assert_eq!(
            page.layout_in_chrome(ChromeMode::Framed, area),
            page.framed().layout(area)
        );
        assert_eq!(
            page.layout_in_chrome(ChromeMode::ContentOnly, area),
            page.content_only().layout(area)
        );
    }
}
