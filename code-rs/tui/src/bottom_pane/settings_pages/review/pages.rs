use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::colors;
use crate::bottom_pane::settings_ui::hints::{hint_esc, hint_enter, hint_nav, hint_nav_horizontal, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;

impl ReviewSettingsView {
    fn render_header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Configure /review and Auto Review models, resolve models, and follow-ups.",
                Style::new().fg(colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    fn shortcuts(&self) -> Vec<KeyHint<'static>> {
        vec![
            hint_nav(" navigate"),
            hint_enter(" activate"),
            KeyHint::new(crate::bottom_pane::settings_ui::hints::key_space(), " toggle"),
            hint_nav_horizontal(" adjust"),
            hint_esc(" close"),
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
            "Review",
            SettingsPanelStyle::bottom_pane(),
            self.render_header_lines(),
            self.render_footer_lines(),
        )
        .with_shortcuts(crate::bottom_pane::settings_ui::hints::ShortcutPlacement::Bottom, 
            self.shortcuts(),
        )
    }
}
