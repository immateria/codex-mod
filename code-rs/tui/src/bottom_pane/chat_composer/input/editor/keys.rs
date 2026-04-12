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
        // Shift+Tab or Alt+A — rotate access preset
        // (Read Only → Write with Approval → Full Access)
        // Alt+A is the fallback for Termux/mobile where Shift+Tab is unreliable.
        // -------------------------------------------------------------
        KeyEvent { code: KeyCode::BackTab, .. }
        | KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::ALT,
            kind: KeyEventKind::Press,
            ..
        } => {
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
        // Ctrl+P / Ctrl+N — readline-style history navigation (Termux fallback
        // for Shift+Up/Down which may not transmit on virtual keyboards)
        // -------------------------------------------------------------
        KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            ..
        } => {
            if view
                .history
                .should_handle_navigation(view.textarea.text(), view.textarea.cursor())
                && let Some(text) = view.history.navigate_up(view.textarea.text(), &view.app_event_tx)
            {
                view.textarea.set_text(&text);
                view.textarea.set_cursor(0);
                return (InputResult::None, true);
            }
            (InputResult::None, true)
        }
        KeyEvent {
            code: KeyCode::Char('n'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            ..
        } => {
            if view
                .history
                .should_handle_navigation(view.textarea.text(), view.textarea.cursor())
                && let Some(text) = view.history.navigate_down(&view.app_event_tx)
            {
                view.textarea.set_text(&text);
                view.textarea.set_cursor(0);
                return (InputResult::None, true);
            }
            (InputResult::None, true)
        }
        // -------------------------------------------------------------
        // Up/Down key handling
        // Plain Up/Down  → input history recall (shell-like)
        // Shift+Up/Down  → viewport scroll
        // -------------------------------------------------------------
        KeyEvent {
            code: code @ (KeyCode::Up | KeyCode::Down),
            modifiers,
            ..
        } => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                // Shift held → viewport scroll
                return match code {
                    KeyCode::Up => (InputResult::ScrollUp, false),
                    KeyCode::Down => (InputResult::ScrollDown, false),
                    _ => unreachable!("outer match restricts code to Up/Down"),
                };
            }

            // Empty composer → history only, no-op when nothing to recall
            if view.textarea.is_empty() {
                let ok = match code {
                    KeyCode::Up => view.try_history_up(),
                    KeyCode::Down => view.try_history_down(),
                    _ => unreachable!("outer match restricts code to Up/Down"),
                };
                return (InputResult::None, ok);
            }

            // Non-empty composer → cursor movement, history at boundaries
            let cursor = view.textarea.cursor();
            let len = view.textarea.text().len();
            match code {
                KeyCode::Up => {
                    if cursor == 0 {
                        (InputResult::None, view.try_history_up())
                    } else {
                        view.textarea
                            .input(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                        (InputResult::None, true)
                    }
                }
                KeyCode::Down => {
                    if cursor == len {
                        (InputResult::None, view.try_history_down())
                    } else {
                        view.textarea
                            .input(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                        (InputResult::None, true)
                    }
                }
                _ => unreachable!("outer match restricts code to Up/Down"),
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
            let original_text = view.textarea.text().to_owned();
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

