use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;

impl AppsSettingsView {
    pub(super) fn shared_snapshot(&self) -> AppsSharedState {
        self.shared_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn mode_label(mode: AppsSourcesModeToml) -> &'static str {
        match mode {
            AppsSourcesModeToml::ActiveOnly => "active_only",
            AppsSourcesModeToml::ActivePlusPinned => "active_plus_pinned",
            AppsSourcesModeToml::PinnedOnly => "pinned_only",
        }
    }

    fn active_account_id(snapshot: &AppsSharedState) -> Option<&str> {
        snapshot
            .accounts_snapshot
            .iter()
            .find(|acc| acc.is_active_model_account && acc.is_chatgpt)
            .map(|acc| acc.id.as_str())
    }

    fn enabled_source_ids(&self, snapshot: &AppsSharedState) -> Vec<String> {
        code_core::apps_sources::effective_source_account_ids(
            &self.draft_sources,
            Self::active_account_id(snapshot),
        )
    }

    pub(super) fn overview_page(&self, snapshot: &AppsSharedState) -> SettingsMenuPage<'static> {
        let mut header_lines = vec![Line::from(Span::styled(
            "Pin connector-source accounts and view connected apps (multi-account connectors).",
            Style::new().fg(colors::text_dim()),
        ))];

        let profile_label = snapshot
            .active_profile
            .as_deref()
            .unwrap_or("default");
        header_lines.push(Line::from(Span::styled(
            format!("Profile: {profile_label}"),
            Style::new().fg(colors::text_dim()),
        )));

        let mode = Self::mode_label(self.draft_sources.mode);
        header_lines.push(Line::from(Span::styled(
            format!("Sources mode: {mode} (press `m` to cycle)"),
            Style::new().fg(colors::text_dim()),
        )));

        let enabled_ids = self.enabled_source_ids(snapshot);
        if enabled_ids.is_empty() {
            header_lines.push(Line::from(Span::styled(
                "Enabled sources this session: none".to_string(),
                Style::new().fg(colors::warning()),
            )));
        } else {
            let mut labels = Vec::new();
            for id in &enabled_ids {
                let label = snapshot
                    .accounts_snapshot
                    .iter()
                    .find(|acc| &acc.id == id)
                    .map(|acc| acc.label.as_str())
                    .unwrap_or(id.as_str());
                labels.push(label.to_string());
            }
            header_lines.push(Line::from(Span::styled(
                format!("Enabled sources this session: {}", labels.join(", ")),
                Style::new().fg(colors::text_dim()),
            )));
        }

        if self.sources_dirty {
            header_lines.push(Line::from(Span::styled(
                "Unsaved changes (Ctrl+S to save)".to_string(),
                Style::new().fg(colors::warning()),
            )));
        }

        if let Some(error) = snapshot.action_error.as_ref() {
            header_lines.push(Line::from(Span::styled(
                error.clone(),
                Style::new().fg(colors::error()),
            )));
        } else if let Some(action) = snapshot.action_in_progress.as_ref() {
            let label = match action {
                crate::chatwidget::AppsActionInProgress::SaveSources => "Saving sources...",
                crate::chatwidget::AppsActionInProgress::RefreshStatus { .. } => {
                    "Refreshing apps status..."
                }
            };
            header_lines.push(Line::from(Span::styled(
                label,
                Style::new().fg(colors::function()),
            )));
        }

        let shortcuts = vec![
            KeyHint::new("↑↓", " select").with_key_style(Style::new().fg(colors::primary())),
            KeyHint::new("Space", " pin").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Enter", " details").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("m", " mode").with_key_style(Style::new().fg(colors::primary())),
            KeyHint::new("r", " refresh").with_key_style(Style::new().fg(colors::info())),
            KeyHint::new("a", " accounts").with_key_style(Style::new().fg(colors::primary())),
            KeyHint::new("l", " login").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Ctrl+S", " save").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " close").with_key_style(Style::new().fg(colors::error())),
        ];

