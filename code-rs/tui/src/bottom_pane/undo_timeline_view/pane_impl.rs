use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::{BottomPane, CancellationEvent};
use crate::ui_interaction::redraw_if;

use super::UndoTimelineView;

impl<'a> BottomPaneView<'a> for UndoTimelineView {
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

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        self.is_complete = true;
        CancellationEvent::Handled
    }

    fn update_status_text(&mut self, _text: String) -> ConditionalUpdate {
        ConditionalUpdate::NoRedraw
    }

    fn desired_height(&self, _width: u16) -> u16 {
        24
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_direct(area, buf);
    }
}

