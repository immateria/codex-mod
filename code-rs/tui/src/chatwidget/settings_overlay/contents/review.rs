use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPaneView, ReviewSettingsView};
use code_core::config_types::ReasoningEffort;

use super::super::SettingsContent;

pub(crate) struct ReviewSettingsContent {
    view: ReviewSettingsView,
}

impl ReviewSettingsContent {
    pub(crate) fn new(view: ReviewSettingsView) -> Self {
        Self { view }
    }

    pub(crate) fn update_review_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_review_model(model, effort);
    }

    pub(crate) fn set_review_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_review_use_chat_model(use_chat);
    }

    pub(crate) fn update_review_resolve_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_review_resolve_model(model, effort);
    }

    pub(crate) fn set_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_review_resolve_use_chat_model(use_chat);
    }

    pub(crate) fn update_auto_review_model(&mut self, model: String, effort: ReasoningEffort) {
        self.view.set_auto_review_model(model, effort);
    }

    pub(crate) fn set_auto_review_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_auto_review_use_chat_model(use_chat);
    }

    pub(crate) fn update_auto_review_resolve_model(
        &mut self,
        model: String,
        effort: ReasoningEffort,
    ) {
        self.view.set_auto_review_resolve_model(model, effort);
    }

    pub(crate) fn set_auto_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        self.view.set_auto_review_resolve_use_chat_model(use_chat);
    }

    pub(crate) fn set_review_followups(&mut self, attempts: u32) {
        self.view.set_review_followups(attempts);
    }

    pub(crate) fn set_auto_review_followups(&mut self, attempts: u32) {
        self.view.set_auto_review_followups(attempts);
    }
}

impl SettingsContent for ReviewSettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render(area, buf);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.view.handle_key_event_direct(key);
        true
    }

    fn is_complete(&self) -> bool {
        self.view.is_complete()
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view.handle_mouse_event_direct(mouse_event, area)
    }
}
