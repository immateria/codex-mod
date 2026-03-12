use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};

use crate::components::form_text_field::FormTextField;

use super::action_page::{SettingsActionPage, SettingsActionPageLayout};
use super::buttons::{StandardButtonSpec, TextButtonAlign};
use super::fields::BorderedField;

#[derive(Clone, Debug)]
pub(crate) struct SettingsFormSection<'a> {
    pub(crate) title: Cow<'a, str>,
    pub(crate) focused: bool,
    pub(crate) constraint: Constraint,
}

impl<'a> SettingsFormSection<'a> {
    pub(crate) fn new(
        title: impl Into<Cow<'a, str>>,
        focused: bool,
        constraint: Constraint,
    ) -> Self {
        Self {
            title: title.into(),
            focused,
            constraint,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsFormSectionLayout {
    pub(crate) outer: Rect,
    pub(crate) inner: Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsFormPageLayout {
    pub(crate) page: SettingsActionPageLayout,
    pub(crate) sections: Vec<SettingsFormSectionLayout>,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsFormPage<'a> {
    page: SettingsActionPage<'a>,
    sections: Vec<SettingsFormSection<'a>>,
    section_gap_rows: usize,
}

impl<'a> SettingsFormPage<'a> {
    pub(crate) fn new(page: SettingsActionPage<'a>, sections: Vec<SettingsFormSection<'a>>) -> Self {
        Self {
            page,
            sections,
            section_gap_rows: 0,
        }
    }

    pub(crate) fn with_section_gap_rows(mut self, section_gap_rows: usize) -> Self {
        self.section_gap_rows = section_gap_rows;
        self
    }

    fn section_rects(&self, body: Rect) -> Vec<Rect> {
        if self.sections.is_empty() || body.width == 0 || body.height == 0 {
            return Vec::new();
        }

        let mut constraints =
            Vec::with_capacity(self.sections.len() + self.sections.len().saturating_sub(1));
        for (idx, section) in self.sections.iter().enumerate() {
            constraints.push(section.constraint);
            if idx + 1 < self.sections.len() && self.section_gap_rows > 0 {
                constraints.push(Constraint::Length(self.section_gap_rows as u16));
            }
        }

        let split = Layout::vertical(constraints).split(body);
        split
            .iter()
            .enumerate()
            .filter_map(|(idx, rect)| {
                if self.section_gap_rows > 0 {
                    (idx % 2 == 0).then_some(*rect)
                } else {
                    Some(*rect)
                }
            })
            .collect()
    }

    fn layout_from_page(&self, page: SettingsActionPageLayout) -> SettingsFormPageLayout {
        let section_rects = self.section_rects(page.body);
        let sections = self
            .sections
            .iter()
            .zip(section_rects)
            .map(|(section, outer)| {
                let field = BorderedField::new(section.title.clone(), section.focused);
                SettingsFormSectionLayout {
                    outer,
                    inner: field.inner(outer),
                }
            })
            .collect();
        SettingsFormPageLayout { page, sections }
    }

    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsFormPageLayout> {
        let page = self.page.framed().layout(area)?;
        Some(self.layout_from_page(page))
    }

    pub(crate) fn layout_content(&self, area: Rect) -> Option<SettingsFormPageLayout> {
        let page = self.page.content_only().layout(area)?;
        Some(self.layout_from_page(page))
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
    ) -> Option<SettingsFormPageLayout> {
        let page = self.page.framed().render_shell(area, buf)?;
        let layout = self.layout_from_page(page);
        debug_assert_eq!(fields.len(), self.sections.len());
        for ((section, field), section_layout) in self
            .sections
            .iter()
            .zip(fields.iter())
            .zip(layout.sections.iter())
        {
            let bordered = BorderedField::new(section.title.clone(), section.focused);
            let _ = bordered.render(section_layout.outer, buf, field);
        }
        Some(layout)
    }

    pub(crate) fn render_content(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
    ) -> Option<SettingsFormPageLayout> {
        let page = self.page.content_only().render_shell(area, buf)?;
        let layout = self.layout_from_page(page);
        debug_assert_eq!(fields.len(), self.sections.len());
        for ((section, field), section_layout) in self
            .sections
            .iter()
            .zip(fields.iter())
            .zip(layout.sections.iter())
        {
            let bordered = BorderedField::new(section.title.clone(), section.focused);
            let _ = bordered.render(section_layout.outer, buf, field);
        }
        Some(layout)
    }

    pub(crate) fn field_index_at(
        &self,
        layout: &SettingsFormPageLayout,
        x: u16,
        y: u16,
    ) -> Option<usize> {
        let pos = Position { x, y };
        layout
            .sections
            .iter()
            .position(|section| section.outer.contains(pos))
    }

    pub(crate) fn render_standard_actions<Id: Copy>(
        &self,
        layout: &SettingsFormPageLayout,
        buf: &mut Buffer,
        buttons: &[StandardButtonSpec<Id>],
        align: TextButtonAlign,
    ) {
        self.page
            .render_standard_actions(&layout.page, buf, buttons, align);
    }

    pub(crate) fn render_with_standard_actions<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
        buttons: &[StandardButtonSpec<Id>],
        align: TextButtonAlign,
    ) -> Option<SettingsFormPageLayout> {
        let layout = self.render(area, buf, fields)?;
        self.render_standard_actions(&layout, buf, buttons, align);
        Some(layout)
    }

    pub(crate) fn render_content_with_standard_actions<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
        buttons: &[StandardButtonSpec<Id>],
        align: TextButtonAlign,
    ) -> Option<SettingsFormPageLayout> {
        let layout = self.render_content(area, buf, fields)?;
        self.render_standard_actions(&layout, buf, buttons, align);
        Some(layout)
    }

    pub(crate) fn render_with_standard_actions_end<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
        buttons: &[StandardButtonSpec<Id>],
    ) -> Option<SettingsFormPageLayout> {
        self.render_with_standard_actions(area, buf, fields, buttons, TextButtonAlign::End)
    }

    pub(crate) fn render_content_with_standard_actions_end<Id: Copy>(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
        buttons: &[StandardButtonSpec<Id>],
    ) -> Option<SettingsFormPageLayout> {
        self.render_content_with_standard_actions(area, buf, fields, buttons, TextButtonAlign::End)
    }

    pub(crate) fn standard_action_at<Id: Copy>(
        &self,
        layout: &SettingsFormPageLayout,
        x: u16,
        y: u16,
        buttons: &[StandardButtonSpec<Id>],
        align: TextButtonAlign,
    ) -> Option<Id> {
        self.page
            .standard_action_at(&layout.page, x, y, buttons, align)
    }

    pub(crate) fn standard_action_at_end<Id: Copy>(
        &self,
        layout: &SettingsFormPageLayout,
        x: u16,
        y: u16,
        buttons: &[StandardButtonSpec<Id>],
    ) -> Option<Id> {
        self.standard_action_at(layout, x, y, buttons, TextButtonAlign::End)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Line;

    use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;

    #[test]
    fn layout_places_sections_in_order_with_gap() {
        let page = SettingsActionPage::new(
            "Test",
            SettingsPanelStyle::bottom_pane(),
            vec![Line::from("header")],
            vec![],
        );
        let form = SettingsFormPage::new(
            page,
            vec![
                SettingsFormSection::new("One", false, Constraint::Length(3)),
                SettingsFormSection::new("Two", true, Constraint::Min(1)),
            ],
        )
        .with_section_gap_rows(1);
        let layout = form.layout(Rect::new(0, 0, 24, 9)).expect("layout");
        assert_eq!(layout.sections.len(), 2);
        assert_eq!(layout.sections[0].outer, Rect::new(1, 2, 22, 3));
        assert_eq!(layout.sections[1].outer, Rect::new(1, 6, 22, 1));
    }

    #[test]
    fn render_and_layout_agree_on_section_rects() {
        let page = SettingsActionPage::new("Test", SettingsPanelStyle::bottom_pane(), vec![], vec![]);
        let form = SettingsFormPage::new(
            page,
            vec![SettingsFormSection::new("Body", true, Constraint::Min(1))],
        );
        let area = Rect::new(0, 0, 24, 7);
        let expected = form.layout(area).expect("layout");
        let field = FormTextField::new_multi_line();
        let mut buf = Buffer::empty(area);
        let rendered = form.render(area, &mut buf, &[&field]).expect("render");
        assert_eq!(rendered.sections, expected.sections);
        assert_eq!(rendered.page, expected.page);
    }
}
