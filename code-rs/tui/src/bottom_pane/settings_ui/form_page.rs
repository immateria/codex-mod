use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};

use crate::bottom_pane::chrome::ChromeMode;
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

#[derive(Clone, Debug, PartialEq, Eq)]
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

pub(crate) struct SettingsFormPageFramed<'p, 'a> {
    page: &'p SettingsFormPage<'a>,
}

pub(crate) struct SettingsFormPageContentOnly<'p, 'a> {
    page: &'p SettingsFormPage<'a>,
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

    pub(crate) fn framed(&self) -> SettingsFormPageFramed<'_, 'a> {
        SettingsFormPageFramed { page: self }
    }

    pub(crate) fn content_only(&self) -> SettingsFormPageContentOnly<'_, 'a> {
        SettingsFormPageContentOnly { page: self }
    }

    pub(crate) fn layout_in_chrome(&self, chrome: ChromeMode, area: Rect) -> Option<SettingsFormPageLayout> {
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
        fields: &[&FormTextField],
    ) -> Option<SettingsFormPageLayout> {
        match chrome {
            ChromeMode::Framed => self.framed().render(area, buf, fields),
            ChromeMode::ContentOnly => self.content_only().render(area, buf, fields),
        }
    }

    pub(crate) fn render_with_standard_actions_end_in_chrome<Id: Copy>(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
        buttons: &[StandardButtonSpec<Id>],
    ) -> Option<SettingsFormPageLayout> {
        let layout = self.render_in_chrome(chrome, area, buf, fields)?;
        self.render_standard_actions(&layout, buf, buttons, TextButtonAlign::End);
        Some(layout)
    }

    fn required_min_body_rows(&self) -> usize {
        if self.sections.is_empty() {
            return 1;
        }

        let mut rows = 0usize;
        for (idx, section) in self.sections.iter().enumerate() {
            let min_rows = match section.constraint {
                Constraint::Length(n) => n as usize,
                Constraint::Min(n) => n as usize,
                Constraint::Max(_) => 1,
                Constraint::Percentage(_) => 1,
                Constraint::Ratio(_, _) => 1,
                Constraint::Fill(_) => 1,
            }
            .max(1);

            rows = rows.saturating_add(min_rows);
            if idx + 1 < self.sections.len() {
                rows = rows.saturating_add(self.section_gap_rows);
            }
        }
        rows.max(1)
    }

    fn page_with_min_rows(&self) -> SettingsActionPage<'a> {
        let required = self.required_min_body_rows();
        self.page
            .clone()
            .with_min_body_rows(self.page.min_body_rows().max(required))
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
                let gap_rows = u16::try_from(self.section_gap_rows).unwrap_or(u16::MAX);
                constraints.push(Constraint::Length(gap_rows));
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

    fn layout_framed(&self, area: Rect) -> Option<SettingsFormPageLayout> {
        let page = self.page_with_min_rows().framed().layout(area)?;
        Some(self.layout_from_page(page))
    }

    fn layout_content_only(&self, area: Rect) -> Option<SettingsFormPageLayout> {
        let page = self.page_with_min_rows().content_only().layout(area)?;
        Some(self.layout_from_page(page))
    }

    fn render_framed(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
    ) -> Option<SettingsFormPageLayout> {
        let page = self.page_with_min_rows().framed().render_shell(area, buf)?;
        let layout = self.layout_from_page(page);
        if fields.len() != self.sections.len() {
            debug_assert_eq!(fields.len(), self.sections.len());
            return None;
        }
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

    fn render_content_only(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
    ) -> Option<SettingsFormPageLayout> {
        let page = self.page_with_min_rows().content_only().render_shell(area, buf)?;
        let layout = self.layout_from_page(page);
        if fields.len() != self.sections.len() {
            debug_assert_eq!(fields.len(), self.sections.len());
            return None;
        }
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

impl<'p, 'a> SettingsFormPageFramed<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsFormPageLayout> {
        self.page.layout_framed(area)
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
    ) -> Option<SettingsFormPageLayout> {
        self.page.render_framed(area, buf, fields)
    }
}

impl<'p, 'a> SettingsFormPageContentOnly<'p, 'a> {
    pub(crate) fn layout(&self, area: Rect) -> Option<SettingsFormPageLayout> {
        self.page.layout_content_only(area)
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        fields: &[&FormTextField],
    ) -> Option<SettingsFormPageLayout> {
        self.page.render_content_only(area, buf, fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Line;

    use crate::bottom_pane::chrome::ChromeMode;
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
        let layout = form.framed().layout(Rect::new(0, 0, 24, 9)).expect("layout");
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
        let expected = form.framed().layout(area).expect("layout");
        let field = FormTextField::new_multi_line();
        let mut buf = Buffer::empty(area);
        let rendered = form.framed().render(area, &mut buf, &[&field]).expect("render");
        assert_eq!(rendered.sections, expected.sections);
        assert_eq!(rendered.page, expected.page);
    }

    #[test]
    fn content_only_render_and_layout_agree_on_section_rects() {
        let page = SettingsActionPage::new("Test", SettingsPanelStyle::bottom_pane(), vec![], vec![]);
        let form = SettingsFormPage::new(
            page,
            vec![SettingsFormSection::new("Body", true, Constraint::Min(1))],
        );
        let area = Rect::new(0, 0, 24, 7);
        let expected = form.content_only().layout(area).expect("layout");
        let field = FormTextField::new_multi_line();
        let mut buf = Buffer::empty(area);
        let rendered = form
            .content_only()
            .render(area, &mut buf, &[&field])
            .expect("render");
        assert_eq!(rendered.sections, expected.sections);
        assert_eq!(rendered.page, expected.page);
    }

    #[test]
    fn layout_in_chrome_matches_concrete_layout() {
        let page = SettingsActionPage::new("Test", SettingsPanelStyle::bottom_pane(), vec![], vec![]);
        let form = SettingsFormPage::new(
            page,
            vec![SettingsFormSection::new("Body", true, Constraint::Min(1))],
        );
        let area = Rect::new(0, 0, 24, 7);

        assert_eq!(
            form.layout_in_chrome(ChromeMode::Framed, area),
            form.framed().layout(area)
        );
        assert_eq!(
            form.layout_in_chrome(ChromeMode::ContentOnly, area),
            form.content_only().layout(area)
        );
    }
}
