use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
    MouseButton,
    MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;

use crate::bottom_pane::{BottomPane, BottomPaneView, CancellationEvent, ConditionalUpdate};

use super::RequestUserInputView;

impl BottomPaneView<'_> for RequestUserInputView {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn handle_key_event(&mut self, _pane: &mut BottomPane<'_>, key_event: KeyEvent) {
        if !self.should_handle_key_event(&key_event) {
            return;
        }

        if self.submitting {
            return;
        }

        match key_event.code {
            KeyCode::Esc => {
                if self.is_mcp_access_prompt() && self.current_has_options() {
                    let has_cancel = self.select_current_option_by_label("Cancel");
                    if !has_cancel {
                        let _ = self.select_current_option_by_label("Deny");
                    }
                    self.submit();
                } else {
                    // Close this UI and fall back to the composer for manual input.
                    self.complete = true;
                }
            }
            KeyCode::Enter => {
                self.go_next_or_submit();
            }
            KeyCode::PageUp => {
                self.current_idx = self.current_idx.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.current_idx =
                    (self.current_idx + 1).min(self.question_count().saturating_sub(1));
            }
            KeyCode::Up => {
                if self.current_has_options() {
                    self.move_selection(true);
                }
            }
            KeyCode::Down => {
                if self.current_has_options() {
                    self.move_selection(false);
                }
            }
            KeyCode::Backspace => {
                if self.current_accepts_freeform() {
                    self.pop_freeform_char();
                }
            }
            KeyCode::Char(ch) => {
                if self.current_accepts_freeform()
                    && !key_event
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                {
                    self.push_freeform_char(ch);
                }
            }
            _ => {}
        }
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'_>,
        mouse_event: MouseEvent,
        area: Rect,
    ) -> ConditionalUpdate {
        if self.submitting {
            return ConditionalUpdate::NoRedraw;
        }

        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let x = mouse_event.column;
                let y = mouse_event.row;
                let Some(hit_idx) = self.option_hit_test(area, x, y) else {
                    return ConditionalUpdate::NoRedraw;
                };
                let Some(answer) = self.current_answer_mut() else {
                    return ConditionalUpdate::NoRedraw;
                };
                let prev = answer.option_state.selected_idx;
                answer.option_state.selected_idx = Some(hit_idx);
                if prev == Some(hit_idx) {
                    self.go_next_or_submit();
                }
                ConditionalUpdate::NeedsRedraw
            }
            MouseEventKind::ScrollUp => {
                if self.current_has_options() {
                    self.move_selection(true);
                    ConditionalUpdate::NeedsRedraw
                } else {
                    ConditionalUpdate::NoRedraw
                }
            }
            MouseEventKind::ScrollDown => {
                if self.current_has_options() {
                    self.move_selection(false);
                    ConditionalUpdate::NeedsRedraw
                } else {
                    ConditionalUpdate::NoRedraw
                }
            }
            _ => ConditionalUpdate::NoRedraw,
        }
    }

    fn update_hover(&mut self, mouse_pos: (u16, u16), area: Rect) -> bool {
        if self.submitting {
            return false;
        }
        if !self.current_has_options() {
            if let Some(answer) = self.current_answer_mut()
                && answer.hover_option_idx.take().is_some()
            {
                return true;
            }
            return false;
        }
        let (x, y) = mouse_pos;
        let hover_idx = self.option_hit_test(area, x, y);
        let Some(answer) = self.current_answer_mut() else {
            return false;
        };
        if answer.hover_option_idx != hover_idx {
            answer.hover_option_idx = hover_idx;
            return true;
        }
        false
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'_>) -> CancellationEvent {
        if self.submitting {
            return CancellationEvent::Handled;
        }
        self.complete = true;
        CancellationEvent::Handled
    }

    fn handle_paste(&mut self, text: String) -> ConditionalUpdate {
        if !self.current_accepts_freeform() {
            return ConditionalUpdate::NoRedraw;
        }
        if text.is_empty() {
            return ConditionalUpdate::NoRedraw;
        }
        if let Some(answer) = self.current_answer_mut() {
            answer.freeform.push_str(&text);
        }
        ConditionalUpdate::NeedsRedraw
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.desired_height_for_width(width)
    }

    fn render(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        self.render_direct(area, buf);
    }
}
