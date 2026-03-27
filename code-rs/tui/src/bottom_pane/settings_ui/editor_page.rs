use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::text::Line;

use crate::bottom_pane::chrome::ChromeMode;
use crate::components::form_text_field::FormTextField;

use super::action_page::{SettingsActionPage, SettingsActionPageLayout};
use super::fields::BorderedField;
use super::panel::SettingsPanelStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsEditorPageLayout {
    pub(crate) header: Rect,
    pub(crate) field_outer: Rect,
    pub(crate) field: Rect,
    pub(crate) actions: Rect,
    pub(crate) footer: Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsEditorPage<'a> {
    page: SettingsActionPage<'a>,
    field_title: Cow<'a, str>,
    field_min_rows: usize,
    field_margin: Margin,
    field_focused: bool,
}

pub(crate) struct SettingsEditorPageFramed<'p, 'a> {
    page: &'p SettingsEditorPage<'a>,
}

pub(crate) struct SettingsEditorPageContentOnly<'p, 'a> {
    page: &'p SettingsEditorPage<'a>,
}

impl<'a> SettingsEditorPage<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        style: SettingsPanelStyle,
        field_title: impl Into<Cow<'a, str>>,
        pre_field_lines: Vec<Line<'static>>,
        post_field_lines: Vec<Line<'static>>,
    ) -> Self {
        Self {
            page: SettingsActionPage::new(title, style, pre_field_lines, Vec::new())
                .with_status_lines(post_field_lines)
                .with_action_rows(0),
            field_title: field_title.into(),
            // Even "single-line" editors read better with a little vertical space.
            field_min_rows: 2,
            field_margin: Margin::new(0, 0),
            field_focused: false,
        }
        .with_field_focused(true)
    }

    pub(crate) fn with_field_margin(mut self, field_margin: Margin) -> Self {
        self.field_margin = field_margin;
        self
    }

    pub(crate) fn with_field_focused(mut self, field_focused: bool) -> Self {
        self.field_focused = field_focused;
        self
    }

    pub(crate) fn with_wrap_lines(mut self, wrap_lines: bool) -> Self {
        self.page = self.page.with_wrap_lines(wrap_lines);
        self
    }

    pub(crate) fn framed(&self) -> SettingsEditorPageFramed<'_, 'a> {
        SettingsEditorPageFramed { page: self }
    }

    pub(crate) fn content_only(&self) -> SettingsEditorPageContentOnly<'_, 'a> {
        SettingsEditorPageContentOnly { page: self }
    }

    pub(crate) fn layout_in_chrome(
        &self,
        chrome: ChromeMode,
        area: Rect,
    ) -> Option<SettingsEditorPageLayout> {
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
        field: &FormTextField,
    ) -> Option<SettingsEditorPageLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().render(area, buf, field),
            ChromeMode::ContentOnly => self.content_only().render(area, buf, field),
        }
    }

    fn min_body_rows(&self) -> usize {
        self.field_min_rows
            .saturating_add(2)
            .saturating_add(self.field_margin.vertical as usize * 2)
    }

    fn page_with_min_rows(&self) -> SettingsActionPage<'a> {
        self.page.clone().with_min_body_rows(self.min_body_rows())
    }

    fn layout_from_page(
        &self,
        layout: SettingsActionPageLayout,
    ) -> Option<SettingsEditorPageLayout> {
        let field_outer = layout.body.inner(self.field_margin);
        if field_outer.width == 0 || field_outer.height == 0 {
            return None;
        }
        let field = BorderedField::new(self.field_title.clone(), self.field_focused).inner(field_outer);
        Some(SettingsEditorPageLayout {
            header: layout.header,
            field_outer,
            field,
            actions: layout.actions,
            footer: layout.footer,
        })
    }

    fn layout_framed(&self, area: Rect) -> Option<SettingsEditorPageLayout> {
        let layout = self.page_with_min_rows().framed().layout(area)?;
        self.layout_from_page(layout)
    }

    fn layout_content_only(&self, area: Rect) -> Option<SettingsEditorPageLayout> {
        let layout = self.page_with_min_rows().content_only().layout(area)?;
        self.layout_from_page(layout)
    }

    fn render_framed(
        &self,
        area: Rect,
        buf: &mut Buffer,
        field: &FormTextField,
    ) -> Option<SettingsEditorPageLayout> {
        let layout = self.page_with_min_rows().framed().render_shell(area, buf)?;
        let layout = self.layout_from_page(layout)?;
        let _ = BorderedField::new(self.field_title.clone(), self.field_focused)
            .render(layout.field_outer, buf, field);

        Some(layout)
    }

    fn render_content_only(
        &self,
        area: Rect,
        buf: &mut Buffer,
        field: &FormTextField,
    ) -> Option<SettingsEditorPageLayout> {
        let layout = self
            .page_with_min_rows()
            .content_only()
            .render_shell(area, buf)?;
        let layout = self.layout_from_page(layout)?;
        let _ = BorderedField::new(self.field_title.clone(), self.field_focused)
            .render(layout.field_outer, buf, field);
        Some(layout)
    }
}

