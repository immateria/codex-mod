use super::*;

pub(super) fn handle_key_event_with_file_popup(
    view: &mut ChatComposer,
    key_event: KeyEvent,
) -> (InputResult, bool) {
    let ActivePopup::File(popup) = &mut view.active_popup else {
        return (InputResult::None, false);
    };

    match key_event {
        KeyEvent { code: KeyCode::Up, modifiers, .. } => {
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
        KeyEvent { code: KeyCode::Down, modifiers, .. } => {
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
        KeyEvent {
            code: KeyCode::Esc, ..
        } => {
            // Hide popup without modifying text, remember token to avoid immediate reopen.
            if let Some(tok) = super::tokens::current_completion_token(&view.textarea) {
                view.dismissed_file_popup_token = Some(tok);
            }
            view.active_popup = ActivePopup::None;
            view.file_popup_origin = None;
            view.current_file_query = None;
            (InputResult::None, true)
        }
        KeyEvent {
            code: KeyCode::Tab, ..
        }
        | KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            ..
        } => view.confirm_file_popup_selection(),
        input => view.handle_input_basic(input),
    }
}

