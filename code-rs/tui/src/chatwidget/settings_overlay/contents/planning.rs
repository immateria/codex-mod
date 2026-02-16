use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPaneView, PlanningSettingsView};
use code_core::config_types::ReasoningEffort;

use super::super::SettingsContent;

pub(crate) struct PlanningSettingsContent {
    view: PlanningSettingsView,
}

impl PlanningSettingsContent {
    pub(crate) fn new(view: PlanningSettingsView) -> Self {
        Self { view }
    }

    pub(crate) fn update_planning_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_planning_model(model, effort);
    }

    pub(crate) fn set_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_use_chat_model(use_chat);
    }
}

impl SettingsContent for PlanningSettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render(area, buf);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.view.handle_key_event_direct(key)
    }

    fn is_complete(&self) -> bool {
        self.view.is_complete()
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view.handle_mouse_event_direct(mouse_event, area)
    }
}
