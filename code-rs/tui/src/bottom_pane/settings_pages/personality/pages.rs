use ratatui::layout::Margin;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{hint_nav, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

use super::PersonalitySettingsView;

impl PersonalitySettingsView {
    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Personality",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            vec![
                Line::from(Span::styled(
                    "Archetype sets character; Tone is an orthogonal modifier.",
                    Style::new().fg(colors::text_dim()),
                )),
                Line::from(Span::styled(
                    "Traits fine-tune individual dimensions (1–5 scale, 3 = neutral).",
                    Style::new().fg(colors::text_dim()),
                )),
                Line::from(""),
            ],
            Vec::new(),
        )
        .with_shortcuts(
            crate::bottom_pane::settings_ui::hints::ShortcutPlacement::Bottom,
            vec![
                hint_nav(" navigate"),
                KeyHint::new(
                    format!("{} cycle", crate::icons::nav_left_right()),
                    "",
                ),
                crate::bottom_pane::settings_ui::hints::hint_esc(" close"),
            ],
        )
    }
}
