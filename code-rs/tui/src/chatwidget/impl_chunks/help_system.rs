impl ChatWidget<'_> {
    /// Returns `true` if the trimmed input is a help trigger (`:?`),
    /// opening the help overlay popup.
    pub(crate) fn try_handle_help_query(&mut self, trimmed: &str) -> bool {
        if trimmed != ":?" {
            return false;
        }
        self.show_help_popup();
        true
    }
}
