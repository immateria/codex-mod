use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::skills_settings_view::SkillsSettingsView;

use super::super::SettingsContent;

pub(crate) struct SkillsSettingsContent {
    view: SkillsSettingsView,
}

impl SkillsSettingsContent {
    pub(crate) fn new(view: SkillsSettingsView) -> Self {
        Self { view }
    }
}

impl SettingsContent for SkillsSettingsContent {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.view.render(area, buf);
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.view.handle_key_event_direct(key)
    }

    fn is_complete(&self) -> bool {
        self.view.is_complete()
    }

    fn handle_paste(&mut self, text: String) -> bool {
        self.view.handle_paste_direct(text)
    }

    fn handle_mouse(&mut self, mouse_event: MouseEvent, area: Rect) -> bool {
        self.view.handle_mouse_event_direct(mouse_event, area)
    }
}
