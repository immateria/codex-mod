use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::buttons::{
    standard_button_specs,
    SettingsButtonKind,
    StandardButtonSpec,
};
use crate::bottom_pane::settings_ui::hints::{
    shortcut_line,
    status_and_shortcuts_split,
    title_line,
    KeyHint,
};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;

use crate::chatwidget::PluginsListState;

#[derive(Clone, Debug)]
pub(super) struct PluginRow {
    pub(super) marketplace_name: String,
    pub(super) marketplace_path: AbsolutePathBuf,
    pub(super) plugin_name: String,
    pub(super) featured: bool,
    pub(super) installed: bool,
    pub(super) enabled: bool,
}

impl PluginsSettingsView {
    pub(super) fn shared_snapshot(&self) -> PluginsSharedState {
        self.shared_state
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub(super) fn plugin_rows_from_snapshot(snapshot: &PluginsSharedState) -> Vec<PluginRow> {
        let PluginsListState::Ready {
            marketplaces,
            featured_plugin_ids,
            ..
        } = &snapshot.list
        else {
            return Vec::new();
        };

        let mut rows = Vec::new();
        for marketplace in marketplaces {
            for plugin in &marketplace.plugins {
                let featured = featured_plugin_ids.contains(&plugin.id);
                rows.push(PluginRow {
                    marketplace_name: marketplace.name.clone(),
                    marketplace_path: marketplace.path.clone(),
                    plugin_name: plugin.name.clone(),
                    featured,
                    installed: plugin.installed,
                    enabled: plugin.enabled,
                });
            }
        }
        rows
    }

    pub(super) fn selected_plugin_row(&self, plugin_rows: &[PluginRow]) -> Option<PluginRow> {
        if plugin_rows.is_empty() {
            return None;
        }
        let idx = self.selected_list_index(plugin_rows.len());
        plugin_rows.get(idx).cloned()
    }

    pub(super) fn list_page(&self, snapshot: &PluginsSharedState) -> SettingsMenuPage<'static> {
        let mut header_lines = vec![
            Line::from(Span::styled(
                "Browse and manage installed plugins and marketplaces.",
                Style::new().fg(colors::text_dim()),
            )),
        ];

        match &snapshot.list {
            PluginsListState::Loading { force_remote_sync, .. } => {
                let label = if *force_remote_sync {
                    "Syncing marketplaces..."
                } else {
                    "Loading marketplaces..."
                };
                header_lines.push(Line::from(Span::styled(
                    label,
                    Style::new().fg(colors::function()),
                )));
            }
            PluginsListState::Failed { error, .. } => {
                header_lines.push(Line::from(Span::styled(
                    format!("Failed to load plugins: {error}"),
                    Style::new().fg(colors::error()),
                )));
            }
            PluginsListState::Ready {
                marketplace_load_errors,
                remote_sync_error,
                ..
            } => {
                if let Some(err) = remote_sync_error.as_ref() {
                    header_lines.push(Line::from(Span::styled(
                        format!("Remote sync error: {err}"),
                        Style::new().fg(colors::warning()),
                    )));
                }
                if !marketplace_load_errors.is_empty() {
                    let error_count = marketplace_load_errors.len();
                    header_lines.push(Line::from(Span::styled(
                        format!("{error_count} marketplace(s) failed to load."),
                        Style::new().fg(colors::warning()),
                    )));
                }
            }
            PluginsListState::Uninitialized => {}
        }

        if let Some(err) = snapshot.action_error.as_ref() {
            header_lines.push(Line::from(Span::styled(
                err.clone(),
                Style::new().fg(colors::error()),
            )));
        }
        if snapshot.action_error.is_none() {
            if let Some(action) = snapshot.action_in_progress.as_ref() {
                let message = match action {
                    crate::chatwidget::PluginsActionInProgress::FetchList => None,
                    crate::chatwidget::PluginsActionInProgress::FetchDetail(key) => Some(format!(
                        "Loading details for {}…",
                        key.plugin_name
                    )),
                    crate::chatwidget::PluginsActionInProgress::Install {
                        marketplace_path: _marketplace_path,
                        plugin_name,
                        force_remote_sync,
                    } => Some(if *force_remote_sync {
                        format!("Installing {plugin_name} (syncing remote)…")
                    } else {
                        format!("Installing {plugin_name}…")
                    }),
                    crate::chatwidget::PluginsActionInProgress::Uninstall {
                        plugin_id_key,
                        force_remote_sync: _force_remote_sync,
                    } => Some(format!("Uninstalling {plugin_id_key}…")),
                    crate::chatwidget::PluginsActionInProgress::SetEnabled {
                        plugin_id_key,
                        enabled,
                    } => Some(if *enabled {
                        format!("Enabling {plugin_id_key}…")
                    } else {
                        format!("Disabling {plugin_id_key}…")
                    }),
                };

                if let Some(message) = message {
                    header_lines.push(Line::from(Span::styled(
                        message,
                        Style::new().fg(colors::function()),
                    )));
                }
            }
        }

        header_lines.push(Line::from(""));

        SettingsMenuPage::new(
            "Plugins",
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            vec![shortcut_line(&[
                KeyHint::new("↑↓", " navigate").with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("Enter", " details").with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("r", " refresh").with_key_style(Style::new().fg(colors::info())),
                KeyHint::new("R", " sync").with_key_style(Style::new().fg(colors::info())),
                KeyHint::new("Esc", " close").with_key_style(Style::new().fg(colors::error())),
            ])],
        )
    }

