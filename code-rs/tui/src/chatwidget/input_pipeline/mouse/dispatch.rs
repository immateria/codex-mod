impl ChatWidget<'_> {
    pub(crate) fn handle_mouse_event(&mut self, mouse_event: crossterm::event::MouseEvent) {
        use crossterm::event::KeyModifiers;
        use crossterm::event::MouseEventKind;

        // Check if Shift is held - if so, let the terminal handle selection
        if mouse_event.modifiers.contains(KeyModifiers::SHIFT) {
            // Don't handle any mouse events when Shift is held
            // This allows the terminal's native text selection to work
            return;
        }

        // Settings overlay is modal: while visible, mouse input must be consumed
        // by the overlay layer and must not leak to global history/pane scrolling.
        if let Some(overlay) = self.settings.overlay.as_mut() {
            // Settings overlay covers the full screen - compute terminal area from layout
            let terminal_area = Rect {
                x: 0,
                y: 0,
                width: self.layout.last_frame_width.get(),
                height: self.layout.last_frame_height.get(),
            };
            let changed = overlay.handle_mouse_event(mouse_event, terminal_area);
            if changed {
                self.sync_limits_layout_mode_preference();
                self.request_redraw();
            }
            return;
        }

        let bottom_pane_area = self.layout.last_bottom_pane_area.get();
        let mouse_pos = (mouse_event.column, mouse_event.row);

        // Helper: check if mouse is inside bottom pane area
        let in_bottom_pane = mouse_event.row >= bottom_pane_area.y
            && mouse_event.row < bottom_pane_area.y + bottom_pane_area.height
            && mouse_event.column >= bottom_pane_area.x
            && mouse_event.column < bottom_pane_area.x + bottom_pane_area.width;

        match mouse_event.kind {
            MouseEventKind::ScrollUp => {
                // If scroll is in the bottom pane area, forward it there first
                if in_bottom_pane {
                    let (input_result, needs_redraw) = self.bottom_pane.handle_mouse_event(mouse_event, bottom_pane_area);
                    if needs_redraw {
                        self.process_mouse_input_result(input_result);
                        return;
                    }
                }
                // Otherwise, scroll the history
                layout_scroll::mouse_scroll(self, true)
            }
            MouseEventKind::ScrollDown => {
                // If scroll is in the bottom pane area, forward it there first
                if in_bottom_pane {
                    let (input_result, needs_redraw) = self.bottom_pane.handle_mouse_event(mouse_event, bottom_pane_area);
                    if needs_redraw {
                        self.process_mouse_input_result(input_result);
                        return;
                    }
                }
                // Otherwise, scroll the history
                layout_scroll::mouse_scroll(self, false)
            }
            // Horizontal scroll on status bars (mouse wheel left/right).
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                let delta: i32 = if matches!(mouse_event.kind, MouseEventKind::ScrollLeft) { -3 } else { 3 };
                let status_bar_area = self.layout.last_status_bar_area.get();
                let in_status_bar = mouse_event.row >= status_bar_area.y
                    && mouse_event.row < status_bar_area.y.saturating_add(status_bar_area.height)
                    && mouse_event.column >= status_bar_area.x
                    && mouse_event.column < status_bar_area.x.saturating_add(status_bar_area.width);
                if in_status_bar {
                    let cur = self.status_bar_hscroll.get() as i32;
                    self.status_bar_hscroll.set(cur.saturating_add(delta).max(0) as u16);
                    self.request_redraw();
                } else if in_bottom_pane {
                    let cur = self.bottom_status_hscroll.get() as i32;
                    self.bottom_status_hscroll.set(cur.saturating_add(delta).max(0) as u16);
                    self.request_redraw();
                }
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                // Record drag start position for status bar panning.
                let status_bar_area = self.layout.last_status_bar_area.get();
                let in_status_bar = mouse_event.row >= status_bar_area.y
                    && mouse_event.row < status_bar_area.y.saturating_add(status_bar_area.height)
                    && mouse_event.column >= status_bar_area.x
                    && mouse_event.column < status_bar_area.x.saturating_add(status_bar_area.width);
                if in_status_bar || in_bottom_pane {
                    self.status_bar_drag_col.set(Some(mouse_event.column));
                }
                // First check if click is inside the bottom pane area
                if in_bottom_pane {
                    // Forward click to bottom pane
                    let (input_result, needs_redraw) = self.bottom_pane.handle_mouse_event(mouse_event, bottom_pane_area);
                    if needs_redraw {
                        self.process_mouse_input_result(input_result);
                        return;
                    }
                }
                // Handle left click by checking clickable regions (header bar, etc.)
                self.handle_click(mouse_pos);
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                self.status_bar_drag_col.set(None);
            }
            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                if let Some(start_col) = self.status_bar_drag_col.get() {
                    let current_col = mouse_event.column;
                    let delta = start_col as i32 - current_col as i32;
                    if delta != 0 {
                        // Determine which bar was being dragged based on row.
                        let status_bar_area = self.layout.last_status_bar_area.get();
                        let in_status_bar = mouse_event.row < status_bar_area.y.saturating_add(status_bar_area.height);
                        if in_status_bar {
                            let cur = self.status_bar_hscroll.get() as i32;
                            self.status_bar_hscroll.set(cur.saturating_add(delta).max(0) as u16);
                        } else {
                            let cur = self.bottom_status_hscroll.get() as i32;
                            self.bottom_status_hscroll.set(cur.saturating_add(delta).max(0) as u16);
                        }
                        self.status_bar_drag_col.set(Some(current_col));
                        self.request_redraw();
                    }
                }
            }
            MouseEventKind::Moved => {
                let mut needs_redraw = false;
                if self.update_header_hover_state(mouse_pos) {
                    needs_redraw = true;
                }
                // Update hover state in bottom pane.
                if in_bottom_pane
                    && self.bottom_pane.update_hover(mouse_pos, bottom_pane_area)
                {
                    needs_redraw = true;
                }
                if needs_redraw {
                    self.request_redraw();
                }
            }
            _ => {
                // Ignore other mouse events for now
            }
        }
    }

    /// Process InputResult from mouse events (similar to key event handling).
    pub(in super::super) fn process_mouse_input_result(&mut self, input_result: InputResult) {
        match input_result {
            InputResult::Submitted(text) => {
                if let Some(pending) = self.pending_request_user_input.take() {
                    self.submit_request_user_input_answer(pending, text);
                    return;
                }
                self.pending_turn_origin = Some(TurnOrigin::User);
                let cleaned = Self::strip_context_sections(&text);
                self.last_user_message = (!cleaned.trim().is_empty()).then_some(cleaned);
                let user_message = self.parse_message_with_images(text);
                self.submit_user_message(user_message);
            }
            InputResult::Command(_cmd) => {
                // Command was dispatched at the App layer; request redraw.
                self.app_event_tx.send(AppEvent::RequestRedraw);
            }
            InputResult::None | InputResult::ScrollUp | InputResult::ScrollDown => {
                self.request_redraw();
            }
        }
    }
}
