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
            SettingsPanelStyle::bottom_pane_padded(),
            Vec::new(),
            vec![Line::from(vec![Span::styled(
                self.selected_section()
                    .map(crate::bottom_pane::SettingsSection::help_line)
                    .unwrap_or(""),
                Style::new().fg(colors::text_dim()),
            )])],
        )
        .with_shortcuts(crate::bottom_pane::settings_ui::hints::ShortcutPlacement::Bottom, 
            vec![
                KeyHint::new(format!("{ud}/jk", ud = crate::icons::nav_up_down()), " navigate"),
                hint_enter(" open"),
                hint_esc(" close"),
            ],
        )
    }
}
