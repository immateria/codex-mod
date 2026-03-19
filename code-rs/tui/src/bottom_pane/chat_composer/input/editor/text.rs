use super::*;

pub(super) fn set_text_content(view: &mut ChatComposer, text: String) {
    view.textarea.set_text(&text);
    *view.textarea_state.borrow_mut() = TextAreaState::default();
    if !text.is_empty() {
        view.typed_anything = true;
    }
    view.sync_command_popup();
    view.sync_file_search_popup();
}

pub(super) fn insert_str(view: &mut ChatComposer, text: &str) {
    view.textarea.insert_str(text);
    view.typed_anything = true; // Mark that user has interacted via programmatic insertion
    view.sync_command_popup();
    view.sync_file_search_popup();
}

pub(super) fn text(view: &ChatComposer) -> &str {
    view.textarea.text()
}

