use super::*;

use crossterm::event::{KeyEvent, KeyEventKind, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPane, BottomPaneView, ConditionalUpdate};
use crate::ui_interaction::redraw_if;

impl crate::bottom_pane::chrome_view::ChromeRenderable for AutoDriveSettingsView {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_framed(area, buf);
    }

    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_content_only(area, buf);
    }
}

impl crate::bottom_pane::chrome_view::ChromeMouseHandler for AutoDriveSettingsView {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_internal(mouse_event, area)
    }

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_content_only(mouse_event, area)
    }
}

impl<'a> BottomPaneView<'a> for AutoDriveSettingsView {
    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return ConditionalUpdate::NoRedraw;
        }

        redraw_if(self.handle_key_event_internal(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(
            self.framed_mut()
                .handle_mouse_event_direct(mouse_event, area),
        )
    }

    fn update_hover(&mut self, mouse_pos: (u16, u16), area: Rect) -> bool {
        self.update_hover_internal(mouse_pos, area)
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        redraw_if(self.handle_paste_internal(&text))
    }

    fn desired_height(&self, _width: u16) -> u16 {
        match &self.mode {
            AutoDriveSettingsMode::Main => 12,
            AutoDriveSettingsMode::RoutingList => 14,
            AutoDriveSettingsMode::RoutingEditor(_) => 16,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
    }

    fn update_status_text(&mut self, _text: String) -> ConditionalUpdate {
        ConditionalUpdate::NoRedraw
    }

    fn is_complete(&self) -> bool {
        self.closing
    }
}
