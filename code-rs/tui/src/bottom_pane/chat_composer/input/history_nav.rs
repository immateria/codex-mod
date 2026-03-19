use super::*;

pub(super) fn try_history_up_inner(view: &mut ChatComposer) -> bool {
    if !view
        .history
        .should_handle_navigation(view.textarea.text(), view.textarea.cursor())
    {
        return false;
    }
    let text = view
        .history
        .navigate_up(view.textarea.text(), &view.app_event_tx);
    apply_history_result(view, text)
}

pub(super) fn try_history_down_inner(view: &mut ChatComposer) -> bool {
    // Only meaningful when browsing or when original text is recorded
    if !view
        .history
        .should_handle_navigation(view.textarea.text(), view.textarea.cursor())
    {
        return false;
    }
    let text = view.history.navigate_down(&view.app_event_tx);
    apply_history_result(view, text)
}

pub(super) fn history_is_browsing_inner(view: &ChatComposer) -> bool {
    view.history.is_browsing()
}

pub(super) fn mark_next_down_scrolls_history_inner(view: &mut ChatComposer) {
    view.next_down_scrolls_history = true;
}

fn apply_history_result(view: &mut ChatComposer, text: Option<String>) -> bool {
    let Some(text) = text else {
        return false;
    };
    view.textarea.set_text(&text);
    view.textarea.set_cursor(0);
    view.resync_popups();
    true
}

