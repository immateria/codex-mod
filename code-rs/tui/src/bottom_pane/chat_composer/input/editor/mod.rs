use super::*;

mod basic;
mod history;
mod keys;
mod text;

impl ChatComposer {
    pub(crate) fn on_history_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
    ) -> bool {
        history::on_history_entry_response(self, log_id, offset, entry)
    }

    pub fn set_text_content(&mut self, text: String) {
        text::set_text_content(self, text);
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        text::insert_str(self, text);
    }

    pub(crate) fn text(&self) -> &str {
        text::text(self)
    }

    pub(super) fn handle_key_event_without_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        keys::handle_key_event_without_popup(self, key_event)
    }

    pub(super) fn handle_backslash_continuation(&mut self) -> bool {
        basic::handle_backslash_continuation(self)
    }

    /// Handle generic Input events that modify the textarea content.
    pub(super) fn handle_input_basic(&mut self, input: KeyEvent) -> (InputResult, bool) {
        basic::handle_input_basic(self, input)
    }
}

