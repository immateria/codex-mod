use super::*;
use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::buttons::{
    standard_button_specs, SettingsButtonKind, StandardButtonSpec,
};
use crate::bottom_pane::settings_ui::form_page::{
    SettingsFormPage,
    SettingsFormPageLayout,
    SettingsFormSection,
};
use crate::bottom_pane::settings_ui::hints::{
    status_or_shortcuts_line, title_line, KeyHint,
};
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use ratatui::layout::Constraint;

impl ShellProfilesSettingsView {
    pub(super) fn open_editor(&mut self, target: ListTarget) {
        let before = match target {
            ListTarget::Summary => self.summary_field.text().to_string(),
            ListTarget::References => self.references_field.text().to_string(),
            ListTarget::SkillRoots => self.skill_roots_field.text().to_string(),
        };
        self.mode = ViewMode::EditList { target, before };
    }

    pub(super) fn editor_field_mut(&mut self, target: ListTarget) -> &mut FormTextField {
        match target {
            ListTarget::Summary => &mut self.summary_field,
            ListTarget::References => &mut self.references_field,
            ListTarget::SkillRoots => &mut self.skill_roots_field,
        }
    }

    fn editor_title(target: ListTarget) -> &'static str {
        match target {
            ListTarget::Summary => "Edit summary",
            ListTarget::References => "Edit references",
            ListTarget::SkillRoots => "Edit skill roots",
        }
    }

    fn editor_field_title(target: ListTarget) -> &'static str {
        match target {
            ListTarget::Summary => "Summary (optional)",
            ListTarget::References => "References (one path per line)",
            ListTarget::SkillRoots => "Skill roots (one path per line)",
        }
    }

    fn editor_status_line(&self, target: ListTarget) -> Line<'static> {
        let status = self.status.as_deref().and_then(|status| {
            let trimmed = status.trim().replace(['\r', '\n'], " ");
            (!trimmed.is_empty())
                .then(|| crate::bottom_pane::settings_ui::rows::StyledText::new(
                    trimmed,
                    Style::new().fg(crate::colors::text_dim()),
                ))
        });
        let hints = match target {
            ListTarget::Summary => vec![
                KeyHint::new("Ctrl+S", " save"),
                KeyHint::new("Ctrl+G", " generate"),
                KeyHint::new("Esc", " cancel"),
            ],
            ListTarget::References | ListTarget::SkillRoots => vec![
                KeyHint::new("Ctrl+S", " save"),
                KeyHint::new("Ctrl+O", " pick"),
                KeyHint::new("Ctrl+V", " show"),
                KeyHint::new("Esc", " cancel"),
            ],
        };
        status_or_shortcuts_line(status, &hints)
    }

    fn editor_page(&self, target: ListTarget) -> SettingsActionPage<'static> {
        SettingsActionPage::new(
            "Shell Profiles",
            SettingsPanelStyle::bottom_pane(),
            vec![title_line(Self::editor_title(target))],
            vec![self.editor_status_line(target)],
        )
    }

    fn editor_form_page(&self, target: ListTarget) -> SettingsFormPage<'static> {
        SettingsFormPage::new(
            self.editor_page(target),
            vec![SettingsFormSection::new(
                Self::editor_field_title(target),
                true,
                Constraint::Min(1),
            )],
        )
    }

    fn editor_footer_actions(target: ListTarget) -> &'static [EditorFooterAction] {
        match target {
            ListTarget::Summary => &[
                EditorFooterAction::Save,
                EditorFooterAction::Generate,
                EditorFooterAction::Cancel,
            ],
            ListTarget::References | ListTarget::SkillRoots => &[
                EditorFooterAction::Save,
                EditorFooterAction::Pick,
                EditorFooterAction::Show,
                EditorFooterAction::Cancel,
            ],
        }
    }

    fn footer_action_kind(action: EditorFooterAction) -> SettingsButtonKind {
        match action {
            EditorFooterAction::Save => SettingsButtonKind::Save,
            EditorFooterAction::Generate => SettingsButtonKind::Generate,
            EditorFooterAction::Pick => SettingsButtonKind::Pick,
            EditorFooterAction::Show => SettingsButtonKind::Show,
            EditorFooterAction::Cancel => SettingsButtonKind::Cancel,
        }
    }

    pub(super) fn editor_footer_action_at(
        &self,
        target: ListTarget,
        x: u16,
        y: u16,
        layout: &SettingsFormPageLayout,
    ) -> Option<EditorFooterAction> {
        let page = self.editor_form_page(target);
        let actions = Self::editor_footer_button_specs(target, None);
        page.standard_action_at_end(layout, x, y, &actions)
    }

    pub(super) fn compute_editor_layout(
        &self,
        area: Rect,
        target: ListTarget,
    ) -> Option<SettingsFormPageLayout> {
        self.editor_form_page(target).layout(area)
    }

    pub(super) fn editor_append_picker_path(&mut self, target: ListTarget) {
        let kind = match target {
            ListTarget::Summary => {
                self.status = Some("Picker not available for summary".to_string());
                return;
            }
            ListTarget::References => NativePickerKind::File,
            ListTarget::SkillRoots => NativePickerKind::Folder,
        };
        let title = match target {
            ListTarget::Summary => "Select path",
            ListTarget::References => "Select reference file",
            ListTarget::SkillRoots => "Select skill root folder",
        };

        match pick_path(kind, title) {
            Ok(Some(path)) => {
                let entry = path.to_string_lossy();
                let entry = entry.trim();
                if !entry.is_empty() {
                    let field = self.editor_field_mut(target);
                    let mut next = field.text().to_string();
                    if !next.trim().is_empty() && !next.ends_with('\n') {
                        next.push('\n');
                    }
                    next.push_str(entry);
                    field.set_text(&next);
                    self.status = Some("Added path (not staged)".to_string());
                }
            }
            Ok(None) => {}
            Err(err) => {
                self.status = Some(format!("Native picker failed: {err:#}"));
            }
        }
    }

    pub(super) fn editor_show_last_path(&mut self, target: ListTarget) {
        let text = self.editor_field_mut(target).text().to_string();
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        let last = lines.next_back();

        match last {
            Some(path) => match crate::native_file_manager::reveal_path(std::path::Path::new(path)) {
                Ok(()) => self.status = Some("Opened in file manager".to_string()),
                Err(err) => self.status = Some(format!("Open failed: {err:#}")),
            },
            None => self.status = Some("No paths to show".to_string()),
        }
    }

    pub(super) fn render_editor(&self, area: Rect, buf: &mut Buffer, target: ListTarget) {
        let page = self.editor_form_page(target);
        let field = match target {
            ListTarget::Summary => &self.summary_field,
            ListTarget::References => &self.references_field,
            ListTarget::SkillRoots => &self.skill_roots_field,
        };
        let buttons = Self::editor_footer_button_specs(target, None);
        let Some(_layout) = page.render_with_standard_actions_end(area, buf, &[field], &buttons)
        else {
            return;
        };
    }

    fn editor_footer_button_specs(
        target: ListTarget,
        focused: Option<EditorFooterAction>,
    ) -> Vec<StandardButtonSpec<EditorFooterAction>> {
        let items = Self::editor_footer_actions(target)
            .iter()
            .map(|action| (*action, Self::footer_action_kind(*action)))
            .collect::<Vec<_>>();
        standard_button_specs(&items, focused, None)
    }
}
