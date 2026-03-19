use super::*;

pub(super) fn handle_key_event_without_popup(
    view: &mut ChatComposer,
    key_event: KeyEvent,
) -> (InputResult, bool) {
    match key_event {
        KeyEvent {
            code: KeyCode::Char('d'),
            modifiers: crossterm::event::KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            ..
        } if view.is_empty() => {
            view.app_event_tx.send(crate::app_event::AppEvent::ExitRequest);
            (InputResult::None, true)
        }
        // -------------------------------------------------------------
        // Shift+Tab — rotate access preset (Read Only → Write with Approval → Full Access)
        // -------------------------------------------------------------
        KeyEvent { code: KeyCode::BackTab, .. } => {
            view.app_event_tx.send(crate::app_event::AppEvent::CycleAccessMode);
            (InputResult::None, true)
        }
        KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            ..
        } if view.auto_drive_active && view.has_focus => {
            view.app_event_tx
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
            if view.is_cursor_in_slash_command_head() {
                return (InputResult::None, false);
            }

            // If already showing a file popup, let the dedicated handler manage Tab.
            if matches!(view.active_popup, ActivePopup::File(_)) {
                return (InputResult::None, false);
            }

            // If an @ token is present or token starts with ./, rely on auto-popup.
            if ChatComposer::current_completion_token(&view.textarea).is_some() {
                return (InputResult::None, false);
            }

            // Use the generic token under cursor for a one-off search.
            if let Some(tok) = ChatComposer::current_generic_token(&view.textarea) && !tok.is_empty()
            {
                view.pending_tab_file_query = Some(tok.clone());
                view.app_event_tx
                    .send(crate::app_event::AppEvent::StartFileSearch(tok));
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
                if view
                    .history
                    .should_handle_navigation(view.textarea.text(), view.textarea.cursor())
                {
                    let replace_text = match code {
                        KeyCode::Up => view
                            .history
                            .navigate_up(view.textarea.text(), &view.app_event_tx),
                        KeyCode::Down => view.history.navigate_down(&view.app_event_tx),
                        _ => unreachable!("outer match restricts code to Up/Down"),
                    };
                    if let Some(text) = replace_text {
                        view.textarea.set_text(&text);
                        view.textarea.set_cursor(0);
                        return (InputResult::None, true);
                    }
                }
                // If history navigation didn't happen, just ignore the key
                (InputResult::None, false)
            } else {
                // No Shift modifier — move cursor within the input first.
                // Only when already at the top-left/bottom-right should Up/Down scroll chat.
                if view.textarea.is_empty() {
                    return match code {
                        KeyCode::Up => (InputResult::ScrollUp, false),
                        KeyCode::Down => (InputResult::ScrollDown, false),
                        _ => unreachable!("outer match restricts code to Up/Down"),
                    };
                }

                let before = view.textarea.cursor();
                let len = view.textarea.text().len();
                match code {
                    KeyCode::Up => {
                        if before == 0 {
                            (InputResult::ScrollUp, false)
                        } else {
                            // Move up a visual/logical line; if already on first line, TextArea moves to start.
                            view.textarea
                                .input(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                            (InputResult::None, true)
                        }
                    }
                    KeyCode::Down => {
                        // If sticky is set, prefer chat ScrollDown once
                        if view.next_down_scrolls_history {
                            view.next_down_scrolls_history = false;
                            return (InputResult::ScrollDown, false);
                        }
                        if before == len {
                            (InputResult::ScrollDown, false)
                        } else {
                            // Move down a visual/logical line; if already on last line, TextArea moves to end.
                            view.textarea
                                .input(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
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
            if view.handle_backslash_continuation() {
                return (InputResult::None, true);
            }
            let original_text = view.textarea.text().to_string();
            let first_line = original_text.lines().next().unwrap_or("");
            if let Some((name, rest)) = parse_slash_name(first_line)
                && rest.is_empty()
                && let Some(cmd) = ChatComposer::resolve_builtin_slash_command(name)
            {
                if cmd.is_prompt_expanding() {
                    view.app_event_tx.send(crate::app_event::AppEvent::PrepareAgents);
                }
                view.history.record_local_submission(&original_text);
                view.app_event_tx
                    .send(crate::app_event::AppEvent::DispatchCommand(cmd, original_text));
                view.textarea.set_text("");
                view.active_popup = ActivePopup::None;
                return (InputResult::Command(cmd), true);
            }

            let mut text = original_text.clone();
            view.textarea.set_text("");

            // Replace all pending pastes in the text
            for (placeholder, actual) in &view.pending_pastes {
                if text.contains(placeholder) {
                    text = text.replace(placeholder, actual);
                }
            }
            view.pending_pastes.clear();

            if text.is_empty() {
                (InputResult::None, true)
            } else {
                if let Some((name, _rest)) = parse_slash_name(first_line)
                    && let Some(cmd) = ChatComposer::resolve_builtin_slash_command(name)
                    && cmd.is_prompt_expanding()
                {
                    view.app_event_tx.send(crate::app_event::AppEvent::PrepareAgents);
                }

                view.history.record_local_submission(&original_text);
                (InputResult::Submitted(text), true)
            }
        }
        input => view.handle_input_basic(input),
    }
}

