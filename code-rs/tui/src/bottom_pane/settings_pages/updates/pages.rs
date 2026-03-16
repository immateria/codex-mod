use ratatui::layout::Margin;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

use super::UpdateSettingsView;

impl UpdateSettingsView {
    pub(super) fn footer_lines() -> Vec<Line<'static>> {
        vec![shortcut_line(&[
            KeyHint::new("Up/Down", " move"),
            KeyHint::new("Enter", " activate"),
            KeyHint::new("Space", " toggle"),
            KeyHint::new("Esc", " close"),
        ])]
    }

    pub(super) fn header_lines(&self) -> Vec<Line<'static>> {
        let guide_line = if self.command.is_some() {
            let guided_command_label = self.guided_command_label();
            format!("Guided command: {guided_command_label}")
        } else {
            "Run Upgrade will post manual instructions in the transcript.".to_string()
        };

        vec![
            Line::from(vec![
                Span::styled("Current: ", Style::new().fg(colors::text_dim())),
                Span::styled(
                    self.current_version.clone(),
                    Style::new().fg(colors::text()).bold(),
                ),
            ]),
            Line::from(Span::styled(guide_line, Style::new().fg(colors::text_dim()))),
        ]
    }

    fn guided_command_label(&self) -> String {
        self.command_display.clone().unwrap_or_else(|| {
            self.command
                .as_ref()
                .map(|command| command.join(" "))
                .unwrap_or_else(|| "manual instructions".to_string())
        })
    }

    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            Self::PANEL_TITLE,
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            self.header_lines(),
            Self::footer_lines(),
        )
    }
}
