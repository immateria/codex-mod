use std::collections::HashMap;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use code_core::protocol::Op;

/// Direction of the most recent history navigation step, used to continue
/// skipping duplicates when an async persistent-entry fetch resolves.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NavDirection { Up, Down }

/// State machine that manages shell-style history navigation (Up/Down) inside
/// the chat composer. This struct is intentionally decoupled from the
/// rendering widget so the logic remains isolated and easier to test.
pub(crate) struct ChatComposerHistory {
    /// Identifier of the history log as reported by `SessionConfiguredEvent`.
    history_log_id: Option<u64>,
    /// Number of entries already present in the persistent cross-session
    /// history file when the session started.
    history_entry_count: usize,

    /// Messages submitted by the user *during this UI session* (newest at END).
    local_history: Vec<String>,

    /// Cache of persistent history entries fetched on-demand.
    fetched_history: HashMap<usize, String>,

    /// Current cursor within the combined (persistent + local) history. `None`
    /// indicates the user is *not* currently browsing history.
    history_cursor: Option<isize>,

    /// The text that was last *shown to the user* via history navigation.
    /// Used for dedup (skip consecutive identical entries) and to decide
    /// whether further Up/Down should be treated as navigation.
    last_history_text: Option<String>,

    /// The original text that was in the composer before starting history navigation.
    /// This allows us to restore it when pressing down past the newest entry.
    original_text: Option<String>,

    /// Direction of the most recent navigation, for continuing dedup after
    /// an async persistent-entry fetch.
    last_nav_direction: NavDirection,
}

impl ChatComposerHistory {
    pub fn new() -> Self {
        Self {
            history_log_id: None,
            history_entry_count: 0,
            local_history: Vec::new(),
            fetched_history: HashMap::new(),
            history_cursor: None,
            last_history_text: None,
            original_text: None,
            last_nav_direction: NavDirection::Up,
        }
    }

