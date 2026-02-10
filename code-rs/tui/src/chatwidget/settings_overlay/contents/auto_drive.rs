use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::AutoDriveSettingsView;
use code_core::config_types::ReasoningEffort;

use super::super::SettingsContent;

pub(crate) struct AutoDriveSettingsContent {
    view: AutoDriveSettingsView,
}

impl AutoDriveSettingsContent {
    pub(crate) fn new(view: AutoDriveSettingsView) -> Self {
        Self { view }
    }

    pub(crate) fn update_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_model(model, effort);
    }

    pub(crate) fn set_use_chat_model(
        &mut self,
        use_chat: bool,
        model: String,
        effort: ReasoningEffort,
    ) {
        self.view.set_use_chat_model(use_chat, model, effort);
    }
}

impl SettingsContent for AutoDriveSettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render_without_frame(area, buf);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.view.handle_key_event_direct(key);
        true
    }

    fn is_complete(&self) -> bool {
        self.view.is_view_complete()
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view.handle_mouse_event_direct(mouse_event, area)
    }
}
