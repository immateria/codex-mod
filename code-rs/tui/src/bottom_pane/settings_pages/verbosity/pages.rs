use ratatui::layout::Margin;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{hint_esc, hint_enter, hint_nav};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

use super::VerbositySelectionView;

impl VerbositySelectionView {
    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        let header_lines = vec![Line::from(vec![
            Span::styled("Current: ", Style::new().fg(colors::text_dim())),
            Span::styled(
                self.current_verbosity.to_string(),
                Style::new().fg(colors::warning()).bold(),
            ),
        ])];
        let shortcuts = vec![
            hint_nav(" navigate"),
            hint_enter(" select"),
            hint_esc(" close"),
        ];

        SettingsMenuPage::new(
            "Text verbosity",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            header_lines,
            Vec::new(),
        )
        .with_shortcuts(crate::bottom_pane::settings_ui::hints::ShortcutPlacement::Bottom, shortcuts)
    }
}
