impl ChatWidget<'_> {
    /// Returns `true` if the trimmed input is a help trigger (`?` or `:?`),
    /// opening the help overlay popup instead of polluting the chat history.
    pub(crate) fn try_handle_help_query(&mut self, trimmed: &str) -> bool {
        if trimmed != "?" && trimmed != ":?" {
            return false;
        }
        self.show_help_popup();
        true
    }
}
