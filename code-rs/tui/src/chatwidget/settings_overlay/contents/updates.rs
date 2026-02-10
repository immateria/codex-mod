use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::UpdateSettingsView;

use super::super::SettingsContent;

pub(crate) struct UpdatesSettingsContent {
    view: UpdateSettingsView,
}

impl UpdatesSettingsContent {
    pub(crate) fn new(view: UpdateSettingsView) -> Self {
        Self { view }
    }
}

impl SettingsContent for UpdatesSettingsContent {
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
