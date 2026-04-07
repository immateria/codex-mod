use ratatui::layout::Margin;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{hint_enter, hint_esc, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

use super::SettingsOverviewView;

impl SettingsOverviewView {
    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Settings",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(1, 0)),
            Vec::new(),
            vec![Line::from(vec![Span::styled(
                self.selected_section()
                    .map(crate::bottom_pane::SettingsSection::help_line)
                    .unwrap_or(""),
                Style::new().fg(colors::text_dim()),
            )])],
        )
        .with_shortcuts(vec![
            KeyHint::new("↑↓/jk", " move")
                .with_key_style(Style::new().fg(colors::function())),
            hint_enter(" open").with_key_style(Style::new().fg(colors::function())),
            hint_esc(" close").with_key_style(Style::new().fg(colors::function())),
        ])
    }
}
