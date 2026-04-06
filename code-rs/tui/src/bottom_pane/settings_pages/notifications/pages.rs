use ratatui::layout::Margin;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use crate::bottom_pane::settings_ui::hints::{hint_enter, hint_nav, shortcut_line, KeyHint};
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
        let footer_lines = match &self.mode {
            NotificationsMode::Toggle { .. } => vec![shortcut_line(&[
                hint_nav(" navigate"),
                KeyHint::new("←→/Space", " toggle")
                    .with_key_style(Style::new().fg(colors::success())),
                hint_enter(" toggle/close"),
                KeyHint::new("Esc", " close")
                    .with_key_style(Style::new().fg(colors::error()).bold()),
            ])],
            NotificationsMode::Custom { .. } => vec![Line::from(vec![
                Span::styled("Edit ", Style::new().fg(colors::text_dim())),
                Span::styled("[tui].notifications", Style::new().fg(colors::info())),
                Span::styled(
                    " in ~/.code/config.toml to adjust filters.",
                    Style::new().fg(colors::text_dim()),
                ),
            ])],
        };

        SettingsMenuPage::new(
            "Notifications",
            SettingsPanelStyle::bottom_pane().with_margin(Margin::new(0, 0)),
            vec![self.status_line(), Line::from("")],
            footer_lines,
        )
    }
}

