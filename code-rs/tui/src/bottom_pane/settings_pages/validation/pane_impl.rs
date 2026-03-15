use super::*;

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPane, BottomPaneView, ConditionalUpdate};
use crate::ui_interaction::redraw_if;

impl crate::bottom_pane::chrome_view::ChromeRenderable for ValidationSettingsView {
    fn render_in_framed_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_framed(area, buf);
    }

    fn render_in_content_only_chrome(&self, area: Rect, buf: &mut Buffer) {
        self.render_content_only(area, buf);
    }
}

impl crate::bottom_pane::chrome_view::ChromeMouseHandler for ValidationSettingsView {
    fn handle_mouse_event_direct_in_framed_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_internal(None, mouse_event, area)
    }

    fn handle_mouse_event_direct_in_content_only_chrome(
        &mut self,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> bool {
        self.handle_mouse_event_direct_content_only(mouse_event, area)
    }
}

impl<'a> BottomPaneView<'a> for ValidationSettingsView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_internal(Some(pane), key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_internal(Some(pane), key_event))
    }

    fn handle_mouse_event(
        &mut self,
        pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_mouse_event_internal(Some(pane), mouse_event, area))
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let base = 6u16; // header + footer + padding
        let rows = self
            .groups
            .len()
            .saturating_add(self.tools.len())
            .saturating_add(2); // section headers and spacing
        let rows = u16::try_from(rows).unwrap_or(u16::MAX);
        base.saturating_add(rows.min(18))
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.framed().render(area, buf);
    }
}

