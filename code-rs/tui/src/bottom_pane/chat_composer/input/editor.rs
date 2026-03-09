use super::*;

impl ChatComposer {
    pub(crate) fn on_history_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
    ) -> bool {
        let Some(text) = self.history.on_entry_response(log_id, offset, entry) else {
            return false;
        };
        self.textarea.set_text(&text);
        self.textarea.set_cursor(0);
        self.resync_popups();
        true
    }
    pub fn set_text_content(&mut self, text: String) {
        self.textarea.set_text(&text);
        *self.textarea_state.borrow_mut() = TextAreaState::default();
        if !text.is_empty() {
            self.typed_anything = true;
        }
        self.sync_command_popup();
        self.sync_file_search_popup();
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        self.textarea.insert_str(text);
        self.typed_anything = true; // Mark that user has interacted via programmatic insertion
        self.sync_command_popup();
        self.sync_file_search_popup();
    }

    pub(crate) fn text(&self) -> &str {
        self.textarea.text()
    }

    pub(super) fn handle_key_event_without_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        match key_event {
            KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } if self.is_empty() => {
                self.app_event_tx.send(crate::app_event::AppEvent::ExitRequest);
                (InputResult::None, true)
            }
            // -------------------------------------------------------------
            // Shift+Tab — rotate access preset (Read Only → Write with Approval → Full Access)
            // -------------------------------------------------------------
            KeyEvent { code: KeyCode::BackTab, .. } => {
                self.app_event_tx.send(crate::app_event::AppEvent::CycleAccessMode);
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.auto_drive_active && self.has_focus => {
                self
                    .app_event_tx
                    .send(crate::app_event::AppEvent::CycleAutoDriveVariant);
                (InputResult::None, true)
            }
            // -------------------------------------------------------------
            // Tab-press file search when not using @ or ./ and not in slash cmd
            // -------------------------------------------------------------
            KeyEvent { code: KeyCode::Tab, .. } => {
                // Suppress Tab completion only while the cursor is within the
                // slash command head (before the first space). Allow Tab-based
                // file search in the arguments of /plan, /solve, etc.
                if self.is_cursor_in_slash_command_head() {
                    return (InputResult::None, false);
                }

                // If already showing a file popup, let the dedicated handler manage Tab.
                if matches!(self.active_popup, ActivePopup::File(_)) {
                    return (InputResult::None, false);
                }

                // If an @ token is present or token starts with ./, rely on auto-popup.
                if Self::current_completion_token(&self.textarea).is_some() {
                    return (InputResult::None, false);
                }

                // Use the generic token under cursor for a one-off search.
                if let Some(tok) = Self::current_generic_token(&self.textarea)
                    && !tok.is_empty() {
                        self.pending_tab_file_query = Some(tok.clone());
                        self.app_event_tx.send(crate::app_event::AppEvent::StartFileSearch(tok));
                        // Do not show a popup yet; wait for results and only
                        // show if there are matches to avoid flicker.
                        return (InputResult::None, true);
                    }
                (InputResult::None, false)
            }
            // -------------------------------------------------------------
            // Handle Esc key — leave to App-level policy (clear/stop/backtrack)
            // -------------------------------------------------------------
            KeyEvent { code: KeyCode::Esc, .. } => {
                // Do nothing here so App can implement global Esc ordering.
                (InputResult::None, false)
            }
            // -------------------------------------------------------------
            // Up/Down key handling - check modifiers to determine action
            // -------------------------------------------------------------
            KeyEvent {
                code: code @ (KeyCode::Up | KeyCode::Down),
                modifiers,
                ..
            } => {
                // Check if Shift is held for history navigation
                if modifiers.contains(KeyModifiers::SHIFT) {
                    // History navigation with Shift+Up/Down
                    if self
                        .history
                        .should_handle_navigation(self.textarea.text(), self.textarea.cursor())
                    {
                        let replace_text = match code {
                            KeyCode::Up => self
                                .history
                                .navigate_up(self.textarea.text(), &self.app_event_tx),
                            KeyCode::Down => self.history.navigate_down(&self.app_event_tx),
                            _ => unreachable!("outer match restricts code to Up/Down"),
                        };
                        if let Some(text) = replace_text {
                            self.textarea.set_text(&text);
                            self.textarea.set_cursor(0);
                            return (InputResult::None, true);
                        }
                    }
                    // If history navigation didn't happen, just ignore the key
                    (InputResult::None, false)
                } else {
                    // No Shift modifier — move cursor within the input first.
                    // Only when already at the top-left/bottom-right should Up/Down scroll chat.
                    if self.textarea.is_empty() {
                        return match code {
                            KeyCode::Up => (InputResult::ScrollUp, false),
                            KeyCode::Down => (InputResult::ScrollDown, false),
                            _ => unreachable!("outer match restricts code to Up/Down"),
                        };
                    }

                    let before = self.textarea.cursor();
                    let len = self.textarea.text().len();
                    match code {
                        KeyCode::Up => {
                            if before == 0 {
                                (InputResult::ScrollUp, false)
                            } else {
                                // Move up a visual/logical line; if already on first line, TextArea moves to start.
                                self.textarea.input(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                                (InputResult::None, true)
                            }
                        }
                        KeyCode::Down => {
                            // If sticky is set, prefer chat ScrollDown once
                            if self.next_down_scrolls_history {
                                self.next_down_scrolls_history = false;
                                return (InputResult::ScrollDown, false);
                            }
                            if before == len {
                                (InputResult::ScrollDown, false)
                            } else {
                                // Move down a visual/logical line; if already on last line, TextArea moves to end.
                                self.textarea.input(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                                (InputResult::None, true)
                            }
                        }
                        _ => unreachable!("outer match restricts code to Up/Down"),
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                if self.handle_backslash_continuation() {
                    return (InputResult::None, true);
                }
                let original_text = self.textarea.text().to_string();
                let first_line = original_text.lines().next().unwrap_or("");
                if let Some((name, rest)) = parse_slash_name(first_line)
                    && rest.is_empty()
                    && let Some(cmd) = Self::resolve_builtin_slash_command(name)
                {
                    if cmd.is_prompt_expanding() {
                        self.app_event_tx.send(crate::app_event::AppEvent::PrepareAgents);
                    }
                    self.history.record_local_submission(&original_text);
                    self.app_event_tx
                        .send(crate::app_event::AppEvent::DispatchCommand(cmd, original_text));
                    self.textarea.set_text("");
                    self.active_popup = ActivePopup::None;
                    return (InputResult::Command(cmd), true);
                }

                let mut text = original_text.clone();
                self.textarea.set_text("");

                // Replace all pending pastes in the text
                for (placeholder, actual) in &self.pending_pastes {
                    if text.contains(placeholder) {
                        text = text.replace(placeholder, actual);
                    }
                }
                self.pending_pastes.clear();

                if text.is_empty() {
                    (InputResult::None, true)
                } else {
                    if let Some((name, _rest)) = parse_slash_name(first_line)
                        && let Some(cmd) = Self::resolve_builtin_slash_command(name)
                        && cmd.is_prompt_expanding()
                    {
                        self.app_event_tx.send(crate::app_event::AppEvent::PrepareAgents);
                    }

                    self.history.record_local_submission(&original_text);
                    (InputResult::Submitted(text), true)
                }
            }
            input => self.handle_input_basic(input),
        }
    }

    pub(super) fn handle_backslash_continuation(&mut self) -> bool {
        let text = self.textarea.text();
        let mut iter = text.char_indices().rev();
        let Some((last_idx, last_char)) = iter.next() else {
            return false;
        };
        if matches!(last_char, ' ' | '\t') {
            return false;
        }
        if last_char != '\\' {
            return false;
        }

        let trailing_backslashes = text[..last_idx]
            .chars()
            .rev()
            .take_while(|c| *c == '\\')
            .count()
            + 1;
        if trailing_backslashes % 2 == 0 {
            return false;
        }

        let line_start = text[..last_idx].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let line_before = &text[line_start..last_idx];
        let indent_end = line_before
            .bytes()
            .take_while(|&byte| byte == b' ' || byte == b'\t')
            .count();
        let indentation = &line_before[..indent_end];
        let replacement = if indentation.is_empty() {
            String::from("\n")
        } else {
            format!("\n{indentation}")
        };
        let backslash_end = last_idx + '\\'.len_utf8();
        self.textarea.replace_range(last_idx..backslash_end, &replacement);

        self.history.reset_navigation();
        self.post_paste_space_guard = None;
        if !self.pending_pastes.is_empty() {
            self.pending_pastes
                .retain(|(placeholder, _)| self.textarea.text().contains(placeholder));
        }
        self.typed_anything = true;
        true
    }

    /// Handle generic Input events that modify the textarea content.
    pub(super) fn handle_input_basic(&mut self, input: KeyEvent) -> (InputResult, bool) {
        if self.should_suppress_post_paste_space(&input) {
            return (InputResult::None, false);
        }

        // Special handling for backspace on placeholders
        if let KeyEvent {
            code: KeyCode::Backspace,
            ..
        } = input
            && self.try_remove_placeholder_at_cursor() {
                // Text was modified, reset history navigation
                self.history.reset_navigation();
                return (InputResult::None, true);
            }

        let text_before = self.textarea.text().to_string();

        // Normal input handling
        self.textarea.input(input);
        let text_after = self.textarea.text();
        let changed = text_before != text_after;

        if changed
            || self
                .post_paste_space_guard
                .as_ref()
                .map(|guard| self.textarea.cursor() != guard.cursor_pos)
                .unwrap_or(false)
        {
            self.post_paste_space_guard = None;
        }

        // If text changed, reset history navigation state
        if changed {
            self.history.reset_navigation();
            if !text_after.is_empty() { self.typed_anything = true; }
        }

        // Check if any placeholders were removed and remove their corresponding pending pastes
        if !self.pending_pastes.is_empty() {
            self.pending_pastes
                .retain(|(placeholder, _)| text_after.contains(placeholder));
        }

        (InputResult::None, true)
    }


}
