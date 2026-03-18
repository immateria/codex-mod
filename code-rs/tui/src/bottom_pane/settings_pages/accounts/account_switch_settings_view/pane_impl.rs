use super::{AccountSwitchSettingsView, ViewMode};

use crate::bottom_pane::chrome::ChromeMode;
use crate::bottom_pane::{BottomPane, BottomPaneView, ConditionalUpdate};
use crate::ui_interaction::redraw_if;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

impl crate::bottom_pane::chrome_view::ChromeRenderable for AccountSwitchSettingsView {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::Framed, area, buf);
    }

    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_in_chrome(ChromeMode::ContentOnly, area, buf);
    }
}

impl crate::bottom_pane::chrome_view::ChromeMouseHandler for AccountSwitchSettingsView {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::Framed, mouse_event, area)
    }

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_in_chrome(ChromeMode::ContentOnly, mouse_event, area)
    }
}

impl<'a> BottomPaneView<'a> for AccountSwitchSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_direct(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_direct(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.framed_mut().handle_mouse_event_direct(mouse_event, area))
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        match self.view_mode {
            ViewMode::Main => 18,
            ViewMode::ConfirmStoreChange { .. } => 10,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
    }
}
