use super::*;

pub(super) fn handle_key_event_with_slash_popup_inner(
    view: &mut ChatComposer,
    key_event: KeyEvent,
) -> (InputResult, bool) {
    match key_event {
        // Allow Shift+Up to navigate history even when slash popup is active.
        KeyEvent { code: KeyCode::Up, modifiers, .. } => {
            let ActivePopup::Command(popup) = &mut view.active_popup else {
                return (InputResult::None, false);
            };
            if modifiers.contains(KeyModifiers::SHIFT) {
                return view.handle_key_event_without_popup(key_event);
            }
            // If there are 0 or 1 items, let Up behave normally (cursor/history/scroll)
            if popup.match_count() <= 1 {
                return view.handle_key_event_without_popup(key_event);
            }
            popup.move_up();
            (InputResult::None, true)
        }
        // Allow Shift+Down to navigate history even when slash popup is active.
        KeyEvent { code: KeyCode::Down, modifiers, .. } => {
            let ActivePopup::Command(popup) = &mut view.active_popup else {
                return (InputResult::None, false);
            };
            if modifiers.contains(KeyModifiers::SHIFT) {
                return view.handle_key_event_without_popup(key_event);
            }
            // If there are 0 or 1 items, let Down behave normally (cursor/history/scroll)
            if popup.match_count() <= 1 {
                return view.handle_key_event_without_popup(key_event);
            }
            popup.move_down();
            (InputResult::None, true)
        }
        KeyEvent { code: KeyCode::Esc, .. } => {
            // Dismiss the slash popup; keep the current input untouched.
            view.active_popup = ActivePopup::None;
            (InputResult::None, true)
        }
        KeyEvent { code: KeyCode::Tab, .. } => {
            let Some(selection) = ({
                let ActivePopup::Command(popup) = &mut view.active_popup else {
                    return (InputResult::None, false);
                };
                popup.selected_item().map(|sel| match sel {
                    CommandItem::Builtin(cmd) => SlashPopupSelection::Builtin(cmd),
                    CommandItem::UserPrompt(idx) => SlashPopupSelection::UserPrompt(
                        popup.prompt(idx).map(|p| p.name.clone()),
                    ),
                    CommandItem::Subagent(i) => {
                        SlashPopupSelection::Subagent(popup.subagent_name(i).map(str::to_owned))
                    }
                })
            }) else {
                return (InputResult::None, true);
            };

            {
                let first_line = view.textarea.text().lines().next().unwrap_or("");

                match selection {
                    SlashPopupSelection::Builtin(cmd) => {
                        let starts_with_cmd =
                            super::completion::starts_with_slash_name(first_line, cmd.command());
                        if !starts_with_cmd {
                            view.textarea.set_text(&format!("/{} ", cmd.command()));
                        }
                    }
                    SlashPopupSelection::UserPrompt(prompt_name) => {
                        if let Some(name) = prompt_name {
                            let trimmed = first_line.trim_start();
                            let prefix_with_name = format!("/{PROMPTS_CMD_PREFIX}:{name}");
                            let prefix_bare = format!("/{PROMPTS_CMD_PREFIX}:");
                            let wants_prefixed = trimmed.starts_with(&prefix_with_name)
                                || trimmed.starts_with(&prefix_bare);
                            let target = if wants_prefixed {
                                format!("/{PROMPTS_CMD_PREFIX}:{name} ")
                            } else {
                                format!("/{name} ")
                            };
                            let starts_with_cmd = trimmed.starts_with(target.trim_end());
                            if !starts_with_cmd {
                                view.textarea.set_text(target.as_str());
                            }
                        }
                    }
                    SlashPopupSelection::Subagent(subagent_name) => {
                        if let Some(name) = subagent_name.as_deref() {
                            let _ = super::completion::apply_subagent_completion(view, name);
                        }
                    }
                }
                // After completing, place the cursor at the end of the
                // slash command so the user can immediately type args.
                let new_cursor = view.textarea.text().len();
                view.textarea.set_cursor(new_cursor);
            }
            (InputResult::None, true)
        }
        KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            ..
        } => {
            let result = view.confirm_slash_popup_selection();
            if result.1 {
                result
            } else {
                view.handle_key_event_without_popup(key_event)
            }
        }
        input => view.handle_input_basic(input),
    }
}

