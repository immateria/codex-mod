use ratatui::layout::Margin;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{hint_enter, hint_esc, hint_nav, KeyHint};
use crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage;
use crate::bottom_pane::settings_ui::panel::SettingsPanelStyle;
use crate::bottom_pane::settings_ui::toggle;
use crate::colors;

use super::{NotificationsMode, NotificationsSettingsView};

impl NotificationsSettingsView {
    fn status_line(&self) -> Line<'static> {
        match &self.mode {
            NotificationsMode::Toggle { enabled } => {
                let mut status = toggle::enabled_word_warning_off(*enabled);
                status.style = status.style.bold();
                Line::from(vec![
                    Span::styled("Status: ", Style::new().fg(colors::text_dim())),
                    Span::styled(status.text, status.style),
                ])
            }
            NotificationsMode::Custom { entries } => {
                let filters = if entries.is_empty() {
                    "<none>".to_string()
                } else {
                    entries.join(", ")
                };
                Line::from(vec![
                    Span::styled("Status: ", Style::new().fg(colors::text_dim())),
                    Span::styled("Custom filter", Style::new().fg(colors::info()).bold()),
                    Span::raw("  "),
                    Span::styled(filters, Style::new().fg(colors::dim())),
                ])
            }
        }
    }

    pub(super) fn page(&self) -> SettingsMenuPage<'static> {
        let (footer_lines, shortcuts) = match &self.mode {
            NotificationsMode::Toggle { .. } => (Vec::new(), vec![
                hint_nav(" navigate"),
                KeyHint::new(format!("{lr}/Space", lr = crate::icons::nav_left_right()), " toggle")
                    .with_key_style(Style::new().fg(colors::success())),
                hint_enter(" toggle/close"),
                hint_esc(" close"),
            ]),
            NotificationsMode::Custom { .. } => (vec![Line::from(vec![
                Span::styled("Edit ", Style::new().fg(colors::text_dim())),
                Span::styled("[tui].notifications", Style::new().fg(colors::info())),
                Span::styled(
                    " in ~/.code/config.toml to adjust filters.",
                    Style::new().fg(colors::text_dim()),
                ),
            ])], Vec::new()),
        };

        SettingsMenuPage::new(
            "Notifications",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            vec![self.status_line(), Line::from("")],
            footer_lines,
        )
        .with_shortcuts(crate::bottom_pane::settings_ui::hints::ShortcutPlacement::Bottom, shortcuts)
    }
}
