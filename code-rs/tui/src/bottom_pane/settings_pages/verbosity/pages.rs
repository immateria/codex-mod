use ratatui::layout::Margin;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

use super::VerbositySelectionView;

impl VerbositySelectionView {
    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        let header_lines = vec![Line::from(vec![
            Span::styled("Current: ", Style::new().fg(colors::text_dim())),
            Span::styled(
                format!("{}", self.current_verbosity),
                Style::new().fg(colors::warning()).bold(),
            ),
        ])];
        let footer_lines = vec![shortcut_line(&[
            KeyHint::new("↑↓", " navigate").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter", " select").with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " cancel").with_key_style(Style::new().fg(colors::error()).bold()),
        ])];

        SettingsMenuPage::new(
            "Text verbosity",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            header_lines,
            footer_lines,
        )
    }
}

