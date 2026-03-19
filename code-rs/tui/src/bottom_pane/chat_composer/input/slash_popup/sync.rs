use super::*;

pub(super) fn sync_command_popup_inner(view: &mut ChatComposer) {
    let first_line = view.textarea.text().lines().next().unwrap_or("");
    let input_starts_with_slash = first_line.starts_with('/');
    // Keep the slash popup only while the cursor is within the command head
    // (before the first space). This allows @-file completion for arguments
    // in commands like "/plan" and "/solve".
    let in_slash_head = view.is_cursor_in_slash_command_head();
    match &mut view.active_popup {
        ActivePopup::Command(popup) => {
            if input_starts_with_slash && in_slash_head {
                popup.on_composer_text_change(first_line.to_string());
            } else {
                view.active_popup = ActivePopup::None;
            }
        }
        _ => {
            if input_starts_with_slash && in_slash_head {
                let mut command_popup = CommandPopup::new_with_filter(view.using_chatgpt_auth);
                if !view.custom_prompts.is_empty() {
                    command_popup.set_prompts(view.custom_prompts.clone());
                }
                if !view.subagent_commands.is_empty() {
                    command_popup.set_subagent_commands(view.subagent_commands.clone());
                }
                command_popup.on_composer_text_change(first_line.to_string());
                view.active_popup = ActivePopup::Command(command_popup);
                // Notify app: composer expanded due to slash popup
                view.app_event_tx
                    .send(crate::app_event::AppEvent::ComposerExpanded);
            }
        }
    }
}

pub(super) fn set_custom_prompts_inner(view: &mut ChatComposer, prompts: Vec<CustomPrompt>) {
    view.custom_prompts = prompts;
    if let ActivePopup::Command(popup) = &mut view.active_popup {
        popup.set_prompts(view.custom_prompts.clone());
    }
}

