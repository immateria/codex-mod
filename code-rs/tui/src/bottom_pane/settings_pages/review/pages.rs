use super::*;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::colors;
use crate::bottom_pane::settings_ui::hints::{hint_esc, hint_enter, hint_nav, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;

impl ReviewSettingsView {
    fn render_header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Configure /review and Auto Review models, resolve models, and follow-ups.",
                Style::new().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                format!("Use {ud} to navigate · Enter select/open · Space toggle · {lr} adjust values · Esc close", ud = crate::icons::nav_up_down(), lr = crate::icons::nav_left_right()),
                Style::new().fg(colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    fn shortcuts(&self) -> Vec<KeyHint<'static>> {
        vec![
            hint_nav(" Navigate"),
            hint_enter(" Select"),
            KeyHint::new(crate::bottom_pane::settings_ui::hints::key_space(), " Toggle")
                .with_key_style(Style::new().fg(colors::success())),
            KeyHint::new(crate::icons::nav_left_right(), " Adjust").with_key_style(Style::new().fg(colors::function())),
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
            "Review Settings",
            SettingsPanelStyle::bottom_pane(),
            self.render_header_lines(),
            self.render_footer_lines(),
        )
        .with_shortcuts(crate::bottom_pane::settings_ui::hints::ShortcutPlacement::Bottom, 
            self.shortcuts(),
        )
    }
}
