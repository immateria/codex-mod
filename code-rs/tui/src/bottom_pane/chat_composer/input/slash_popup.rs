use super::*;

enum SlashPopupSelection {
    Builtin(SlashCommand),
    UserPrompt(Option<String>),
    Subagent(Option<String>),
}

impl ChatComposer {
    fn starts_with_slash_name(line: &str, name: &str) -> bool {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix('/') else {
            return false;
        };
        let Some(suffix) = rest.strip_prefix(name) else {
            return false;
        };
        suffix.is_empty() || suffix.starts_with(char::is_whitespace)
    }

    fn apply_subagent_completion(&mut self, name: &str) -> bool {
        let first_line = self.textarea.text().lines().next().unwrap_or("");
        if Self::starts_with_slash_name(first_line, name) {
            return true;
        }
        self.textarea.set_text(&format!("/{name} "));
        let new_cursor = self.textarea.text().len();
        self.textarea.set_cursor(new_cursor);
        false
    }

    pub(crate) fn confirm_slash_popup_selection(&mut self) -> (InputResult, bool) {
        let selection = {
            let ActivePopup::Command(popup) = &mut self.active_popup else {
                return (InputResult::None, false);
            };

            let Some(sel) = popup.selected_item() else {
                return (InputResult::None, false);
            };

            match sel {
                CommandItem::Builtin(cmd) => SlashPopupSelection::Builtin(cmd),
                CommandItem::UserPrompt(idx) => {
                    SlashPopupSelection::UserPrompt(popup.prompt(idx).map(|p| p.content.clone()))
                }
                CommandItem::Subagent(i) => {
                    SlashPopupSelection::Subagent(popup.subagent_name(i).map(str::to_owned))
                }
            }
        };

        let command_text = self.textarea.text().to_string();

        match selection {
            SlashPopupSelection::Builtin(cmd) => {
                self.history.record_local_submission(&command_text);
                if cmd.is_prompt_expanding() {
                    self.app_event_tx.send(crate::app_event::AppEvent::PrepareAgents);
                }
                self.app_event_tx
                    .send(crate::app_event::AppEvent::DispatchCommand(cmd, command_text));
                self.textarea.set_text("");
                self.active_popup = ActivePopup::None;
                (InputResult::Command(cmd), true)
            }
            SlashPopupSelection::UserPrompt(prompt_content) => {
                self.history.record_local_submission(&command_text);
                self.textarea.set_text("");
                self.active_popup = ActivePopup::None;
                if let Some(contents) = prompt_content {
                    (InputResult::Submitted(contents), true)
                } else {
                    (InputResult::None, true)
                }
            }
            SlashPopupSelection::Subagent(subagent_name) => {
                if let Some(name) = subagent_name.as_deref() {
                    if self.apply_subagent_completion(name) {
                        self.active_popup = ActivePopup::None;
                        return self.handle_key_event_without_popup(
                            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                        );
                    }
                    return (InputResult::None, true);
                }
                (InputResult::None, true)
            }
        }
    }

