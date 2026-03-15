use ratatui::layout::Margin;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

use super::SettingsOverviewView;

impl SettingsOverviewView {
    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Settings",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            vec![shortcut_line(&[
                KeyHint::new("↑↓/jk", " move")
                    .with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("Enter", " open")
                    .with_key_style(Style::new().fg(colors::function())),
                KeyHint::new("Esc", " close")
                    .with_key_style(Style::new().fg(colors::function())),
            ])],
            vec![Line::from(vec![Span::styled(
                self.selected_section()
                    .map(crate::bottom_pane::SettingsSection::help_line)
                    .unwrap_or(""),
                Style::new().fg(colors::text_dim()),
            )])],
        )
    }
}

