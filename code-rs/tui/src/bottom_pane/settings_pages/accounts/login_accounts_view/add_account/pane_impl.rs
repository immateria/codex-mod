use super::LoginAddAccountView;

use crate::bottom_pane::{BottomPane, BottomPaneView, ConditionalUpdate};
use crate::ui_interaction::redraw_if;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

impl<'a> BottomPaneView<'a> for LoginAddAccountView {
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
        self.state.borrow().is_complete()
    }

    fn desired_height(&self, _width: u16) -> u16 {
        u16::try_from(self.state.borrow().desired_height()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.state.borrow().render(area, buf);
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        self.state.borrow_mut().handle_paste(text)
    }
}