impl<'p, 'a> SettingsEditorPageFramed<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsEditorPageLayout> {
        self.page.layout_framed(area)
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        field: &FormTextField,
    ) -> Option<SettingsEditorPageLayout> {
        self.page.render_framed(area, buf, field)
    }
}

impl<'p, 'a> SettingsEditorPageContentOnly<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsEditorPageLayout> {
        self.page.layout_content_only(area)
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        field: &FormTextField,
    ) -> Option<SettingsEditorPageLayout> {
        self.page.render_content_only(area, buf, field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::form_text_field::FormTextField;
    use ratatui::layout::Margin;
    use ratatui::text::Span;

    #[test]
    fn layout_places_field_between_pre_and_post_lines() {
        let page = SettingsEditorPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            "Field",
            vec![Line::from("a"), Line::from("b")],
            vec![Line::from("c")],
        );
        let area = Rect::new(0, 0, 20, 9);
        let layout = page.framed().layout(area).expect("layout");

        assert_eq!(layout.header, Rect::new(1, 1, 18, 2));
        assert_eq!(layout.field_outer, Rect::new(1, 3, 18, 4));
        assert_eq!(layout.field, Rect::new(2, 4, 16, 2));
        assert_eq!(layout.footer, Rect::new(1, 8, 18, 0));
    }

    #[test]
    fn too_small_area_returns_none() {
        let page = SettingsEditorPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            "Field",
            vec![Line::from("a"), Line::from("b")],
            vec![Line::from("post")],
        );
        assert!(page.framed().layout(Rect::new(0, 0, 20, 8)).is_none());
    }

    #[test]
    fn render_and_layout_agree_on_field_rect() {
        let page = SettingsEditorPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            "Field",
            vec![Line::from(Span::raw("hint"))],
            vec![Line::from(Span::raw("tail"))],
        );
        let area = Rect::new(0, 0, 24, 8);
        let layout = page.framed().layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let field = FormTextField::new_single_line();
        let rendered = page.framed().render(area, &mut buf, &field).expect("render");

        assert_eq!(rendered, layout);
    }

    #[test]
    fn content_only_render_and_layout_agree_on_field_rect() {
        let page = SettingsEditorPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            "Field",
            vec![Line::from(Span::raw("hint"))],
            vec![Line::from(Span::raw("tail"))],
        );
        let area = Rect::new(0, 0, 24, 8);
        let layout = page.content_only().layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let field = FormTextField::new_single_line();
        let rendered = page
            .content_only()
            .render(area, &mut buf, &field)
            .expect("render");

        assert_eq!(rendered, layout);
    }

    #[test]
    fn field_margin_is_applied_inside_field_rect() {
        let page = SettingsEditorPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            "Field",
            vec![Line::from("hint")],
            vec![],
        )
        .with_field_margin(Margin::new(2, 0));
        let area = Rect::new(0, 0, 20, 7);
        let layout = page.framed().layout(area).expect("layout");

        assert_eq!(layout.field_outer, Rect::new(3, 2, 14, 4));
        assert_eq!(layout.field, Rect::new(4, 3, 12, 2));
    }
}
