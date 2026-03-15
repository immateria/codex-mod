use ratatui::style::{Style, Stylize};

use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::toggle;
use crate::colors;

use super::{NotificationsMode, NotificationsSettingsView};

impl NotificationsSettingsView {
    pub(super) fn menu_rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        let notifications_row = match &self.mode {
            NotificationsMode::Toggle { enabled } => {
                let mut status = toggle::enabled_word_warning_off(*enabled);
                status.style = status.style.bold();
                SettingsMenuRow::new(0usize, "Notifications").with_value(status)
            }
            NotificationsMode::Custom { entries } => {
                let filters = if entries.is_empty() {
                    "<none>".to_string()
                } else {
                    entries.join(", ")
                };
                SettingsMenuRow::new(0usize, "Notifications")
                    .with_value(StyledText::new(
                        "Custom filter".to_string(),
                        Style::new().fg(colors::info()).bold(),
                    ))
                    .with_detail(StyledText::new(filters, Style::new().fg(colors::dim())))
            }
        };

        vec![notifications_row, SettingsMenuRow::new(1usize, "Close")]
    }
}

