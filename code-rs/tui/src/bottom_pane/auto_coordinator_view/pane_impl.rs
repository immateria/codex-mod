use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPane, BottomPaneView, ChatComposer, ConditionalUpdate};
use crate::ui_interaction::redraw_if;

use super::*;

impl<'a> BottomPaneView<'a> for AutoCoordinatorView {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

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

    fn desired_height(&self, width: u16) -> u16 {
        let AutoCoordinatorViewModel::Active(model) = &self.model;
        self.estimated_height_active(width, model, 0)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }

        let AutoCoordinatorViewModel::Active(model) = &self.model;
        self.render_active(area, buf, model, None);
    }

    fn render_with_composer(&self, area: Rect, buf: &mut Buffer, composer: &ChatComposer) {
        if area.height == 0 {
            return;
        }

        let AutoCoordinatorViewModel::Active(model) = &self.model;
        self.render_active(area, buf, model, Some(composer));
    }

    fn update_status_text(&mut self, text: String) -> ConditionalUpdate {
        if self.update_status_message(text) {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn handle_paste_with_composer(
        &mut self,
        composer: &mut ChatComposer,
        pasted: String,
    ) -> ConditionalUpdate {
        if composer.handle_paste(pasted) {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
