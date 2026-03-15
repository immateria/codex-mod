use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::{BottomPane, CancellationEvent};
use crate::ui_interaction::redraw_if;

use super::StatusLineSetupView;

impl<'a> BottomPaneView<'a> for StatusLineSetupView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.process_key_event(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.process_key_event(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_mouse_event_direct(mouse_event, area))
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        self.cancel();
        CancellationEvent::Handled
    }

    fn desired_height(&self, _width: u16) -> u16 {
        u16::try_from(self.choices_for_active_lane().len())
            .unwrap_or(u16::MAX)
            .saturating_add(12)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_direct(area, buf);
    }
}

