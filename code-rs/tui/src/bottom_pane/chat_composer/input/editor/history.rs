use super::*;

pub(super) fn on_history_entry_response(
    view: &mut ChatComposer,
    log_id: u64,
    offset: usize,
    entry: Option<String>,
) -> bool {
    let Some(text) = view.history.on_entry_response(log_id, offset, entry) else {
        return false;
    };
    view.textarea.set_text(&text);
    view.textarea.set_cursor(0);
    view.resync_popups();
    true
}

