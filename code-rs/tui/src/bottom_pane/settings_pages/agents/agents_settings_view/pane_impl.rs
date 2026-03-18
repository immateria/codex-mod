use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::BottomPane;
use crate::ui_interaction::redraw_if;

use super::model::{Focus, SubagentEditorView};

impl<'a> BottomPaneView<'a> for SubagentEditorView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_internal(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_internal(key_event))
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        match self.focus {
            Focus::Name => self.name_field.handle_paste(text),
            Focus::Instructions => self.orch_field.handle_paste(text),
            _ => {}
        }
        ConditionalUpdate::NeedsRedraw
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.desired_height_inner(width)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_inner(area, buf);
    }
}

