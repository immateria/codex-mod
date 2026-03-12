use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::MemoriesSettingsView;

use super::super::SettingsContent;

pub(crate) struct MemoriesSettingsContent {
    view: MemoriesSettingsView,
}

impl MemoriesSettingsContent {
    pub(crate) fn new(view: MemoriesSettingsView) -> Self {
        Self { view }
    }
}

impl SettingsContent for MemoriesSettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.content_only().render(area, buf);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.view.handle_key_event_direct(key);
        true
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view
            .content_only_mut()
            .handle_mouse_event_direct(mouse_event, area)
    }

    fn is_complete(&self) -> bool {
        self.view.is_view_complete()
    }
}
