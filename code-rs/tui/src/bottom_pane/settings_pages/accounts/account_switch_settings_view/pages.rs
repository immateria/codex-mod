use super::AccountSwitchSettingsView;

use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::line_runs::SelectableLineRun;
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::toggle;
use crate::colors;
use code_core::config_types::AuthCredentialsStoreMode;
use ratatui::layout::Margin;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

impl AccountSwitchSettingsView {
    pub(super) fn main_page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Accounts",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            Vec::new(),
            vec![shortcut_line(&[
                KeyHint::new("↑↓/Tab", " navigate")
                    .with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("Enter/Space", " activate")
                    .with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("Esc", " close").with_key_style(Style::new().fg(colors::error()).bold()),
            ])],
        )
    }

    pub(super) fn main_runs(
        &self,
        selected_id: Option<usize>,
    ) -> Vec<SelectableLineRun<'static, usize>> {
        let bool_value = |enabled: bool| toggle::checkbox_marker(enabled);

        let mut runs = Vec::new();

        let mut auto = SettingsMenuRow::new(0usize, "Auto-switch on rate/usage limit")
            .with_value(bool_value(self.auto_switch_enabled))
            .with_selected_hint("Enter to toggle")
            .into_run(selected_id);
        auto.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Switches to another connected account on 429/usage_limit.",
                Style::new().fg(colors::text_dim()),
            ),
        ]));
        runs.push(auto);

        let mut fallback = SettingsMenuRow::new(1usize, "API key fallback when all accounts limited")
            .with_value(bool_value(self.api_key_fallback_enabled))
            .with_selected_hint("Enter to toggle")
            .into_run(selected_id);
        fallback.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Only used if every connected ChatGPT account is limited.",
                Style::new().fg(colors::text_dim()),
            ),
        ]));
        runs.push(fallback);

        let store_mode = Self::auth_store_mode_label(self.auth_credentials_store_mode);
        let store_detail = match self.auth_credentials_store_mode {
            AuthCredentialsStoreMode::Ephemeral => {
                "In-memory only (will not persist across restarts)."
            }
            _ => "Where Code stores CLI auth credentials (auth.json payload).",
        };
        let mut store = SettingsMenuRow::new(2usize, "Credential store")
            .with_value(StyledText::new(
                format!("[{store_mode}]"),
                Style::new().fg(colors::primary()).bold(),
            ))
            .with_selected_hint("Enter to change")
            .into_run(selected_id);
        store.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(store_detail, Style::new().fg(colors::text_dim())),
        ]));
        runs.push(store);

        runs.push(SelectableLineRun::plain(vec![Line::from("")]));

        let mut manage = SettingsMenuRow::new(3usize, "Manage connected accounts")
            .with_selected_hint("Enter to open")
            .into_run(selected_id);
        manage.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "View, switch, and remove stored accounts.",
                Style::new().fg(colors::text_dim()),
            ),
        ]));
        runs.push(manage);

        let mut add = SettingsMenuRow::new(4usize, "Add account")
            .with_selected_hint("Enter to open")
            .into_run(selected_id);
        add.lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Start ChatGPT or API-key account setup.",
                Style::new().fg(colors::text_dim()),
            ),
        ]));
        runs.push(add);

        runs.push(SelectableLineRun::plain(vec![Line::from("")]));

        runs.push(
            SettingsMenuRow::new(5usize, "Close")
                .with_selected_hint("Enter to close")
                .into_run(selected_id),
        );

        runs
    }

    pub(super) fn confirm_page(&self, target: AuthCredentialsStoreMode) -> SettingsMenuPage<'static> {
        let current = Self::auth_store_mode_label(self.auth_credentials_store_mode);
        let next = Self::auth_store_mode_label(target);
        let header_lines = vec![
            Line::from(vec![
                Span::styled("Current: ", Style::new().fg(colors::text_dim())),
                Span::styled(current, Style::new().fg(colors::text())),
                Span::styled("   New: ", Style::new().fg(colors::text_dim())),
                Span::styled(next, Style::new().fg(colors::primary()).bold()),
            ]),
            Line::from(""),
        ];
        let footer_lines = vec![shortcut_line(&[
            KeyHint::new("↑↓/Tab", " select").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter/Space", " apply").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error()).bold()),
        ])];

        SettingsMenuPage::new(
            "Credential store",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            header_lines,
            footer_lines,
        )
    }

    pub(super) fn confirm_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        vec![
            SettingsMenuRow::new(0usize, "Apply + migrate existing credentials"),
            SettingsMenuRow::new(1usize, "Apply (do not migrate)  (may log you out)"),
            SettingsMenuRow::new(2usize, "Cancel"),
        ]
    }
}
