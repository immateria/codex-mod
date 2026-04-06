use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::buttons::{standard_button_specs, SettingsButtonKind, StandardButtonSpec};
use crate::bottom_pane::settings_ui::hints::{hint_enter, hint_esc, shortcut_line, title_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;

impl SecretsSettingsView {
    pub(super) fn list_page(&self, snapshot: &SecretsSharedState) -> SettingsMenuPage<'static> {
        let mut header_lines = vec![
            Line::from(Span::styled(
                "Secrets are stored locally in CODE_HOME (encrypted at rest).",
                Style::new().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Use `code secrets set NAME` to add. Values are never shown in the TUI.",
                Style::new().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                format!("Environment scope: {}", self.env_id),
                Style::new().fg(colors::text_dim()),
            )),
        ];

        match &snapshot.list {
            crate::chatwidget::SecretsListState::Uninitialized => {
                header_lines.push(Line::from(Span::styled(
                    "Loading secrets...".to_string(),
                    Style::new().fg(colors::function()),
                )));
            }
            crate::chatwidget::SecretsListState::Loading { .. } => {
                header_lines.push(Line::from(Span::styled(
                    "Loading secrets...".to_string(),
                    Style::new().fg(colors::function()),
                )));
            }
            crate::chatwidget::SecretsListState::Failed { error, .. } => {
                header_lines.push(Line::from(Span::styled(
                    format!("Error: {error}"),
                    Style::new().fg(colors::error()),
                )));
            }
            crate::chatwidget::SecretsListState::Ready { entries, .. } => {
                let mut env_count = 0usize;
                let mut global_count = 0usize;
                for entry in entries {
                    match entry.scope {
                        code_secrets::SecretScope::Environment(_) => env_count = env_count.saturating_add(1),
                        code_secrets::SecretScope::Global => global_count = global_count.saturating_add(1),
                    }
                }
                header_lines.push(Line::from(Span::styled(
                    format!("Secrets: repo {env_count} · global {global_count}"),
                    Style::new().fg(colors::text_dim()),
                )));
            }
        }

        if let Some(error) = snapshot.action_error.as_ref() {
            header_lines.push(Line::from(Span::styled(
                error.clone(),
                Style::new().fg(colors::error()),
            )));
        } else if let Some(action) = snapshot.action_in_progress.as_ref() {
            let label = match action {
                crate::chatwidget::SecretsActionInProgress::FetchList => "Loading secrets...".to_string(),
                crate::chatwidget::SecretsActionInProgress::Delete { entry, .. } => {
                    format!("Deleting {}...", entry.name.as_str())
                }
            };
            header_lines.push(Line::from(Span::styled(label, Style::new().fg(colors::function()))));
        }

        let deleting = matches!(
            snapshot.action_in_progress,
            Some(crate::chatwidget::SecretsActionInProgress::Delete { .. })
        );

        SettingsMenuPage::new(
            "Secrets",
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            vec![shortcut_line(&self.list_shortcuts(deleting))],
        )
    }

    fn list_shortcuts(&self, deleting: bool) -> Vec<KeyHint<'static>> {
        let mut hints = vec![
            KeyHint::new("↑↓", " select").with_key_style(Style::new().fg(colors::primary())),
            KeyHint::new("r", " refresh").with_key_style(Style::new().fg(colors::info())),
            hint_esc(" back"),
        ];

        if !deleting {
            hints.insert(
                1,
                KeyHint::new("Del", " delete").with_key_style(Style::new().fg(colors::error())),
            );
        }

        hints
    }

    pub(super) fn list_rows(&self, snapshot: &SecretsSharedState) -> Vec<SettingsMenuRow<'static, usize>> {
        let deleting = matches!(
            snapshot.action_in_progress,
            Some(crate::chatwidget::SecretsActionInProgress::Delete { .. })
        );

        let entries = match &snapshot.list {
            crate::chatwidget::SecretsListState::Uninitialized
            | crate::chatwidget::SecretsListState::Loading { .. } => {
                return vec![SettingsMenuRow::new(0, "Loading secrets...").disabled()];
            }
            crate::chatwidget::SecretsListState::Failed { .. } => {
                return vec![
                    SettingsMenuRow::new(0, "Failed to load secrets (press r to retry)").disabled(),
                ];
            }
            crate::chatwidget::SecretsListState::Ready { entries, .. } => entries,
        };

        if entries.is_empty() {
            return vec![SettingsMenuRow::new(0, "No secrets stored").disabled()];
        }

        entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let scope_label = match entry.scope {
                    code_secrets::SecretScope::Environment(_) => "env",
                    code_secrets::SecretScope::Global => "global",
                };
                let value_style = match entry.scope {
                    code_secrets::SecretScope::Environment(_) => Style::new().fg(colors::info()),
                    code_secrets::SecretScope::Global => Style::new().fg(colors::text_dim()),
                };
                let mut row = SettingsMenuRow::new(idx, entry.name.as_str().to_string())
                    .with_value(StyledText::new(scope_label.to_string(), value_style))
                    .with_selected_hint("Del delete");
                if deleting {
                    row = row.disabled();
                }
                row
            })
            .collect()
    }

    pub(super) fn confirm_delete_page(
        &self,
        snapshot: &SecretsSharedState,
    ) -> SettingsActionPage<'static> {
        let status = snapshot.action_error.as_ref().map(|err| {
            StyledText::new(err.clone(), Style::new().fg(colors::error()))
        });

        let shortcuts = [
            KeyHint::new("←→", " actions").with_key_style(Style::new().fg(colors::function())),
            hint_enter(" activate"),
            hint_esc(" back"),
        ];

        let (status_lines, footer_lines) =
            crate::bottom_pane::settings_ui::hints::status_and_shortcuts_split(status, &shortcuts);

        SettingsActionPage::new(
            "Secrets",
            SettingsPanelStyle::bottom_pane(),
            vec![title_line("Confirm delete".to_string())],
            footer_lines,
        )
        .with_status_lines(status_lines)
        .with_wrap_lines(true)
        .with_min_body_rows(4)
        .with_action_rows(1)
    }

    pub(super) fn confirm_delete_button_specs(&self) -> Vec<StandardButtonSpec<ConfirmAction>> {
        standard_button_specs(
            &[
                (ConfirmAction::Delete, SettingsButtonKind::Delete),
                (ConfirmAction::Cancel, SettingsButtonKind::Cancel),
            ],
            Some(self.focused_confirm_button),
            self.hovered_confirm_button,
        )
    }
}
