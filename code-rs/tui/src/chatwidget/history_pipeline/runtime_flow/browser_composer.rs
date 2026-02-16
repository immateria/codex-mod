use super::*;

impl ChatWidget<'_> {
    // If a completed exec cell sits at `idx`, attempt to merge it into the
    // previous cell when they represent the same action header (e.g., Search, Read).

    // MCP tool call handlers now live in chatwidget::tools

    /// Get or create the global browser manager
    pub(in super::super::super) async fn get_browser_manager() -> Arc<BrowserManager> {
        code_browser::global::get_or_create_browser_manager().await
    }

    pub(crate) fn insert_str(&mut self, s: &str) {
        if self.auto_state.should_show_goal_entry()
            && matches!(self.auto_goal_escape_state, AutoGoalEscState::Inactive)
            && !s.trim().is_empty()
        {
            self.auto_goal_escape_state = AutoGoalEscState::NeedsEnableEditing;
        }
        self.bottom_pane.insert_str(s);
    }

    pub(crate) fn set_composer_text(&mut self, text: String) {
        if self.auto_state.should_show_goal_entry()
            && matches!(self.auto_goal_escape_state, AutoGoalEscState::Inactive)
            && !text.trim().is_empty()
        {
            self.auto_goal_escape_state = AutoGoalEscState::NeedsEnableEditing;
        }
        self.bottom_pane.set_composer_text(text);
    }

    // Removed: pending insert sequencing is not used under strict ordering.

    pub(crate) fn register_pasted_image(&mut self, placeholder: String, path: std::path::PathBuf) {
        let persisted = self
            .persist_user_image_if_needed(&path)
            .unwrap_or_else(|| path.clone());
        if persisted.exists() && persisted.is_file() {
            self.pending_images.insert(placeholder, persisted);
            self.request_redraw();
            return;
        }

        // Some terminals (notably on macOS) can drop a "promised" file path
        // (e.g., from Preview) that doesn't actually exist on disk. Fall back
        // to reading the image directly from the clipboard.
        match crate::clipboard_paste::paste_image_to_temp_png() {
            Ok((clipboard_path, _info)) => {
                let clipboard_persisted = self
                    .persist_user_image_if_needed(&clipboard_path)
                    .unwrap_or(clipboard_path);
                self.pending_images.insert(placeholder, clipboard_persisted);
                self.push_background_tail("Used clipboard image (dropped file path was missing).");
                self.request_redraw();
            }
            Err(err) => {
                tracing::warn!(
                    "dropped/pasted image path missing ({}); clipboard fallback failed: {}",
                    persisted.display(),
                    err
                );
            }
        }
    }
}
