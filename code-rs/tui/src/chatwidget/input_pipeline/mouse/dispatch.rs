impl ChatWidget<'_> {
    /// Drag threshold in columns — movement beyond this invalidates a click.
    const CLICK_DRAG_THRESHOLD: u16 = 3;

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
                let in_status_bar = Self::pos_in_rect(mouse_pos, status_bar_area);
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
                // Record mouse-down position; defer the click to mouse-up so
                // that drags don't accidentally trigger clickable actions.
                self.mouse_down_pos.set(Some(mouse_pos));
                self.mouse_drag_exceeded.set(false);

                // Forward to bottom pane for its own drag/selection tracking.
                if in_bottom_pane {
                    let (input_result, needs_redraw) = self.bottom_pane.handle_mouse_event(mouse_event, bottom_pane_area);
                    if needs_redraw {
                        self.process_mouse_input_result(input_result);
                    }
                }
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                let was_drag = self.mouse_drag_exceeded.get();
                let down_pos = self.mouse_down_pos.take();
                self.mouse_drag_exceeded.set(false);

                // Only fire click if the mouse didn't move beyond threshold.
                if !was_drag {
                    if let Some(pos) = down_pos {
                        if in_bottom_pane {
                            // If the down was in the bottom pane, let it handle
                            // the up. Otherwise check header clickable regions.
                            // (Bottom pane clicks are already handled in Down.)
                        }
                        self.handle_click(pos);
                    }
                }
            }
            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                if let Some((start_col, _start_row)) = self.mouse_down_pos.get() {
                    let current_col = mouse_event.column;
                    let col_delta = (start_col as i32 - current_col as i32).unsigned_abs() as u16;

                    // Once movement exceeds threshold, mark as drag and start
                    // applying horizontal scroll to status bars.
                    if col_delta > Self::CLICK_DRAG_THRESHOLD {
                        self.mouse_drag_exceeded.set(true);
                    }

                    if self.mouse_drag_exceeded.get() {
                        let delta = start_col as i32 - current_col as i32;
                        if delta != 0 {
                            let status_bar_area = self.layout.last_status_bar_area.get();
                            let in_status_bar = Self::pos_in_rect(mouse_pos, status_bar_area);
                            if in_status_bar {
                                let cur = self.status_bar_hscroll.get() as i32;
                                self.status_bar_hscroll.set(cur.saturating_add(delta).max(0) as u16);
                            } else if in_bottom_pane {
                                let cur = self.bottom_status_hscroll.get() as i32;
                                self.bottom_status_hscroll.set(cur.saturating_add(delta).max(0) as u16);
                            }
                            // Update the anchor so subsequent drag events are
                            // relative to the current position.
                            self.mouse_down_pos.set(Some(mouse_pos));
                            self.request_redraw();
                        }
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

    /// Check whether a (col, row) position falls inside a `Rect`.
    fn pos_in_rect(pos: (u16, u16), rect: Rect) -> bool {
        pos.0 >= rect.x
            && pos.0 < rect.x.saturating_add(rect.width)
            && pos.1 >= rect.y
            && pos.1 < rect.y.saturating_add(rect.height)
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
