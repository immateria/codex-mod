use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::text::Line;

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
            field_min_rows: 1,
            field_margin: Margin::new(0, 0),
            field_focused: true,
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

    fn min_body_rows(&self) -> usize {
        self.field_min_rows
            .saturating_add(2)
            .saturating_add(self.field_margin.vertical as usize * 2)
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

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsEditorPageLayout> {
        let page = self.page.clone().with_min_body_rows(self.min_body_rows());
        let layout = page.framed().layout(area)?;
        self.layout_from_page(layout)
    }

    pub(crate) fn layout_content(&self, area: Rect) -> Option<SettingsEditorPageLayout> {
        let page = self.page.clone().with_min_body_rows(self.min_body_rows());
        let layout = page.content_only().layout(area)?;
        self.layout_from_page(layout)
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        field: &FormTextField,
    ) -> Option<SettingsEditorPageLayout> {
        let page = self.page.clone().with_min_body_rows(self.min_body_rows());
        let layout = page.framed().render_shell(area, buf)?;
        let layout = self.layout_from_page(layout)?;
        let _ = BorderedField::new(self.field_title.clone(), self.field_focused)
            .render(layout.field_outer, buf, field);

        Some(layout)
    }

    pub(crate) fn render_content(
        &self,
        area: Rect,
        buf: &mut Buffer,
        field: &FormTextField,
    ) -> Option<SettingsEditorPageLayout> {
        let page = self.page.clone().with_min_body_rows(self.min_body_rows());
        let layout = page.content_only().render_shell(area, buf)?;
        let layout = self.layout_from_page(layout)?;
        let _ = BorderedField::new(self.field_title.clone(), self.field_focused)
            .render(layout.field_outer, buf, field);
        Some(layout)
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
        let layout = page.layout(area).expect("layout");

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
        assert!(page.layout(Rect::new(0, 0, 20, 8)).is_none());
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
        let layout = page.layout(area).expect("layout");
        let mut buf = Buffer::empty(area);
        let field = FormTextField::new_single_line();
        let rendered = page.render(area, &mut buf, &field).expect("render");

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
        let layout = page.layout(area).expect("layout");

        assert_eq!(layout.field_outer, Rect::new(3, 2, 14, 4));
        assert_eq!(layout.field, Rect::new(4, 3, 12, 2));
    }
}
