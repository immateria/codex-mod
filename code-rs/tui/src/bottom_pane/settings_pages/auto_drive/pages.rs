use super::*;

use crate::colors;
use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::hints::{status_and_shortcuts, status_and_shortcuts_split, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::toggle;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

impl AutoDriveSettingsView {
    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        match &self.mode {
            AutoDriveSettingsMode::Main => SettingsMenuPage::new(
                Self::PANEL_TITLE,
                SettingsPanelStyle::bottom_pane(),
                Vec::new(),
                self.main_footer_lines(),
            ),
            AutoDriveSettingsMode::RoutingList => SettingsMenuPage::new(
                Self::PANEL_TITLE,
                SettingsPanelStyle::bottom_pane(),
                vec![Line::from(Span::styled(
                    "Routing entries",
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD),
                ))],
                self.routing_list_footer_lines(),
            ),
            AutoDriveSettingsMode::RoutingEditor(_) => {
                unreachable!("routing editor uses SettingsActionPage")
            }
        }
    }

    pub(super) fn routing_editor_page(
        &self,
        editor: &RoutingEditorState,
    ) -> SettingsActionPage<'static> {
        let title = if editor.index.is_some() {
            "Edit routing entry"
        } else {
            "Add routing entry"
        };
        let status = self.status_message.as_deref().map(|message| {
            crate::bottom_pane::settings_ui::rows::StyledText::new(
                message,
                Style::new().fg(colors::warning()),
            )
        });
        let (status_lines, footer_lines) = status_and_shortcuts_split(
            status,
            &[
                KeyHint::new("Tab", " next field"),
                KeyHint::new("Space", " toggle"),
                KeyHint::new("Enter", " save/activate"),
                KeyHint::new("Esc", " back"),
            ],
        );
        SettingsActionPage::new(
            Self::PANEL_TITLE,
            SettingsPanelStyle::bottom_pane(),
            vec![Line::from(Span::styled(
                title,
                Style::default()
                    .fg(colors::primary())
                    .add_modifier(Modifier::BOLD),
            ))],
            footer_lines,
        )
        .with_status_lines(status_lines)
        .with_action_rows(1)
        .with_min_body_rows(4)
    }

    pub(super) fn main_footer_lines(&self) -> Vec<Line<'static>> {
        status_and_shortcuts(
            self.status_message
                .as_deref()
                .map(|message| crate::bottom_pane::settings_ui::rows::StyledText::new(message, Style::new().fg(colors::warning()))),
            &[
                KeyHint::new("Enter", " select/toggle"),
                KeyHint::new("←/→", " adjust delay"),
                KeyHint::new("Esc", " close"),
                KeyHint::new("Ctrl+S", " close"),
            ],
        )
    }

    pub(super) fn routing_list_footer_lines(&self) -> Vec<Line<'static>> {
        status_and_shortcuts(
            self.status_message
                .as_deref()
                .map(|message| crate::bottom_pane::settings_ui::rows::StyledText::new(message, Style::new().fg(colors::warning()))),
            &[
                KeyHint::new("Enter", " edit/add"),
                KeyHint::new("Space", " toggle enabled"),
                KeyHint::new("D", " remove"),
                KeyHint::new("Esc", " back"),
            ],
        )
    }

    pub(super) fn main_menu_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        let model_value = if self.use_chat_model {
            "Follow Chat Mode".to_string()
        } else {
            let model_label = self.model.trim();
            if model_label.is_empty() {
                "(not set)".to_string()
            } else {
                format!(
                    "{} · {}",
                    Self::format_model_label(model_label),
                    Self::reasoning_label(self.model_reasoning)
                )
            }
        };
        vec![
            SettingsMenuRow::new(0, "Auto Drive model")
                .with_value(StyledText::new(model_value, Style::new().fg(colors::text_dim())))
                .with_selected_hint("Enter to change"),
            SettingsMenuRow::new(
                1,
                "Agents enabled (uses multiple agents to speed up complex tasks)",
            )
            .with_value(toggle::on_off_word(self.agents_enabled)),
            SettingsMenuRow::new(
                2,
                "Diagnostics enabled (monitors and adjusts system in real time)",
            )
            .with_value(toggle::on_off_word(self.diagnostics_enabled)),
            SettingsMenuRow::new(
                3,
                "Coordinator model routing (choose model + reasoning per turn)",
            )
            .with_value(toggle::enabled_word(self.model_routing_enabled)),
            SettingsMenuRow::new(4, "Routing entries (add/remove/edit per-model routes)")
                .with_value(StyledText::new(
                    format!(
                        "{}/{} enabled",
                        self.enabled_routing_entry_count(),
                        self.model_routing_entries.len()
                    ),
                    Style::new().fg(colors::text_dim()),
                ))
                .with_selected_hint("Enter to edit"),
            SettingsMenuRow::new(5, "Auto-continue delay").with_value(StyledText::new(
                self.continue_mode.label(),
                Style::new().fg(colors::text_dim()),
            )),
        ]
    }

    pub(super) fn routing_list_menu_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        let mut rows = self
            .model_routing_entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                SettingsMenuRow::new(idx, Self::route_entry_summary(entry))
                    .with_value(toggle::checkbox_marker(entry.enabled))
            })
            .collect::<Vec<_>>();
        rows.push(SettingsMenuRow::new(
            self.model_routing_entries.len(),
            "+ Add routing entry",
        ));
        rows
    }

    pub(super) fn routing_editor_menu_rows(
        &self,
        editor: &RoutingEditorState,
    ) -> Vec<SettingsMenuRow<'static, RoutingEditorField>> {
        let label_pad_cols = u16::try_from(
            ["Model", "Enabled", "Reasoning", "Description"]
                .iter()
                .map(|label| label.width())
                .max()
                .unwrap_or(0),
        )
        .unwrap_or(u16::MAX);

        let model = self
            .routing_model_options
            .get(editor.model_cursor)
            .cloned()
            .unwrap_or_else(Self::default_routing_model);
        let reasoning = ROUTING_REASONING_LEVELS
            .iter()
            .enumerate()
            .map(|(idx, level)| {
                let cursor = if editor.reasoning_cursor == idx { ">" } else { " " };
                let checkbox = toggle::checkbox_marker(editor.reasoning_enabled[idx]);
                format!(
                    "{cursor}{}{}",
                    checkbox.text,
                    Self::reasoning_label(*level).to_ascii_lowercase()
                )
            })
            .collect::<Vec<_>>()
            .join("  ");
        let description = if editor.description.trim().is_empty() {
            "(empty)".to_string()
        } else {
            editor.description.clone()
        };
        vec![
            SettingsMenuRow::new(RoutingEditorField::Model, "Model")
                .with_label_pad_cols(label_pad_cols)
                .with_value(StyledText::new(model, Style::new().fg(colors::text_dim())))
                .with_selected_hint("Enter/Space to cycle"),
            SettingsMenuRow::new(RoutingEditorField::Enabled, "Enabled")
                .with_label_pad_cols(label_pad_cols)
                .with_value(toggle::on_off_word(editor.enabled))
                .with_selected_hint("Enter/Space to toggle"),
            SettingsMenuRow::new(RoutingEditorField::Reasoning, "Reasoning")
                .with_label_pad_cols(label_pad_cols)
                .with_detail(StyledText::new(reasoning, Style::new().fg(colors::text_dim())))
                .with_selected_hint("←/→ move, Space toggle"),
            SettingsMenuRow::new(RoutingEditorField::Description, "Description")
                .with_label_pad_cols(label_pad_cols)
                .with_detail(StyledText::new(description, Style::new().fg(colors::text_dim()))),
        ]
    }

}
