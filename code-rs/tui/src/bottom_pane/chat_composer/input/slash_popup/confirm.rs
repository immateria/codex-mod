use super::*;

fn canonicalize_slash_command_text_for_builtin(original: &str, cmd: SlashCommand) -> String {
    let trimmed = original.trim_start();
    let Some(rest) = trimmed.strip_prefix('/') else {
        return format!("/{}", cmd.command());
    };

    let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let suffix = &rest[name_end..];
    format!("/{}{}", cmd.command(), suffix)
}

pub(super) fn confirm_slash_popup_selection_inner(view: &mut ChatComposer) -> (InputResult, bool) {
    let selection = {
        let ActivePopup::Command(popup) = &mut view.active_popup else {
            return (InputResult::None, false);
        };

        // Auto-select first match when nothing is explicitly selected
        // (e.g., user typed partial command like /set and pressed Enter without navigating)
        let sel = popup.selected_item()
            .or_else(|| popup.filtered_items().into_iter().next());
        let Some(sel) = sel else {
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

    let original_text = view.textarea.text().to_owned();

    match selection {
        SlashPopupSelection::Builtin(cmd) => {
            let command_text = canonicalize_slash_command_text_for_builtin(&original_text, cmd);
            view.history.record_local_submission(&command_text);
            if cmd.is_prompt_expanding() {
                view.app_event_tx
                    .send(crate::app_event::AppEvent::PrepareAgents);
            }
            view.app_event_tx
                .send(crate::app_event::AppEvent::DispatchCommand(cmd, command_text));
            view.textarea.set_text("");
            view.active_popup = ActivePopup::None;
            (InputResult::Command(cmd), true)
        }
        SlashPopupSelection::UserPrompt(prompt_content) => {
            view.history.record_local_submission(&original_text);
            view.textarea.set_text("");
            view.active_popup = ActivePopup::None;
            if let Some(contents) = prompt_content {
                (InputResult::Submitted(contents), true)
            } else {
                (InputResult::None, true)
            }
        }
        SlashPopupSelection::Subagent(subagent_name) => {
            if let Some(name) = subagent_name.as_deref() {
                if super::completion::apply_subagent_completion(view, name) {
                    view.active_popup = ActivePopup::None;
                    return view.handle_key_event_without_popup(KeyEvent::new(
                        KeyCode::Enter,
                        KeyModifiers::NONE,
                    ));
                }
                return (InputResult::None, true);
            }
            (InputResult::None, true)
        }
    }
}