    /// Update metadata when a new session is configured.
    pub fn set_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.history_log_id = Some(log_id);
        self.history_entry_count = entry_count;
        self.fetched_history.clear();
        self.local_history.clear();
        self.history_cursor = None;
        self.last_history_text = None;
        self.original_text = None;
    }

    /// Record a message submitted by the user in the current session so it can
    /// be recalled later.
    pub fn record_local_submission(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        // Avoid inserting a duplicate if identical to the previous entry.
        if self
            .local_history
            .last()
            .is_some_and(|prev| prev == text)
        {
            return;
        }
        self.local_history.push(text.to_owned());
        self.history_cursor = None;
        self.last_history_text = None;
        self.original_text = None;
    }

    /// Reset navigation state (used when clearing input with double-Esc or when text is edited)
    pub fn reset_navigation(&mut self) {
        self.history_cursor = None;
        self.last_history_text = None;
        self.original_text = None;
    }

    /// Returns true if the user is currently browsing history.
    pub fn is_browsing(&self) -> bool { self.history_cursor.is_some() }

    /// Should Up/Down key presses be interpreted as history navigation given
    /// the current content and cursor position of `textarea`?
    pub fn should_handle_navigation(&self, text: &str, cursor: usize) -> bool {
        if self.history_entry_count == 0 && self.local_history.is_empty() {
            return false;
        }

        // Empty textarea - always handle navigation
        if text.is_empty() {
            return true;
        }

        // If we're currently browsing history and the text matches the last history text,
        // continue handling navigation (allows continued browsing)
        if self.history_cursor.is_some()
            && let Some(ref last) = self.last_history_text
                && last == text {
                    return true;
                }

        // If cursor at start and text is either original or a history entry, handle navigation
        if cursor == 0 {
            // Check if it's the original text we saved
            if let Some(ref orig) = self.original_text
                && orig == text {
                    return true;
                }
            // Check if it matches last history text
            if let Some(ref last) = self.last_history_text
                && last == text {
                    return true;
                }
        }

        false
    }

    /// Handle <Up>. Skips consecutive duplicate entries so the user sees
    /// distinct commands only.
    pub fn navigate_up(
        &mut self,
        current_text: &str,
        app_event_tx: &AppEventSender,
    ) -> Option<String> {
        let total_entries = self.history_entry_count + self.local_history.len();
        if total_entries == 0 {
            return None;
        }

        self.last_nav_direction = NavDirection::Up;

        // If we're not browsing yet, save the current text as the original
        if self.history_cursor.is_none() && self.original_text.is_none() {
            self.original_text = Some(current_text.to_owned());
        }

        let mut next_idx = match self.history_cursor {
            None => (total_entries as isize) - 1,
            Some(0) => return None, // already at oldest
            Some(idx) => idx - 1,
        };

        // Skip consecutive duplicates (only for synchronously-available entries).
        loop {
            self.history_cursor = Some(next_idx);
            match self.peek_text_at_index(next_idx as usize, app_event_tx) {
                Some(text) if self.is_duplicate(&text) => {
                    if next_idx == 0 {
                        // Hit the bottom — commit and return even if dup.
                        self.last_history_text = Some(text.clone());
                        return Some(text);
                    }
                    next_idx -= 1;
                }
                Some(text) => {
                    // Distinct entry — commit and return.
                    self.last_history_text = Some(text.clone());
                    return Some(text);
                }
                None => return None, // async fetch pending
            }
        }
    }

    /// Handle <Down>. Skips consecutive duplicate entries.
    pub fn navigate_down(&mut self, app_event_tx: &AppEventSender) -> Option<String> {
        let total_entries = self.history_entry_count + self.local_history.len();
        if total_entries == 0 {
            return None;
        }

        self.last_nav_direction = NavDirection::Down;

        loop {
            let next_idx_opt = match self.history_cursor {
                None => return None, // not browsing
                Some(idx) if (idx as usize) + 1 >= total_entries => None,
                Some(idx) => Some(idx + 1),
            };

            if let Some(idx) = next_idx_opt {
                self.history_cursor = Some(idx);
                match self.peek_text_at_index(idx as usize, app_event_tx) {
                    Some(text) if self.is_duplicate(&text) => {
                        continue; // skip duplicate
                    }
                    Some(text) => {
                        // Distinct entry — commit and return.
                        self.last_history_text = Some(text.clone());
                        return Some(text);
                    }
                    None => return None, // async fetch pending
                }
            } else {
                // Past newest – restore original text and exit browsing mode.
                let result = self.original_text.clone().unwrap_or_default();
                self.history_cursor = None;
                self.last_history_text = None;
                self.original_text = None;
                return Some(result);
            }
        }
    }

    /// Integrate a `GetHistoryEntryResponse` event. If the fetched text is a
    /// duplicate of the currently shown entry, auto-advances in the last
    /// navigation direction.
    pub fn on_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
        app_event_tx: &AppEventSender,
    ) -> Option<String> {
        if self.history_log_id != Some(log_id) {
            return None;
        }
        let text = entry?;
        self.fetched_history.insert(offset, text.clone());

        if self.history_cursor == Some(offset as isize) {
            // If this resolves to a duplicate, auto-skip in the same direction.
            // Use a dummy current_text — navigate_up only uses it to set
            // original_text on first browse, which is already set by now.
            if self.is_duplicate(&text) {
                return match self.last_nav_direction {
                    NavDirection::Up => self.navigate_up("", app_event_tx),
                    NavDirection::Down => self.navigate_down(app_event_tx),
                };
            }
            self.last_history_text = Some(text.clone());
            return Some(text);
        }
        None
    }

    // ---------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------

    /// Returns true when `text` matches the most recently shown history entry.
    fn is_duplicate(&self, text: &str) -> bool {
        self.last_history_text.as_deref() == Some(text)
    }

    /// Return the text at `global_idx` if synchronously available (local or
    /// cached persistent entry). For uncached persistent entries, fires an
    /// async fetch and returns `None`.
    ///
    /// This is a *pure peek* — it does NOT update `last_history_text`.
    fn peek_text_at_index(
        &self,
        global_idx: usize,
        app_event_tx: &AppEventSender,
    ) -> Option<String> {
        if global_idx >= self.history_entry_count {
            // Local entry.
            return self
                .local_history
                .get(global_idx - self.history_entry_count)
                .cloned();
        }
        if let Some(text) = self.fetched_history.get(&global_idx) {
            return Some(text.clone());
        }
        if let Some(log_id) = self.history_log_id {
            let op = Op::GetHistoryEntryRequest {
                offset: global_idx,
                log_id,
            };
            app_event_tx.send(AppEvent::codex_op(op));
        }
        None
    }
}