    pub(super) fn list_rows(&self, snapshot: &PluginsSharedState) -> Vec<SettingsMenuRow<'static, usize>> {
        let plugin_rows = Self::plugin_rows_from_snapshot(snapshot);
        if plugin_rows.is_empty() {
            return vec![SettingsMenuRow::new(0usize, "No plugins found")
                .with_detail(StyledText::new(
                    "Try 'R' to sync marketplaces from remote.",
                    Style::new().fg(colors::text_dim()),
                ))
                .disabled()];
        }

        plugin_rows
            .into_iter()
            .enumerate()
            .map(|(idx, row)| {
                let label = if row.featured {
                    format!("{} (Featured)", row.plugin_name)
                } else {
                    row.plugin_name
                };
                let status_text = if row.installed { "Installed" } else { "Available" };
                let status_style = if row.installed {
                    Style::new().fg(colors::success())
                } else {
                    Style::new().fg(colors::text_dim())
                };

                let mut detail = row.marketplace_name.clone();
                if row.installed && !row.enabled {
                    detail.push_str(" · Disabled");
                }

                SettingsMenuRow::new(idx, label)
                    .with_value(StyledText::new(status_text.to_string(), status_style))
                    .with_detail(StyledText::new(detail, Style::new().fg(colors::text_dim())))
            })
            .collect()
    }

    pub(super) fn detail_page(
        &self,
        _snapshot: &PluginsSharedState,
        title: &str,
        status: Option<StyledText<'static>>,
        shortcuts: &[KeyHint<'_>],
    ) -> SettingsActionPage<'static> {
        let (status_lines, footer_lines) = status_and_shortcuts_split(status, shortcuts);
        SettingsActionPage::new(
            "Plugin",
            SettingsPanelStyle::bottom_pane(),
            vec![title_line(title.to_string())],
            footer_lines,
        )
        .with_status_lines(status_lines)
        .with_wrap_lines(true)
    }

    pub(super) fn detail_button_specs(
        &self,
        plugin_installed: bool,
        plugin_enabled: bool,
    ) -> Vec<StandardButtonSpec<DetailAction>> {
        let mut items = Vec::new();

        if plugin_installed {
            items.push((DetailAction::Uninstall, SettingsButtonKind::Uninstall));
            items.push((
                if plugin_enabled {
                    DetailAction::Disable
                } else {
                    DetailAction::Enable
                },
                if plugin_enabled {
                    SettingsButtonKind::Disable
                } else {
                    SettingsButtonKind::Enable
                },
            ));
        } else {
            items.push((DetailAction::Install, SettingsButtonKind::Install));
        }
        items.push((DetailAction::Back, SettingsButtonKind::Back));

        standard_button_specs(
            &items,
            Some(self.focused_detail_button),
            self.hovered_detail_button,
        )
    }

    pub(super) fn confirm_button_specs(&self) -> Vec<StandardButtonSpec<ConfirmAction>> {
        standard_button_specs(
            &[
                (ConfirmAction::Uninstall, SettingsButtonKind::Uninstall),
                (ConfirmAction::Cancel, SettingsButtonKind::Cancel),
            ],
            Some(self.focused_confirm_button),
            self.hovered_confirm_button,
        )
    }
}
