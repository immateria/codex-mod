use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::hints::KeyHint;
use crate::colors;
use crate::util::buffer::fill_rect;

impl PluginsSettingsView {
    pub(super) fn render_framed(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }

    pub(super) fn render_content_only(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }

    fn render_in_chrome(&self, chrome: ChromeMode, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let snapshot = self.shared_snapshot();

        match &self.mode {
            Mode::List => self.render_list(chrome, area, buf, &snapshot),
            Mode::Detail { key } => self.render_detail(chrome, area, buf, &snapshot, key.clone()),
            Mode::ConfirmUninstall { plugin_id_key: _, key } => {
                self.render_confirm_uninstall(chrome, area, buf, &snapshot, key.clone())
            }
            Mode::Sources(mode) => self.render_sources(chrome, area, buf, &snapshot, mode.clone()),
        }
    }

    fn render_list(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &PluginsSharedState,
    ) {
        let rows = self.list_rows(snapshot);
        let page = self.list_page(snapshot);
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return;
        };

        let visible_rows = layout.body.height.max(1) as usize;
        self.list_viewport_rows.set(visible_rows);

        let mut state = self.list_state.get();
        state.clamp_selection(rows.len());
        state.ensure_visible(rows.len(), visible_rows);
        self.list_state.set(state);

        let _ = page.render_menu_rows_in_chrome(
            chrome,
            area,
            buf,
            state.scroll_top,
            state.selected_idx,
            &rows,
        );
    }

    fn render_detail(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &PluginsSharedState,
        key: PluginDetailKey,
    ) {
        let mut status = snapshot.action_error.as_ref().map(|err| {
            StyledText::new(err.clone(), Style::new().fg(colors::error()))
        });

        let shortcuts = [
            KeyHint::new("←→", " actions").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter", " activate").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error())),
        ];

        let detail_state = snapshot.details.get(&key);
        if detail_state.is_none() {
            status = Some(StyledText::new(
                "Loading plugin details...".to_string(),
                Style::new().fg(colors::function()),
            ));
        }

        let title = key.plugin_name.clone();
        let page = self.detail_page(snapshot, &title, status, &shortcuts);

        let (installed, enabled) = detail_state
            .and_then(|state| match state {
                crate::chatwidget::PluginsDetailState::Ready(outcome) => {
                    Some((outcome.plugin.installed, outcome.plugin.enabled))
                }
                _ => None,
            })
            .unwrap_or((false, false));

        let buttons = self.detail_button_specs(installed, enabled);
        let Some(layout) = page.render_with_standard_actions_end_in_chrome(chrome, area, buf, &buttons) else {
            return;
        };

        let lines = build_detail_body_lines(detail_state, &key);
        let paragraph = Paragraph::new(lines)
            .style(Style::new().bg(colors::background()).fg(colors::text()))
            .wrap(Wrap { trim: false });
        paragraph.render(layout.body, buf);
    }

    fn render_confirm_uninstall(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &PluginsSharedState,
        key: PluginDetailKey,
    ) {
        let status = snapshot.action_error.as_ref().map(|err| {
            StyledText::new(err.clone(), Style::new().fg(colors::error()))
        });
        let shortcuts = [
            KeyHint::new("←→", " actions").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter", " activate").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " back").with_key_style(Style::new().fg(colors::error())),
        ];
        let page = self.detail_page(snapshot, "Confirm uninstall", status, &shortcuts);

        let buttons = self.confirm_button_specs();
        let Some(layout) = page.render_with_standard_actions_end_in_chrome(chrome, area, buf, &buttons) else {
            return;
        };

        let base = Style::new().bg(colors::background()).fg(colors::text());
        fill_rect(buf, layout.body, Some(' '), base);
        let mut lines = Vec::new();
        lines.push(Line::from(Span::styled(
            format!("Uninstall {}?", key.plugin_name),
            Style::new().fg(colors::warning()),
        )));
        lines.push(Line::from(Span::styled(
            "This will remove plugin files and clear its config entry.",
            Style::new().fg(colors::text_dim()),
        )));
        Paragraph::new(lines)
            .style(base)
            .wrap(Wrap { trim: false })
            .render(layout.body, buf);
    }

    fn render_sources(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &PluginsSharedState,
        mode: SourcesMode,
    ) {
        match mode {
            SourcesMode::List => self.render_sources_list(chrome, area, buf, snapshot),
            SourcesMode::EditCurated | SourcesMode::EditMarketplaceRepo { .. } => {
                self.render_sources_editor(chrome, area, buf, snapshot, &mode)
            }
            SourcesMode::ConfirmRemoveRepo { index } => {
                self.render_sources_confirm_remove(chrome, area, buf, snapshot, index)
            }
        }
    }

    fn render_sources_list(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &PluginsSharedState,
    ) {
        let rows = self.sources_list_rows(snapshot);
        let page = self.sources_list_page(snapshot);
        let Some(layout) = page.layout_in_chrome(chrome, area) else {
            return;
        };

        let visible_rows = layout.body.height.max(1) as usize;
        self.sources_list_viewport_rows.set(visible_rows);

        let mut state = self.sources_list_state.get();
        state.clamp_selection(rows.len());
        state.ensure_visible(rows.len(), visible_rows);
        self.sources_list_state.set(state);

        let _ = page.render_menu_rows_in_chrome(
            chrome,
            area,
            buf,
            state.scroll_top,
            state.selected_idx,
            &rows,
        );
    }

    fn render_sources_editor(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &PluginsSharedState,
        mode: &SourcesMode,
    ) {
        let page = self.sources_editor_form_page(snapshot, mode);
        let buttons = self.sources_editor_button_specs();
        let _ = page.render_with_standard_actions_end_in_chrome(
            chrome,
            area,
            buf,
            &[&self.sources_editor.url_field, &self.sources_editor.ref_field],
            &buttons,
        );
    }

    fn render_sources_confirm_remove(
        &self,
        chrome: ChromeMode,
        area: Rect,
        buf: &mut Buffer,
        snapshot: &PluginsSharedState,
        index: usize,
    ) {
        let page = self.sources_confirm_remove_page(snapshot);
        let buttons = self.sources_confirm_remove_button_specs();
        let Some(layout) = page.render_with_standard_actions_end_in_chrome(chrome, area, buf, &buttons) else {
            return;
        };

        let base = Style::new().bg(colors::background()).fg(colors::text());
        fill_rect(buf, layout.body, Some(' '), base);

        let label = snapshot
            .sources
            .marketplace_repos
            .get(index)
            .map(|repo| repo.url.as_str())
            .unwrap_or("(unknown repo)");
        let lines = vec![
            Line::from(Span::styled(
                "Remove marketplace repo?".to_string(),
                Style::new().fg(colors::warning()),
            )),
            Line::from(Span::styled(
                label.to_string(),
                Style::new().fg(colors::text_dim()),
            )),
        ];
        Paragraph::new(lines)
            .style(base)
            .wrap(Wrap { trim: false })
            .render(layout.body, buf);
    }
}

