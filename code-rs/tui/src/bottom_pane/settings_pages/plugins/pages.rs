use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::layout::Constraint;

use crate::bottom_pane::settings_ui::action_page::SettingsActionPage;
use crate::bottom_pane::settings_ui::buttons::{
    standard_button_specs,
    SettingsButtonKind,
    StandardButtonSpec,
};
use crate::bottom_pane::settings_ui::form_page::{SettingsFormPage, SettingsFormSection};
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
                    let lowered = err.to_ascii_lowercase();
                    if lowered.contains("refresh_token_reused")
                        || lowered.contains("auth required")
                        || lowered.contains("sign in")
                    {
                        header_lines.push(Line::from(Span::styled(
                            "Hint: run /login and try sync again.".to_string(),
                            Style::new().fg(colors::text_dim()),
                        )));
                    }
                }
                if !marketplace_load_errors.is_empty() {
                    let error_count = marketplace_load_errors.len();
                    header_lines.push(Line::from(Span::styled(
                        format!("{error_count} marketplace(s) failed to load."),
                        Style::new().fg(colors::warning()),
                    )));
                    for error in marketplace_load_errors.iter().take(2) {
                        header_lines.push(Line::from(Span::styled(
                            format!("  {}: {}", error.path.display(), error.message),
                            Style::new().fg(colors::text_dim()),
                        )));
                    }
                    if error_count > 2 {
                        header_lines.push(Line::from(Span::styled(
                            format!("  ... and {} more", error_count.saturating_sub(2)),
                            Style::new().fg(colors::text_dim()),
                        )));
                    }
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
                KeyHint::new("s", " sources").with_key_style(Style::new().fg(colors::primary())),
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

    pub(super) fn sources_list_page(&self, snapshot: &PluginsSharedState) -> SettingsMenuPage<'static> {
        let mut header_lines = vec![
            Line::from(Span::styled(
                "Edit plugin marketplace git sources (curated + additional repos).",
                Style::new().fg(colors::text_dim()),
            )),
        ];

        if snapshot.sources_sync_in_progress {
            header_lines.push(Line::from(Span::styled(
                "Syncing marketplaces...",
                Style::new().fg(colors::function()),
            )));
        }
        if let Some(err) = snapshot.sources_sync_error.as_ref() {
            header_lines.push(Line::from(Span::styled(
                format!("Marketplace sync error: {err}"),
                Style::new().fg(colors::warning()),
            )));
        }

        header_lines.push(Line::from(""));

        SettingsMenuPage::new(
            "Plugins",
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            vec![shortcut_line(&[
                KeyHint::new("↑↓", " navigate").with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("Enter", " edit").with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("a", " add repo").with_key_style(Style::new().fg(colors::primary())),
                KeyHint::new("Del", " remove").with_key_style(Style::new().fg(colors::error())),
                KeyHint::new("r", " refresh").with_key_style(Style::new().fg(colors::info())),
                KeyHint::new("R", " sync").with_key_style(Style::new().fg(colors::info())),
                KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error())),
            ])],
        )
    }

    pub(super) fn sources_list_rows(
        &self,
        snapshot: &PluginsSharedState,
    ) -> Vec<SettingsMenuRow<'static, usize>> {
        let sources = &snapshot.sources;
        let mut rows = Vec::new();

        let curated_is_custom = sources
            .curated_repo_url
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let curated_value = if curated_is_custom { "Custom" } else { "Default" };
        let curated_style = if curated_is_custom {
            Style::new().fg(colors::primary())
        } else {
            Style::new().fg(colors::text_dim())
        };
        let mut curated_detail = if curated_is_custom {
            sources
                .curated_repo_url
                .clone()
                .unwrap_or_else(|| "Custom".to_string())
        } else {
            "Uses the built-in curated marketplace.".to_string()
        };
        if curated_is_custom
            && let Some(git_ref) = sources.curated_repo_ref.as_deref()
            && !git_ref.trim().is_empty()
        {
            curated_detail.push_str(&format!(" @ {git_ref}"));
        }
        rows.push(
            SettingsMenuRow::new(0usize, "Curated marketplace")
                .with_value(StyledText::new(curated_value.to_string(), curated_style))
                .with_detail(StyledText::new(curated_detail, Style::new().fg(colors::text_dim()))),
        );

        rows.push(
            SettingsMenuRow::new(1usize, "Add marketplace repo")
                .with_detail(StyledText::new(
                    "Append a new git repo source.".to_string(),
                    Style::new().fg(colors::text_dim()),
                )),
        );

        for (idx, repo) in sources.marketplace_repos.iter().enumerate() {
            let row_id = idx.saturating_add(2);
            let mut detail = repo.url.clone();
            if let Some(git_ref) = repo.git_ref.as_deref()
                && !git_ref.trim().is_empty()
            {
                detail.push_str(&format!(" @ {git_ref}"));
            }
            rows.push(
                SettingsMenuRow::new(row_id, format!("Repo {}", idx + 1))
                    .with_value(StyledText::new(
                        repo.url.clone(),
                        Style::new().fg(colors::text()),
                    ))
                    .with_detail(StyledText::new(detail, Style::new().fg(colors::text_dim()))),
            );
        }

        rows
    }

    pub(super) fn sources_editor_form_page(
        &self,
        snapshot: &PluginsSharedState,
        mode: &SourcesMode,
    ) -> SettingsFormPage<'static> {
        let title = match mode {
            SourcesMode::EditCurated => "Curated source",
            SourcesMode::EditMarketplaceRepo { .. } => "Marketplace repo",
            _ => "Sources",
        };

        let mut header_lines = vec![title_line(title.to_string())];
        match mode {
            SourcesMode::EditCurated => {
                header_lines.push(Line::from(
                    "Override the curated marketplace repo. Leave URL blank to use the default.",
                ));
            }
            SourcesMode::EditMarketplaceRepo { .. } => {
                header_lines.push(Line::from(
                    "Add or edit an additional marketplace git repo (URL required).",
                ));
            }
            _ => {}
        }
        header_lines.push(Line::from(""));

        let status = if let Some(err) = self.sources_editor.error.as_ref() {
            Some(StyledText::new(err.clone(), Style::new().fg(colors::error())))
        } else if let Some(err) = snapshot.sources_sync_error.as_ref() {
            Some(StyledText::new(
                format!("Marketplace sync error: {err}"),
                Style::new().fg(colors::warning()),
            ))
        } else if snapshot.sources_sync_in_progress {
            Some(StyledText::new(
                "Syncing marketplaces...".to_string(),
                Style::new().fg(colors::function()),
            ))
        } else {
            None
        };

        let (status_lines, footer_lines) = status_and_shortcuts_split(
            status,
            &[
                KeyHint::new("Tab", " next"),
                KeyHint::new("Ctrl+S", " save").with_key_style(Style::new().fg(colors::success())),
                KeyHint::new("Esc", " cancel").with_key_style(Style::new().fg(colors::error())),
            ],
        );

        let page = SettingsActionPage::new(
            "Plugins",
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            footer_lines,
        )
        .with_status_lines(status_lines)
        .with_action_rows(1)
        .with_min_body_rows(4);

        SettingsFormPage::new(
            page,
            vec![
                SettingsFormSection::new("Repository URL", false, Constraint::Length(1)),
                SettingsFormSection::new("Git ref (optional)", false, Constraint::Length(1)),
            ],
        )
        .with_section_gap_rows(1)
    }

    pub(super) fn sources_editor_button_specs(&self) -> Vec<StandardButtonSpec<SourcesEditorAction>> {
        let focused = match self.sources_editor.selected_row {
            2 => Some(SourcesEditorAction::Save),
            3 => Some(SourcesEditorAction::Cancel),
            _ => None,
        };
        standard_button_specs(
            &[
                (SourcesEditorAction::Save, SettingsButtonKind::Save),
                (SourcesEditorAction::Cancel, SettingsButtonKind::Cancel),
            ],
            focused,
            self.sources_editor.hovered_button,
        )
    }

    pub(super) fn sources_confirm_remove_page(
        &self,
        snapshot: &PluginsSharedState,
    ) -> SettingsActionPage<'static> {
        let status = snapshot.sources_sync_error.as_ref().map(|err| {
            StyledText::new(
                format!("Marketplace sync error: {err}"),
                Style::new().fg(colors::warning()),
            )
        });
        let shortcuts = [
            KeyHint::new("←→", " actions").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter", " activate").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error())),
        ];
        let (status_lines, footer_lines) = status_and_shortcuts_split(status, &shortcuts);
        SettingsActionPage::new(
            "Plugins",
            SettingsPanelStyle::bottom_pane(),
            vec![title_line("Confirm remove".to_string())],
            footer_lines,
        )
        .with_status_lines(status_lines)
        .with_wrap_lines(true)
        .with_action_rows(1)
    }

    pub(super) fn sources_confirm_remove_button_specs(
        &self,
    ) -> Vec<StandardButtonSpec<SourcesConfirmRemoveAction>> {
        standard_button_specs(
            &[
                (SourcesConfirmRemoveAction::Delete, SettingsButtonKind::Delete),
                (SourcesConfirmRemoveAction::Cancel, SettingsButtonKind::Cancel),
            ],
            Some(self.focused_sources_confirm_button),
            self.hovered_sources_confirm_button,
        )
    }
}
