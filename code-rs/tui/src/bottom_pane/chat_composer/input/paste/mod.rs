use super::*;

mod burst;
mod insert;
mod placeholders;
mod space_guard;

impl ChatComposer {
    pub fn handle_paste(&mut self, pasted: String) -> bool {
        insert::handle_paste_inner(self, pasted)
    }

    pub(super) fn should_suppress_post_paste_space(&mut self, event: &KeyEvent) -> bool {
        space_guard::should_suppress_post_paste_space_inner(self, event)
    }

    /// Clear all composer input and reset transient state like pending pastes
    /// and history navigation.
    pub(crate) fn clear_text(&mut self) {
        burst::clear_text_inner(self);
    }

    /// Retire any expired paste-burst timing window.
    pub(crate) fn flush_paste_burst_if_due(&mut self) -> bool {
        burst::flush_paste_burst_if_due_inner(self)
    }

    pub(crate) fn is_in_paste_burst(&self) -> bool {
        burst::is_in_paste_burst_inner(self)
    }

    pub(crate) fn recommended_paste_flush_delay() -> Duration {
        burst::recommended_paste_flush_delay_inner()
    }

    /// Attempts to remove a placeholder if the cursor is at the end of one.
    /// Returns true if a placeholder was removed.
    pub(super) fn try_remove_placeholder_at_cursor(&mut self) -> bool {
        placeholders::try_remove_placeholder_at_cursor_inner(self)
    }

}
