use ratatui::style::Style;

use crate::bottom_pane::settings_ui::menu_rows::SettingsMenuRow;
use crate::bottom_pane::settings_ui::rows::StyledText;
use crate::bottom_pane::settings_ui::toggle;
use crate::colors;

use super::{
    UpdateSettingsInit,
    UpdateSettingsView,
    UpdateSettingsViewContentOnly,
    UpdateSettingsViewContentOnlyMut,
    UpdateSettingsViewFramed,
    UpdateSettingsViewFramedMut,
    UpdateSharedState,
};

impl UpdateSettingsView {
    pub(super) const PANEL_TITLE: &'static str = "Upgrade";
    pub(super) const FIELD_COUNT: usize = 3;

    pub fn new(init: UpdateSettingsInit) -> Self {
        let UpdateSettingsInit {
            app_event_tx,
            ticket,
            current_version,
            auto_enabled,
            command,
            command_display,
            manual_instructions,
            shared,
        } = init;
        Self {
            app_event_tx,
            ticket,
            field: 0,
            is_complete: false,
            auto_enabled,
            shared,
            current_version,
            command,
            command_display,
            manual_instructions,
        }
    }

    pub(super) fn current_state(&self) -> UpdateSharedState {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn version_summary(&self, state: &UpdateSharedState) -> String {
        if state.checking {
            "checking for updates".to_string()
        } else if let Some(err) = state.error.as_deref() {
            format!("check failed: {err}")
        } else if let Some(latest) = state.latest_version.as_deref() {
            format!("{} -> {latest}", self.current_version)
        } else {
            format!("{} (up to date)", self.current_version)
        }
    }

    fn run_upgrade_value(&self, state: &UpdateSharedState) -> StyledText<'static> {
        if state.checking {
            StyledText::new("checking", Style::new().fg(colors::warning()))
        } else if state.error.is_some() {
            StyledText::new("blocked", Style::new().fg(colors::error()))
        } else if state.latest_version.is_some() {
            StyledText::new("available", Style::new().fg(colors::success()))
        } else if self.command.is_some() {
            StyledText::new("up to date", Style::new().fg(colors::text_dim()))
        } else {
            StyledText::new("manual", Style::new().fg(colors::info()))
        }
    }

    pub(super) fn auto_upgrade_row(auto_enabled: bool) -> SettingsMenuRow<'static, usize> {
        SettingsMenuRow::new(1usize, "Automatic Upgrades")
            .with_value(toggle::enabled_word(auto_enabled))
            .with_detail(StyledText::new(
                "checks on launch",
                Style::new().fg(colors::text_dim()),
            ))
            .with_selected_hint("(press Enter/Space to toggle)")
    }

    pub(super) fn rows(&self) -> Vec<SettingsMenuRow<'static, usize>> {
        let state = self.current_state();
        vec![
            SettingsMenuRow::new(0usize, "Run Upgrade")
                .with_value(self.run_upgrade_value(&state))
                .with_detail(StyledText::new(
                    self.version_summary(&state),
                    Style::new().fg(colors::text_dim()),
                ))
                .with_selected_hint(if self.command.is_some() {
                    "(press Enter to start)"
                } else {
                    "(press Enter for instructions)"
                }),
            Self::auto_upgrade_row(self.auto_enabled),
            SettingsMenuRow::new(2usize, "Close"),
        ]
    }

    pub(crate) fn framed(&self) -> UpdateSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> UpdateSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> UpdateSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> UpdateSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}
