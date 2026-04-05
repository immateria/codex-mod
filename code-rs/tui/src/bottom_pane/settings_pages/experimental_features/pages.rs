use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::colors;

impl ExperimentalFeaturesSettingsView {
    pub(super) fn overview_page(&self) -> SettingsMenuPage<'static> {
        let mut header_lines = vec![
            Line::from(Span::styled(
                "Toggle experimental features. Changes are saved to config.toml (active profile when set) and applied after session reconfigure.",
                Style::new().fg(colors::text_dim()),
            )),
        ];

        let profile_label = self.active_profile.as_deref().unwrap_or("default");
        header_lines.push(Line::from(Span::styled(
            format!("Profile: {profile_label}"),
            Style::new().fg(colors::text_dim()),
        )));

        if self.dirty {
            header_lines.push(Line::from(Span::styled(
                "Unsaved changes (Ctrl+S to save)".to_string(),
                Style::new().fg(colors::warning()),
            )));
        }

        let shortcuts = vec![
            KeyHint::new("↑↓", " select").with_key_style(Style::new().fg(colors::primary())),
            KeyHint::new("Space", " toggle").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Ctrl+S", " save").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " close").with_key_style(Style::new().fg(colors::error())),
        ];

        SettingsMenuPage::new(
            "Experimental",
            SettingsPanelStyle::bottom_pane(),
            header_lines,
            vec![shortcut_line(&shortcuts)],
        )
        .with_detail_pane()
    }

    pub(super) fn overview_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        if self.rows.is_empty() {
            return vec![SettingsMenuRow::new(0usize, "No experimental features available")
                .disabled()];
        }

        self.rows
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                let enabled = self
                    .draft_enabled
                    .get(idx)
                    .copied()
                    .unwrap_or(row.default_enabled);
                let value_style = if enabled {
                    Style::new().fg(colors::success())
                } else {
                    Style::new().fg(colors::dim())
                };
                let value = if enabled { "on" } else { "off" };

                let description = if cfg!(target_os = "android") && row.key == "prevent_idle_sleep" {
                    format!("{} (no-op on Android)", row.description)
                } else {
                    row.description.to_string()
                };

                SettingsMenuRow::new(idx, row.name)
                    .with_value(StyledText::new(value.to_string(), value_style))
                    .with_detail(StyledText::new(
                        description,
                        Style::new().fg(colors::text_dim()),
                    ))
            })
            .collect()
    }
}
