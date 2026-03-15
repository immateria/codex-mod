use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{shortcut_line, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::colors;

impl ValidationSettingsView {
    fn render_header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Toggle validation groups and installed tools.",
                Style::new().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Use ↑↓ to navigate · Enter/Space toggle · Esc close",
                Style::new().fg(colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    fn render_footer_lines(&self) -> Vec<Line<'static>> {
        let shortcuts = shortcut_line(&[
            KeyHint::new("↑↓", " Navigate").with_key_style(Style::new().fg(colors::function())),
            KeyHint::new("Enter/Space", " Toggle")
                .with_key_style(Style::new().fg(colors::success())),
            KeyHint::new("Esc", " Close").with_key_style(Style::new().fg(colors::error())),
        ]);

        let notice_line = match &self.pending_notice {
            Some(notice) => Line::from(Span::styled(
                notice.clone(),
                Style::new().fg(colors::warning()),
            )),
            None => Line::default(),
        };

        vec![shortcuts, notice_line]
    }

    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Validation Settings",
            SettingsPanelStyle::bottom_pane(),
            self.render_header_lines(),
            self.render_footer_lines(),
        )
    }
}

