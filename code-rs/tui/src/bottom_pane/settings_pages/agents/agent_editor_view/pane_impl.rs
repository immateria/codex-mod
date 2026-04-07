use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPaneView, ConditionalUpdate};
use crate::bottom_pane::BottomPane;
use crate::ui_interaction::redraw_if;

use super::AgentEditorView;

impl<'a> BottomPaneView<'a> for AgentEditorView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_internal(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_internal(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        _area: Rect,
    ) -> ConditionalUpdate {
        match mouse_event.kind {
            MouseEventKind::ScrollUp => {
                let cur = self.scroll_offset.get();
                if cur > 0 {
                    self.scroll_offset.set(cur.saturating_sub(3));
                    return ConditionalUpdate::NeedsRedraw;
                }
            }
            MouseEventKind::ScrollDown => {
                let cur = self.scroll_offset.get();
                self.scroll_offset.set(cur.saturating_add(3));
                return ConditionalUpdate::NeedsRedraw;
            }
            _ => {}
        }
        ConditionalUpdate::NoRedraw
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        if self.paste_into_current_field(&text) {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn desired_height(&self, width: u16) -> u16 {
        let content_width = width.saturating_sub(4).max(1);
        let layout = self.layout(content_width);
        u16::try_from(layout.lines.len())
            .unwrap_or(u16::MAX)
            .saturating_add(2)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_inner(area, buf);
    }
}

