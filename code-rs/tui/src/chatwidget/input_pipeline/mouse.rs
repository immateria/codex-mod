use super::*;

impl ChatWidget<'_> {
    #[allow(dead_code)]
    pub(crate) fn set_mouse_status_message(&mut self, message: &str) {
        self.bottom_pane.update_status_text(message.to_string());
    }

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
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
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

    pub(in super::super) fn handle_click(&mut self, pos: (u16, u16)) {
        let (x, y) = pos;
        
        // Check clickable regions from last render and find matching action
        let action_opt: Option<ClickableAction> = {
            let regions = self.clickable_regions.borrow();
            
            regions.iter().find_map(|region| {
                // Check if click is inside this region
                if x >= region.rect.x
                    && x < region.rect.x + region.rect.width
                    && y >= region.rect.y
                    && y < region.rect.y + region.rect.height
                {
                    Some(region.action.clone())
                } else {
                    None
                }
            })
        };
        
        // Execute the action after dropping the borrow
        if let Some(action) = action_opt {
            match action {
                ClickableAction::ShowModelSelector => {
                    // Open model selector with empty args (opens selector UI)
                    self.handle_model_command(String::new());
                }
                ClickableAction::ShowShellSelector => {
                    self.app_event_tx.send(AppEvent::ShowShellSelector);
                }
                ClickableAction::ShowReasoningSelector => {
                    // Cycle through reasoning efforts
                    use code_core::config_types::ReasoningEffort;
                    let current = self.config.model_reasoning_effort;
                    let next = match current {
                        ReasoningEffort::None => ReasoningEffort::Minimal,
                        ReasoningEffort::Minimal => ReasoningEffort::Low,
                        ReasoningEffort::Low => ReasoningEffort::Medium,
                        ReasoningEffort::Medium => ReasoningEffort::High,
                        ReasoningEffort::High => ReasoningEffort::XHigh,
                        ReasoningEffort::XHigh => ReasoningEffort::None,
                    };
                    self.set_reasoning_effort(next);
                }
                ClickableAction::ShowNetworkSettings => {
                    self.ensure_settings_overlay_section(crate::bottom_pane::SettingsSection::Network);
                }
                ClickableAction::ExecuteCommand(cmd) => {
                    // Parse and dispatch the slash command
                    let trimmed = cmd.trim_start_matches('/').trim();
                    if let Ok(slash_cmd) = trimmed.parse::<SlashCommand>() {
                        self.app_event_tx.send(AppEvent::DispatchCommand(slash_cmd, cmd));
                    }
                }
            }
        }
    }

    fn update_header_hover_state(&mut self, pos: (u16, u16)) -> bool {
        let (x, y) = pos;
        let hovered = {
            let regions = self.clickable_regions.borrow();
            regions.iter().find_map(|region| {
                if x >= region.rect.x
                    && x < region.rect.x + region.rect.width
                    && y >= region.rect.y
                    && y < region.rect.y + region.rect.height
                {
                    Some(region.action.clone())
                } else {
                    None
                }
            })
        };
        let mut current = self.hovered_clickable_action.borrow_mut();
        if *current == hovered {
            false
        } else {
            *current = hovered;
            true
        }
    }
}
