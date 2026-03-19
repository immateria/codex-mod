use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use super::layout::compute_composer_rect;
use super::panes::auto_coordinator::AutoCoordinatorView;
use super::{ActiveViewKind, BottomPane, CancellationEvent, ChatComposer, ConditionalUpdate, InputResult};

impl<'a> BottomPane<'a> {
    /// Forward a mouse event to the active view if present, or to the composer.
    /// Returns (InputResult, bool) where bool indicates if a redraw is needed.
    pub fn handle_mouse_event(
        &mut self,
        mouse_event: crossterm::event::MouseEvent,
        area: Rect,
    ) -> (InputResult, bool) {
        // If there's an active view, forward to it
        if let Some(mut view) = self.active_view.take() {
            let kind = self.active_view_kind;
            let view_rect = self.compute_active_view_rect(area, view.as_ref());
            let result = view.handle_mouse_event(
                self,
                mouse_event,
                view_rect.unwrap_or_else(|| compute_composer_rect(area, self.top_spacer_enabled)),
            );
            let is_complete = view.is_complete();

            // Only restore the local view if the callback did not already
            // claim active view ownership by mutating `self.active_view`.
            if !self.callback_claimed_active_view(kind) {
                if !is_complete {
                    self.active_view = Some(view);
                    self.active_view_kind = kind;
                } else {
                    self.clear_active_view_state();
                }
            }

            let needs_redraw = matches!(result, ConditionalUpdate::NeedsRedraw) || is_complete;
            return (InputResult::None, needs_redraw);
        }

        // No active view - forward to the composer for popup handling
        let (input_result, needs_redraw) = self.composer.handle_mouse_event(mouse_event, area);
        if needs_redraw {
            self.request_redraw();
        }
        (input_result, needs_redraw)
    }

    /// Update hover state in the active view.
    /// Returns true if a redraw is needed.
    pub fn update_hover(&mut self, mouse_pos: (u16, u16), area: Rect) -> bool {
        let view_rect = self
            .active_view
            .as_ref()
            .and_then(|view| self.compute_active_view_rect(area, view.as_ref()))
            .unwrap_or_else(|| compute_composer_rect(area, self.top_spacer_enabled));
        if let Some(view) = self.active_view.as_mut() {
            view.update_hover(mouse_pos, view_rect)
        } else {
            false
        }
    }

    /// Forward a key event to the active view or the composer.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> InputResult {
        if let Some(mut view) = self.active_view.take() {
            let kind = self.active_view_kind;
            if matches!(kind, ActiveViewKind::AutoCoordinator) {
                let consumed = if let Some(auto_view) = view
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<AutoCoordinatorView>())
                {
                    auto_view.handle_active_key_event(self, key_event)
                } else {
                    let update = view.handle_key_event_with_result(self, key_event);
                    let is_complete = view.is_complete();
                    if !self.callback_claimed_active_view(kind) {
                        if !is_complete {
                            self.active_view = Some(view);
                            self.active_view_kind = kind;
                        } else {
                            self.clear_active_view_state();
                        }
                    }
                    if matches!(update, ConditionalUpdate::NeedsRedraw) || is_complete {
                        self.request_redraw();
                    }
                    return InputResult::None;
                };

                if !self.callback_claimed_active_view(kind) {
                    if !view.is_complete() {
                        self.active_view = Some(view);
                        self.active_view_kind = kind;
                    } else {
                        self.clear_active_view_state();
                    }
                }

                if consumed {
                    self.request_redraw();
                    // When Auto Drive hides the composer, Up/Down should keep
                    // scrolling chat history instead of becoming dead keys.
                    if matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                        match key_event.code {
                            KeyCode::Up => return InputResult::ScrollUp,
                            KeyCode::Down => return InputResult::ScrollDown,
                            _ => {}
                        }
                    }
                    return InputResult::None;
                }

