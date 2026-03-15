use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPane, BottomPaneView, ConditionalUpdate};

use super::SettingsOverviewView;

impl<'a> BottomPaneView<'a> for SettingsOverviewView {
    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        let visible_rows = self.viewport_rows.get().max(1);
        if self.process_key_event(key_event, visible_rows) {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        // Ignore move events when the terminal doesn't support them.
        if matches!(mouse_event.kind, MouseEventKind::Moved) && area.width == 0 {
            return ConditionalUpdate::NoRedraw;
        }
        if self.handle_mouse_event_direct(mouse_event, area) {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let visible = self.rows.len().clamp(1, 12) as u16;
        // border (2) + header (1) + visible rows + footer (1)
        2 + 1 + visible + 1
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_framed(area, buf);
    }
}