    /// Handle a key event when the slash-command popup is visible.
    pub(super) fn handle_key_event_with_slash_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        match key_event {
            // Allow Shift+Up to navigate history even when slash popup is active.
            KeyEvent { code: KeyCode::Up, modifiers, .. } => {
                let ActivePopup::Command(popup) = &mut self.active_popup else {
                    return (InputResult::None, false);
                };
                if modifiers.contains(KeyModifiers::SHIFT) {
                    return self.handle_key_event_without_popup(key_event);
                }
                // If there are 0 or 1 items, let Up behave normally (cursor/history/scroll)
                if popup.match_count() <= 1 {
                    return self.handle_key_event_without_popup(key_event);
                }
                popup.move_up();
                (InputResult::None, true)
            }
            // Allow Shift+Down to navigate history even when slash popup is active.
            KeyEvent { code: KeyCode::Down, modifiers, .. } => {
                let ActivePopup::Command(popup) = &mut self.active_popup else {
                    return (InputResult::None, false);
                };
                if modifiers.contains(KeyModifiers::SHIFT) {
                    return self.handle_key_event_without_popup(key_event);
                }
                // If there are 0 or 1 items, let Down behave normally (cursor/history/scroll)
                if popup.match_count() <= 1 {
                    return self.handle_key_event_without_popup(key_event);
                }
                popup.move_down();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // Dismiss the slash popup; keep the current input untouched.
                self.active_popup = ActivePopup::None;
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                let Some(selection) = ({
                    let ActivePopup::Command(popup) = &mut self.active_popup else {
                        return (InputResult::None, false);
                    };
                    popup.selected_item().map(|sel| match sel {
                        CommandItem::Builtin(cmd) => SlashPopupSelection::Builtin(cmd),
                        CommandItem::UserPrompt(idx) => SlashPopupSelection::UserPrompt(
                            popup.prompt(idx).map(|p| p.name.clone()),
                        ),
                        CommandItem::Subagent(i) => SlashPopupSelection::Subagent(
                            popup.subagent_name(i).map(str::to_owned),
                        ),
                    })
                }) else {
                    return (InputResult::None, true);
                };

                {
                    let first_line = self.textarea.text().lines().next().unwrap_or("");

                    match selection {
                        SlashPopupSelection::Builtin(cmd) => {
                            let starts_with_cmd =
                                Self::starts_with_slash_name(first_line, cmd.command());
                            if !starts_with_cmd {
                                self.textarea.set_text(&format!("/{} ", cmd.command()));
                            }
                        }
                        SlashPopupSelection::UserPrompt(prompt_name) => {
                            if let Some(name) = prompt_name {
                                let trimmed = first_line.trim_start();
                                let wants_prefixed = trimmed.starts_with(&format!(
                                    "/{PROMPTS_CMD_PREFIX}:{name}"
                                )) || trimmed.starts_with(&format!("/{PROMPTS_CMD_PREFIX}:"));
                                let target = if wants_prefixed {
                                    format!("/{PROMPTS_CMD_PREFIX}:{name} ")
                                } else {
                                    format!("/{name} ")
                                };
                                let starts_with_cmd = trimmed.starts_with(target.trim_end());
                                if !starts_with_cmd {
                                    self.textarea.set_text(target.as_str());
                                }
                            }
                        }
                        SlashPopupSelection::Subagent(subagent_name) => {
                            if let Some(name) = subagent_name.as_deref() {
                                let _ = self.apply_subagent_completion(name);
                            }
                        }
                    }
                    // After completing, place the cursor at the end of the
                    // slash command so the user can immediately type args.
                    let new_cursor = self.textarea.text().len();
                    self.textarea.set_cursor(new_cursor);
                }
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let result = self.confirm_slash_popup_selection();
                if result.1 {
                    result
                } else {
                    self.handle_key_event_without_popup(key_event)
                }
            }
            input => self.handle_input_basic(input),
        }
    }

    /// Synchronize `self.command_popup` with the current text in the
    /// textarea. This must be called after every modification that can change
    /// the text so the popup is shown/updated/hidden as appropriate.
    pub(crate) fn sync_command_popup(&mut self) {
        let first_line = self.textarea.text().lines().next().unwrap_or("");
        let input_starts_with_slash = first_line.starts_with('/');
        // Keep the slash popup only while the cursor is within the command head
        // (before the first space). This allows @-file completion for arguments
        // in commands like "/plan" and "/solve".
        let in_slash_head = self.is_cursor_in_slash_command_head();
        match &mut self.active_popup {
            ActivePopup::Command(popup) => {
                if input_starts_with_slash && in_slash_head {
                    popup.on_composer_text_change(first_line.to_string());
                } else {
                    self.active_popup = ActivePopup::None;
                }
            }
            _ => {
                if input_starts_with_slash && in_slash_head {
                    let mut command_popup = CommandPopup::new_with_filter(self.using_chatgpt_auth);
                    if !self.custom_prompts.is_empty() {
                        command_popup.set_prompts(self.custom_prompts.clone());
                    }
                    if !self.subagent_commands.is_empty() {
                        command_popup.set_subagent_commands(self.subagent_commands.clone());
                    }
                    command_popup.on_composer_text_change(first_line.to_string());
                    self.active_popup = ActivePopup::Command(command_popup);
                    // Notify app: composer expanded due to slash popup
                    self.app_event_tx.send(crate::app_event::AppEvent::ComposerExpanded);
                }
            }
        }
    }

    pub(crate) fn set_custom_prompts(&mut self, prompts: Vec<CustomPrompt>) {
        self.custom_prompts = prompts;
        if let ActivePopup::Command(popup) = &mut self.active_popup {
            popup.set_prompts(self.custom_prompts.clone());
        }
    }

}
