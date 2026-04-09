use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{hint_esc, hint_nav, KeyHint};
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
            Line::from(""),
        ]
    }

    fn shortcuts(&self) -> Vec<KeyHint<'static>> {
        vec![
            hint_nav(" Navigate"),
            KeyHint::new("Enter/Space", " Toggle")
                .with_key_style(Style::new().fg(colors::success())),
            hint_esc(" Close"),
        ]
    }

    fn render_footer_lines(&self) -> Vec<Line<'static>> {
        let notice_line = match &self.pending_notice {
            Some(notice) => Line::from(Span::styled(
                notice.clone(),
                Style::new().fg(colors::warning()),
            )),
            None => Line::default(),
        };

        vec![notice_line]
    }

    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        SettingsMenuPage::new(
            "Validation Settings",
            SettingsPanelStyle::bottom_pane(),
            self.render_header_lines(),
            self.render_footer_lines(),
        )
        .with_shortcuts(crate::bottom_pane::settings_ui::hints::ShortcutPlacement::Bottom, 
            self.shortcuts(),
        )
    }
}
