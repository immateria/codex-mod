use ratatui::layout::Margin;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{hint_nav, shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

use super::PlanningSettingsView;

impl PlanningSettingsView {
    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Planning Settings",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            vec![
                Line::from(Span::styled(
                    "Select the model used when you’re in Plan Mode (Read Only).",
                    Style::new().fg(colors::text_dim()),
                )),
                shortcut_line(&[
                    hint_nav(" navigate"),
                    KeyHint::new("Enter/Space", " toggle/open")
                        .with_key_style(Style::new().fg(colors::function())),
                    KeyHint::new("Esc", " close")
                        .with_key_style(Style::new().fg(colors::function())),
                ]),
                Line::from(""),
            ],
            vec![],
        )
    }
}

