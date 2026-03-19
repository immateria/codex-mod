use super::*;

mod keys;
mod popup;
mod results;
mod sync;
mod tokens;

impl ChatComposer {
    /// Integrate results from an asynchronous file search.
    pub(crate) fn on_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        results::on_file_search_result(self, query, matches);
    }

    /// Close the file-search popup if it is currently active. Returns true if closed.
    pub(crate) fn close_file_popup_if_active(&mut self) -> bool {
        popup::close_file_popup_if_active(self)
    }

    pub(crate) fn file_popup_visible(&self) -> bool {
        popup::file_popup_visible(self)
    }

    pub(super) fn confirm_file_popup_selection(&mut self) -> (InputResult, bool) {
        popup::confirm_file_popup_selection(self)
    }

    /// Clamps `pos` to the nearest valid UTF-8 char boundary within `text`.
    pub(super) fn clamp_to_char_boundary(text: &str, pos: usize) -> usize {
        tokens::clamp_to_char_boundary(text, pos)
    }

    /// Handle key events when file search popup is visible.
    pub(super) fn handle_key_event_with_file_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        keys::handle_key_event_with_file_popup(self, key_event)
    }

    /// Extract the completion token under the cursor for auto file search.
    pub(super) fn current_completion_token(textarea: &TextArea) -> Option<String> {
        tokens::current_completion_token(textarea)
    }

    /// Extract the generic token under the cursor (no special rules).
    /// Used for Tab-triggered one-off file searches.
    pub(super) fn current_generic_token(textarea: &TextArea) -> Option<String> {
        tokens::current_generic_token(textarea)
    }

    /// Replace the active `@token` (the one under the cursor) with `path`.
    pub(crate) fn insert_selected_path(&mut self, path: &str) {
        tokens::insert_selected_path(self, path);
    }

    /// Synchronize `self.file_search_popup` with the current text in the textarea.
    /// Note this is only called when self.active_popup is NOT Command.
    pub(super) fn sync_file_search_popup(&mut self) {
        sync::sync_file_search_popup(self);
    }
}