                return self.handle_composer_key_event(key_event);
            }

            let update = view.handle_key_event_with_result(self, key_event);
            let is_complete = view.is_complete();
            if !self.callback_claimed_active_view(kind) {
                if !is_complete {
                    self.active_view = Some(view);
                    self.active_view_kind = kind;
                } else {
                    self.clear_active_view_state();
                }
            }
            let needs_redraw = matches!(update, ConditionalUpdate::NeedsRedraw) || is_complete;
            if needs_redraw {
                // Don't create a status view - keep composer visible.
                // Debounce view navigation redraws to reduce render thrash.
                self.request_redraw();
            }

            InputResult::None
        } else {
            self.handle_composer_key_event(key_event)
        }
    }

    fn handle_composer_key_event(&mut self, key_event: KeyEvent) -> InputResult {
        let (input_result, needs_redraw) = self.composer.handle_key_event(key_event);
        if needs_redraw {
            // Route input updates through the app's debounced redraw path so typing
            // doesn't attempt a full-screen redraw per key.
            self.request_redraw();
        }
        if self.composer.is_in_paste_burst() {
            self.request_redraw_in(ChatComposer::recommended_paste_flush_delay());
        }
        input_result
    }

    /// Attempt to navigate history upwards from the composer. Returns true if consumed.
    pub(crate) fn try_history_up(&mut self) -> bool {
        let consumed = self.composer.try_history_up();
        if consumed {
            self.request_redraw();
        }
        consumed
    }

    /// Attempt to navigate history downwards from the composer. Returns true if consumed.
    pub(crate) fn try_history_down(&mut self) -> bool {
        let consumed = self.composer.try_history_down();
        if consumed {
            self.request_redraw();
        }
        consumed
    }

    /// Returns true if the composer is currently browsing history.
    pub(crate) fn history_is_browsing(&self) -> bool {
        self.composer.history_is_browsing()
    }

    /// After a chat scroll-up, make the next Down key scroll chat instead of moving within input.
    pub(crate) fn mark_next_down_scrolls_history(&mut self) {
        self.composer.mark_next_down_scrolls_history();
    }

    /// Handle Ctrl-C in the bottom pane. If a modal view is active it gets a
    /// chance to consume the event (e.g. to dismiss itself).
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        let kind = self.active_view_kind;
        let mut view = match self.active_view.take() {
            Some(view) => view,
            None => return CancellationEvent::Ignored,
        };

        let event = view.on_ctrl_c(self);
        match event {
            CancellationEvent::Handled => {
                if !self.callback_claimed_active_view(kind) {
                    if !view.is_complete() {
                        self.active_view = Some(view);
                        self.active_view_kind = kind;
                    } else {
                        self.clear_active_view_state();
                    }
                }
            }
            CancellationEvent::Ignored => {
                if !self.callback_claimed_active_view(kind) {
                    if !view.is_complete() {
                        self.active_view = Some(view);
                        self.active_view_kind = kind;
                    } else {
                        self.clear_active_view_state();
                    }
                }
            }
        }
        event
    }

    pub fn handle_paste(&mut self, pasted: String) {
        if let Some(mut view) = self.active_view.take() {
            let kind = self.active_view_kind;
            let update = view.handle_paste_with_composer(&mut self.composer, pasted);
            if !view.is_complete() {
                self.active_view = Some(view);
                self.active_view_kind = kind;
            } else {
                self.clear_active_view_state();
            }
            if matches!(update, ConditionalUpdate::NeedsRedraw) {
                self.request_redraw();
            }
            return;
        }
        let needs_redraw = self.composer.handle_paste(pasted);
        if needs_redraw {
            // Large pastes may arrive as bursts; coalesce paints
            self.request_redraw();
        }
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        self.composer.insert_str(text);
        self.request_redraw();
    }

    pub(crate) fn set_composer_text(&mut self, text: String) {
        self.composer.set_text_content(text);
        self.request_redraw();
    }

    /// Clear the composer text and reset transient composer state.
    pub(crate) fn clear_composer(&mut self) {
        self.composer.clear_text();
        self.request_redraw();
    }

    /// Attempt to close the file-search popup if visible. Returns true if closed.
    pub(crate) fn close_file_popup_if_active(&mut self) -> bool {
        let closed = self.composer.close_file_popup_if_active();
        if closed {
            self.request_redraw();
        }
        closed
    }

    pub(crate) fn file_popup_visible(&self) -> bool {
        self.composer.file_popup_visible()
    }
}