fn build_detail_body_lines(
    detail_state: Option<&crate::chatwidget::PluginsDetailState>,
    key: &PluginDetailKey,
) -> Vec<Line<'static>> {
    match detail_state {
        None | Some(crate::chatwidget::PluginsDetailState::Loading) => vec![Line::from(
            Span::styled("Loading plugin details...", Style::new().fg(colors::function())),
        )],
        Some(crate::chatwidget::PluginsDetailState::Failed(err)) => vec![Line::from(
            Span::styled(format!("Failed to load plugin: {err}"), Style::new().fg(colors::error())),
        )],
        Some(crate::chatwidget::PluginsDetailState::Uninitialized) => vec![Line::from(
            Span::styled(
                "Loading plugin details...".to_string(),
                Style::new().fg(colors::function()),
            ),
        )],
        Some(crate::chatwidget::PluginsDetailState::Ready(outcome)) => {
            let mut lines = Vec::<Line<'static>>::new();
            let detail = &outcome.plugin;

            lines.push(Line::from(Span::styled(
                format!("Marketplace: {}", outcome.marketplace_name),
                Style::new().fg(colors::text_dim()),
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "Status: {}{}",
                    if detail.installed { "Installed" } else { "Not installed" },
                    if detail.installed && !detail.enabled {
                        " (Disabled)"
                    } else {
                        ""
                    }
                ),
                Style::new().fg(colors::text_dim()),
            )));
            lines.push(Line::from(""));

            if let Some(desc) = detail.description.as_ref() {
                lines.push(Line::from(Span::raw(desc.clone())));
                lines.push(Line::from(""));
            } else {
                lines.push(Line::from(Span::styled(
                    "No description available.",
                    Style::new().fg(colors::text_dim()),
                )));
                lines.push(Line::from(""));
            }

            lines.push(Line::from(Span::styled(
                "Skills:",
                Style::new().fg(colors::primary()).bold(),
            )));
            if detail.skills.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (none)",
                    Style::new().fg(colors::text_dim()),
                )));
            } else {
                for skill in &detail.skills {
                    lines.push(Line::from(Span::styled(
                        format!("  • {}", skill.name),
                        Style::new().fg(colors::text()),
                    )));
                }
            }
            lines.push(Line::from(""));

            lines.push(Line::from(Span::styled(
                "MCP servers:",
                Style::new().fg(colors::primary()).bold(),
            )));
            if detail.mcp_server_names.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (none)",
                    Style::new().fg(colors::text_dim()),
                )));
            } else {
                for server in &detail.mcp_server_names {
                    lines.push(Line::from(Span::raw(format!("  • {server}"))));
                }
            }
            lines.push(Line::from(""));

            lines.push(Line::from(Span::styled(
                "Apps:",
                Style::new().fg(colors::primary()).bold(),
            )));
            if detail.apps.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  (none)",
                    Style::new().fg(colors::text_dim()),
                )));
            } else {
                for app in &detail.apps {
                    lines.push(Line::from(Span::raw(format!("  • {}", app.0))));
                }
            }

            // If we haven't loaded the detail state yet, at least show which key we asked for.
            if lines.is_empty() {
                lines.push(Line::from(Span::raw(format!(
                    "{} @ {}",
                    key.plugin_name,
                    key.marketplace_path.display()
                ))));
            }
            lines
        }
    }
}
