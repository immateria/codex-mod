impl ChatWidget<'_> {
    /// Drag threshold in columns — movement beyond this invalidates a click.
    const CLICK_DRAG_THRESHOLD: u16 = 3;

    /// Toggle mouse capture on/off (Ctrl+M). Uses the same `mouse_capture_paused`
    /// Cell as Shift+click so both mechanisms stay in sync.
    pub(crate) fn toggle_mouse_capture(&mut self) {
        if self.mouse_capture_paused.get() {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::event::EnableMouseCapture
            );
            self.mouse_capture_paused.set(false);
        } else {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::event::DisableMouseCapture
            );
            self.mouse_capture_paused.set(true);
            self.bottom_pane.flash_footer_notice(
                "Mouse capture off — press Ctrl+M or any key to resume".into(),
            );
        }
    }

    pub(crate) fn handle_mouse_event(&mut self, mouse_event: crossterm::event::MouseEvent) {
        use crossterm::event::KeyModifiers;
        use crossterm::event::MouseEventKind;

        // When Shift is held, temporarily disable mouse capture so the
        // terminal handles native text selection. Capture is re-enabled
        // on the next key event (see handle_key_event).
        if mouse_event.modifiers.contains(KeyModifiers::SHIFT) {
            if !self.mouse_capture_paused.get() {
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::event::DisableMouseCapture
                );
                self.mouse_capture_paused.set(true);
                self.bottom_pane.flash_footer_notice(
                    "Selection mode — select text, then press any key to resume".into(),
                );
                self.request_redraw();
            }
            return;
        }

        // Auto-recovery: if we receive a non-Shift mouse event while
        // capture was paused, re-enable capture. This prevents permanent
        // "stuck" states on Android/Termux where Shift can be triggered
        // accidentally by virtual keyboard or touch gestures.
        if self.mouse_capture_paused.get() {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::event::EnableMouseCapture
            );
            self.mouse_capture_paused.set(false);
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
            if overlay.close_requested.get() {
                self.close_settings_overlay();
                return;
            }
            if changed {
                self.sync_limits_layout_mode_preference();
                self.request_redraw();
            }
            return;
        }

        // Help overlay is modal when visible: consume all mouse input.
        if self.help.overlay.is_some() {
            self.handle_help_mouse(mouse_event);
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

                // Check if click is on the history scrollbar thumb.
                if self.try_begin_scrollbar_drag(mouse_event.column, mouse_event.row) {
                    return;
                }

                // Forward to bottom pane for its own drag/selection tracking.
                if in_bottom_pane {
                    let (input_result, needs_redraw) = self.bottom_pane.handle_mouse_event(mouse_event, bottom_pane_area);
                    if needs_redraw {
                        self.process_mouse_input_result(input_result);
                    }
                }
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                let was_scrollbar_drag = self.scrollbar_drag_offset.get().is_some();
                self.scrollbar_drag_offset.set(None);

                let was_drag = self.mouse_drag_exceeded.get();
                let down_pos = self.mouse_down_pos.take();
                self.mouse_drag_exceeded.set(false);

                if was_scrollbar_drag {
                    return;
                }

                // Only fire click if the mouse didn't move beyond threshold.
                if !was_drag
                    && let Some(pos) = down_pos
                {
                    if in_bottom_pane {
                        // If the down was in the bottom pane, let it handle
                        // the up. Otherwise check header clickable regions.
                        // (Bottom pane clicks are already handled in Down.)
                    }
                    self.handle_click(pos);
                }
            }
            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                // If dragging the history scrollbar thumb, update scroll.
                if self.scrollbar_drag_offset.get().is_some() {
                    self.handle_scrollbar_drag(mouse_event.row);
                    return;
                }

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
                if self.try_handle_help_query(text.trim()) {
                    return;
                }
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

    /// Handle mouse events when the help overlay is active.
    fn handle_help_mouse(&mut self, mouse_event: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        use crate::chatwidget::internals::state::HelpTab;

        let pos = (mouse_event.column, mouse_event.row);
        let window_rect = self.help.window_rect.get();

        match mouse_event.kind {
            MouseEventKind::Moved => {
                let mut changed = false;

                let close_rect = self.help.close_rect.get();
                let close_hov = close_rect.width > 0
                    && close_rect.height > 0
                    && Self::pos_in_rect(pos, close_rect);
                if self.help.close_hovered.replace(close_hov) != close_hov {
                    changed = true;
                }

                let prev_rect = self.help.prev_arrow_rect.get();
                let prev_hov = prev_rect.width > 0 && Self::pos_in_rect(pos, prev_rect);
                if self.help.prev_hovered.replace(prev_hov) != prev_hov {
                    changed = true;
                }

                let next_rect = self.help.next_arrow_rect.get();
                let next_hov = next_rect.width > 0 && Self::pos_in_rect(pos, next_rect);
                if self.help.next_hovered.replace(next_hov) != next_hov {
                    changed = true;
                }

                let tab_hov = {
                    let rects = self.help.tab_rects.borrow();
                    rects.iter().position(|r| Self::pos_in_rect(pos, *r))
                };
                if self.help.hovered_tab.replace(tab_hov) != tab_hov {
                    changed = true;
                }

                if changed {
                    self.request_redraw();
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Close button
                if Self::pos_in_rect(pos, self.help.close_rect.get()) {
                    self.help.overlay = None;
                    self.request_redraw();
                    return;
                }
                // Click outside window closes overlay
                if !Self::pos_in_rect(pos, window_rect) {
                    self.help.overlay = None;
                    self.request_redraw();
                    return;
                }
                // Prev/next arrow clicks
                if Self::pos_in_rect(pos, self.help.prev_arrow_rect.get()) {
                    if let Some(ref mut overlay) = self.help.overlay {
                        overlay.active_tab = overlay.active_tab.prev();
                    }
                    self.request_redraw();
                    return;
                }
                if Self::pos_in_rect(pos, self.help.next_arrow_rect.get()) {
                    if let Some(ref mut overlay) = self.help.overlay {
                        overlay.active_tab = overlay.active_tab.next();
                    }
                    self.request_redraw();
                    return;
                }
                // Tab clicks
                let tab_rects = self.help.tab_rects.borrow();
                for (i, rect) in tab_rects.iter().enumerate() {
                    if Self::pos_in_rect(pos, *rect) {
                        if let Some(&tab) = HelpTab::ALL.get(i) {
                            drop(tab_rects);
                            if let Some(ref mut overlay) = self.help.overlay {
                                overlay.active_tab = tab;
                            }
                            self.request_redraw();
                            return;
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if Self::pos_in_rect(pos, window_rect) {
                    if let Some(ref mut overlay) = self.help.overlay {
                        let s = overlay.scroll_mut();
                        *s = s.saturating_sub(3);
                    }
                    self.request_redraw();
                }
            }
            MouseEventKind::ScrollDown => {
                if Self::pos_in_rect(pos, window_rect) {
                    if let Some(ref mut overlay) = self.help.overlay {
                        let visible_rows = self.help.body_visible_rows.get() as usize;
                        let max_off = overlay.lines().len().saturating_sub(visible_rows.max(1));
                        let cur = overlay.scroll() as usize;
                        let next = cur.saturating_add(3).min(max_off);
                        *overlay.scroll_mut() = next as u16;
                    }
                    self.request_redraw();
                }
            }
            _ => {
                // Consume all other mouse events while overlay is active
            }
        }
    }
}