        SettingsMenuPage::new(
            "Apps",
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            vec![shortcut_line(&shortcuts)],
        )
    }

    pub(super) fn overview_rows(
        &self,
        snapshot: &AppsSharedState,
    ) -> Vec<SettingsMenuRow<'static, usize>> {
        if snapshot.accounts_snapshot.is_empty() {
            return vec![SettingsMenuRow::new(0usize, "No accounts found")
                .with_detail(StyledText::new(
                    "Press `a` to open Accounts settings.".to_string(),
                    Style::new().fg(colors::text_dim()),
                ))
                .disabled()];
        }

        let enabled = self.enabled_source_ids(snapshot);
        let enabled_lookup = |id: &str| enabled.iter().any(|item| item == id);
        let pinned_lookup = |id: &str| self.draft_sources.pinned_account_ids.iter().any(|item| item == id);

        snapshot
            .accounts_snapshot
            .iter()
            .enumerate()
            .map(|(idx, account)| {
                let mut label = account.label.clone();
                if account.is_active_model_account {
                    label = format!("{label} [model]");
                }

                let mut tags = Vec::new();
                if enabled_lookup(&account.id) {
                    tags.push("enabled");
                }
                if pinned_lookup(&account.id) {
                    tags.push("pinned");
                }
                let value = if tags.is_empty() {
                    None
                } else {
                    Some(StyledText::new(
                        tags.join(" · "),
                        Style::new().fg(colors::success()),
                    ))
                };

                let detail = match snapshot.status_by_account_id.get(&account.id) {
                    Some(crate::chatwidget::AppsAccountStatusState::Loading) => Some(StyledText::new(
                        "loading...".to_string(),
                        Style::new().fg(colors::function()),
                    )),
                    Some(crate::chatwidget::AppsAccountStatusState::Ready { connected_apps, .. }) => {
                        Some(StyledText::new(
                            format!("apps: {}", connected_apps.len()),
                            Style::new().fg(colors::info()),
                        ))
                    }
                    Some(crate::chatwidget::AppsAccountStatusState::Failed { error, .. }) => Some(StyledText::new(
                        format!("error: {error}"),
                        Style::new().fg(colors::warning()),
                    )),
                    _ => None,
                };

                let mut row = SettingsMenuRow::new(idx, label);
                if let Some(value) = value {
                    row = row.with_value(value);
                }
                if let Some(detail) = detail {
                    row = row.with_detail(detail);
                }

                if !account.is_chatgpt {
                    row = row
                        .with_detail(StyledText::new(
                            "not a ChatGPT account".to_string(),
                            Style::new().fg(colors::text_dim()),
                        ))
                        .disabled();
                }
                row
            })
            .collect()
    }

    pub(super) fn account_detail_page(
        &self,
        snapshot: &AppsSharedState,
        account_id: &str,
    ) -> SettingsMenuPage<'static> {
        let label = snapshot
            .accounts_snapshot
            .iter()
            .find(|acc| acc.id == account_id)
            .map(|acc| acc.label.as_str())
            .unwrap_or(account_id);

        let mut header_lines = vec![
            Line::from(Span::styled(
                "Connected apps are derived from MCP tool annotations for this source account.",
                Style::new().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                format!("Account: {label}"),
                Style::new().fg(colors::text_dim()),
            )),
        ];

        if let Some(status) = snapshot.status_by_account_id.get(account_id) {
            match status {
                crate::chatwidget::AppsAccountStatusState::Loading => {
                    header_lines.push(Line::from(Span::styled(
                        "Loading...".to_string(),
                        Style::new().fg(colors::function()),
                    )));
                }
                crate::chatwidget::AppsAccountStatusState::Ready {
                    connected_apps,
                    last_refresh,
                } => {
                    header_lines.push(Line::from(Span::styled(
                        format!("Connected apps: {}", connected_apps.len()),
                        Style::new().fg(colors::info()),
                    )));
                    header_lines.push(Line::from(Span::styled(
                        format!("Last refreshed: {}", last_refresh.format("%Y-%m-%d %H:%M:%S UTC")),
                        Style::new().fg(colors::text_dim()),
                    )));
                }
                crate::chatwidget::AppsAccountStatusState::Failed { error, needs_login } => {
                    header_lines.push(Line::from(Span::styled(
                        format!("Error: {error}"),
                        Style::new().fg(colors::warning()),
                    )));
                    if *needs_login {
                        header_lines.push(Line::from(Span::styled(
                            "Action: press `l` to log in, or `a` for Accounts settings."
                                .to_string(),
                            Style::new().fg(colors::text_dim()),
                        )));
                    }
                }
                crate::chatwidget::AppsAccountStatusState::Uninitialized => {}
            }
        }

        let mut shortcuts = vec![
            KeyHint::new("r", " refresh").with_key_style(Style::new().fg(colors::info())),
            KeyHint::new("a", " accounts").with_key_style(Style::new().fg(colors::primary())),
            KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error())),
        ];
        if let Some(crate::chatwidget::AppsAccountStatusState::Failed { needs_login: true, .. }) =
            snapshot.status_by_account_id.get(account_id)
        {
            shortcuts.insert(
                2,
                KeyHint::new("l", " login").with_key_style(Style::new().fg(colors::success())),
            );
        }

        SettingsMenuPage::new(
            "Apps",
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            vec![shortcut_line(&shortcuts)],
        )
    }
}
