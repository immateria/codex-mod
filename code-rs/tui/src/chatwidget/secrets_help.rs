use crate::bottom_pane::SettingsSection;

use super::ChatWidget;

impl ChatWidget<'_> {
    pub(crate) fn handle_secrets_command(&mut self) {
        self.show_settings_overlay(Some(SettingsSection::Secrets));
    }
}
