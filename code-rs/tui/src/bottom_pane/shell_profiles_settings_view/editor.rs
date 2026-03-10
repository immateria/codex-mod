use super::*;
use crate::bottom_pane::settings_ui::buttons::{render_text_button_strip, text_button_at, TextButton};
use crate::bottom_pane::settings_ui::fields::BorderedField;
use crate::bottom_pane::settings_ui::frame::{SettingsFrame, SettingsFrameLayout};
use crate::bottom_pane::settings_ui::layout::DEFAULT_BUTTON_GAP;

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
        let text = if let Some(status) = self.status.as_deref()
            && !status.trim().is_empty()
        {
            status.trim().replace(['\r', '\n'], " ")
        } else {
            match target {
                ListTarget::Summary => "Ctrl+S save  •  Ctrl+G generate  •  Esc cancel".to_string(),
                ListTarget::References | ListTarget::SkillRoots => {
                    "Ctrl+S save  •  Ctrl+O pick  •  Ctrl+V show  •  Esc cancel".to_string()
                }
            }
        };
        Line::from(Span::styled(
            text,
            Style::default().fg(crate::colors::text_dim()),
        ))
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

    fn footer_action_label(action: EditorFooterAction) -> &'static str {
        match action {
            EditorFooterAction::Save => "Save",
            EditorFooterAction::Generate => "Generate",
            EditorFooterAction::Pick => "Pick",
            EditorFooterAction::Show => "Show",
            EditorFooterAction::Cancel => "Cancel",
        }
    }

    fn footer_action_style(action: EditorFooterAction) -> Style {
        match action {
            EditorFooterAction::Save => {
                Style::default().fg(crate::colors::success()).add_modifier(Modifier::BOLD)
            }
            EditorFooterAction::Generate => {
                Style::default().fg(crate::colors::function()).add_modifier(Modifier::BOLD)
            }
            EditorFooterAction::Pick | EditorFooterAction::Show => {
                Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD)
            }
            EditorFooterAction::Cancel => {
                Style::default().fg(crate::colors::text_dim()).add_modifier(Modifier::BOLD)
            }
        }
    }

    fn editor_button_rect(target: ListTarget, footer: Rect) -> Rect {
        let labels: Vec<&str> = Self::editor_footer_actions(target)
            .iter()
            .map(|action| Self::footer_action_label(*action))
            .collect();
        let content_width: u16 = labels
            .iter()
            .enumerate()
            .map(|(idx, label)| {
                let width = u16::try_from(unicode_width::UnicodeWidthStr::width(*label))
                    .unwrap_or(u16::MAX);
                if idx + 1 < labels.len() {
                    width.saturating_add(DEFAULT_BUTTON_GAP.len() as u16)
                } else {
                    width
                }
            })
            .fold(0, u16::saturating_add)
            .min(footer.width);
        Rect::new(
            footer.x.saturating_add(footer.width.saturating_sub(content_width)),
            footer.y,
            content_width,
            footer.height,
        )
    }

    pub(super) fn editor_footer_action_at(
        target: ListTarget,
        x: u16,
        y: u16,
        footer: Rect,
    ) -> Option<EditorFooterAction> {
        let actions = Self::editor_footer_buttons(target, None);
        let buttons_rect = Self::editor_button_rect(target, footer);
        text_button_at(x, y, buttons_rect, &actions)
    }

    pub(super) fn compute_editor_layout(
        area: Rect,
        target: ListTarget,
    ) -> Option<(SettingsFrameLayout, Rect)> {
        let header_lines = vec![
            Line::from(Span::styled(
                Self::editor_title(target),
                Style::default()
                    .fg(crate::colors::text_bright())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];
        let layout = SettingsFrame::new("Shell Profiles", header_lines, vec![Line::from("")])
            .layout(area)?;
        let block = BorderedField::new(Self::editor_field_title(target), true);
        let field_inner = block.inner(layout.body);
        Some((layout, field_inner))
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
        let header_lines = vec![Line::from(Span::styled(
            Self::editor_title(target),
            Style::default()
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD),
        )), self.editor_status_line(target)];
        let Some(layout) = SettingsFrame::new("Shell Profiles", header_lines, vec![Line::from("")])
            .render(area, buf)
        else {
            return;
        };

        let focused = true;
        let block = BorderedField::new(Self::editor_field_title(target), focused);
        match target {
            ListTarget::Summary => {
                let _ = block.render(layout.body, buf, &self.summary_field);
            }
            ListTarget::References => {
                let _ = block.render(layout.body, buf, &self.references_field);
            }
            ListTarget::SkillRoots => {
                let _ = block.render(layout.body, buf, &self.skill_roots_field);
            }
        }

        let buttons_rect = Self::editor_button_rect(target, layout.footer);
        let buttons = Self::editor_footer_buttons(target, None);
        render_text_button_strip(buttons_rect, buf, &buttons);
    }

    fn editor_footer_buttons(
        target: ListTarget,
        focused: Option<EditorFooterAction>,
    ) -> Vec<TextButton<'static, EditorFooterAction>> {
        Self::editor_footer_actions(target)
            .iter()
            .map(|action| {
                TextButton::new(
                    *action,
                    Self::footer_action_label(*action),
                    focused == Some(*action),
                    false,
                    Self::footer_action_style(*action),
                )
            })
            .collect()
    }
}
