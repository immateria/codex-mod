use super::*;

use ratatui::layout::Constraint;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::buttons::{
    standard_button_specs,
    SettingsButtonKind,
    StandardButtonSpec,
};
use crate::bottom_pane::settings_ui::form_page::{
    SettingsFormPage,
    SettingsFormSection,
};
use crate::bottom_pane::settings_ui::hints::{
    hint_enter,
    hint_esc,
    hint_nav,
    shortcut_line,
    status_and_shortcuts_split,
    title_line,
    KeyHint,
};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

impl PromptsSettingsView {
    fn name_field_title(&self) -> &'static str {
        if matches!(self.focus, Focus::Name) {
            "Name (slug) • Enter to save"
        } else {
            "Name (slug)"
        }
    }

    fn body_field_title(&self) -> &'static str {
        if matches!(self.focus, Focus::Body) {
            "Content (multiline)"
        } else {
            "Content"
        }
    }

    fn edit_page(&self) -> SettingsActionPage<'static> {
        let status = self.status.as_ref().map(|(msg, style)| {
            crate::bottom_pane::settings_ui::rows::StyledText::new(msg.clone(), *style)
        });
        let (status_lines, footer_lines) = status_and_shortcuts_split(
            status,
            &[
                KeyHint::new("Tab", " next"),
                hint_enter(" activate"),
                hint_esc(" back"),
            ],
        );
        SettingsActionPage::new(
            "Custom Prompt",
            SettingsPanelStyle::bottom_pane(),
            vec![title_line(if self.selected_prompt_index().is_none() {
                "New prompt"
            } else {
                "Edit prompt"
            })],
            footer_lines,
        )
        .with_status_lines(status_lines)
    }

    pub(super) fn edit_form_page(&self) -> SettingsFormPage<'static> {
        SettingsFormPage::new(
            self.edit_page(),
            vec![
                SettingsFormSection::new(
                    self.name_field_title(),
                    matches!(self.focus, Focus::Name),
                    Constraint::Length(3),
                ),
                SettingsFormSection::new(
                    self.body_field_title(),
                    matches!(self.focus, Focus::Body),
                    Constraint::Min(6),
                ),
            ],
        )
    }

    fn list_header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Custom prompts allow you to save reusable prompts initiated with a simple slash command. They are invoked with /name. Create and update your custom prompts below.",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    pub(super) fn list_page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Custom Prompts",
            SettingsPanelStyle::bottom_pane(),
            self.list_header_lines(),
            vec![shortcut_line(&[
                hint_nav(" navigate"),
                hint_enter(" edit"),
                KeyHint::new("Ctrl+N", " new").with_key_style(Style::new().fg(colors::info())),
                hint_esc(" close"),
            ])],
        )
    }

    pub(super) fn list_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        let mut rows = self
            .prompts
            .iter()
            .enumerate()
            .map(|(idx, prompt)| {
                let preview = prompt
                    .content
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let name = prompt.name.as_str();
                let mut row = SettingsMenuRow::new(idx, format!("/{name}"));
                if !preview.is_empty() {
                    row = row.with_detail(crate::bottom_pane::settings_ui::rows::StyledText::new(
                        preview,
                        Style::new().fg(colors::text_dim()),
                    ));
                }
                row
            })
            .collect::<Vec<_>>();

        rows.push(
            SettingsMenuRow::new(self.prompts.len(), "Add new…").with_detail(
                crate::bottom_pane::settings_ui::rows::StyledText::new(
                    "Create a custom slash prompt",
                    Style::new().fg(colors::text_dim()),
                ),
            ),
        );
        rows
    }

    pub(super) fn edit_button_specs(&self) -> Vec<StandardButtonSpec<Focus>> {
        standard_button_specs(
            &[
                (Focus::Save, SettingsButtonKind::Save),
                (Focus::Delete, SettingsButtonKind::Delete),
                (Focus::Cancel, SettingsButtonKind::Cancel),
            ],
            match self.focus {
                Focus::Save | Focus::Delete | Focus::Cancel => Some(self.focus),
                _ => None,
            },
            None,
        )
    }
}
