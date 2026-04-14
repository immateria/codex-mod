use super::*;

pub(super) fn handle_key_event_with_skill_popup_inner(
    view: &mut ChatComposer,
    key_event: KeyEvent,
) -> (InputResult, bool) {
    match key_event {
        KeyEvent { code: KeyCode::Up, modifiers, .. } => {
            let ActivePopup::Skill(popup) = &mut view.active_popup else {
                return (InputResult::None, false);
            };
            if modifiers.contains(KeyModifiers::SHIFT) || popup.match_count() <= 1 {
                return view.handle_key_event_without_popup(key_event);
            }
            popup.move_up();
            (InputResult::None, true)
        }
        KeyEvent { code: KeyCode::Down, modifiers, .. } => {
            let ActivePopup::Skill(popup) = &mut view.active_popup else {
                return (InputResult::None, false);
            };
            if modifiers.contains(KeyModifiers::SHIFT) || popup.match_count() <= 1 {
                return view.handle_key_event_without_popup(key_event);
            }
            popup.move_down();
            (InputResult::None, true)
        }
        KeyEvent { code: KeyCode::Esc, .. } => {
            view.active_popup = ActivePopup::None;
            (InputResult::None, true)
        }
        KeyEvent { code: KeyCode::Tab, .. }
        | KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press | KeyEventKind::Repeat,
            ..
        } => {
            let selected = {
                let ActivePopup::Skill(popup) = &mut view.active_popup else {
                    return (InputResult::None, false);
                };
                popup.selected_item().or_else(|| popup.first_match())
            };

            if let Some(skill_name) = selected {
                super::completion::apply_skill_completion(view, &skill_name);
                view.active_popup = ActivePopup::None;
                if matches!(key_event.code, KeyCode::Tab) {
                    return (InputResult::None, true);
                }
                return view.handle_key_event_without_popup(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::NONE,
                ));
            }
            (InputResult::None, true)
        }
        input => view.handle_input_basic(input),
    }
}
