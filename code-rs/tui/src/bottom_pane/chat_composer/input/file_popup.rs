use super::*;

impl ChatComposer {
    fn ensure_file_popup(&mut self) -> &mut FileSearchPopup {
        if !matches!(self.active_popup, ActivePopup::File(_)) {
            self.active_popup = ActivePopup::File(FileSearchPopup::new());
        }
        let ActivePopup::File(popup) = &mut self.active_popup else {
            unreachable!("ensure_file_popup always installs a File popup");
        };
        popup
    }

    /// Integrate results from an asynchronous file search.
    pub(crate) fn on_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        // Handle one-off Tab-triggered case first: only open if matches exist.
        if self.pending_tab_file_query.as_ref() == Some(&query) {
            // If the user kept typing while the search was in-flight, resync to the
            // latest token before applying stale results.
            if let Some(current_token) = Self::current_generic_token(&self.textarea) {
                if Self::current_completion_token(&self.textarea).is_some() {
                    // A new auto-triggerable token (e.g., @ or ./) should be handled by the
                    // standard auto completion path instead of hijacking the manual popup.
                    self.pending_tab_file_query = None;
                    self.file_popup_origin = None;
                    self.current_file_query = None;
                    return;
                }
                if !current_token.is_empty() && current_token != query {
                    self.pending_tab_file_query = Some(current_token.clone());
                    self.ensure_file_popup().set_query(&current_token);
                    self.current_file_query = Some(current_token.clone());
                    self.file_popup_origin = Some(FilePopupOrigin::Manual {
                        token: current_token.clone(),
                    });
                    self.app_event_tx
                        .send(crate::app_event::AppEvent::StartFileSearch(current_token));
                    return;
                }
            }

            // Clear pending regardless of result to avoid repeats.
            self.pending_tab_file_query = None;

            if matches.is_empty() {
                self.dismissed_file_popup_token = None;
                // Clear the waiting state so the popup shows "no matches" instead of spinning forever.
                self.ensure_file_popup().set_matches(&query, Vec::new());
                return; // do not open popup when no matches to avoid flicker
            }

            let popup = self.ensure_file_popup();
            popup.set_query(&query);
            popup.set_matches(&query, matches);
            self.current_file_query = Some(query.clone());
            self.file_popup_origin = Some(FilePopupOrigin::Manual { token: query });
            self.dismissed_file_popup_token = None;
            return;
        }

        if matches!(self.file_popup_origin, Some(FilePopupOrigin::Manual { .. }))
            && self.current_file_query.as_ref() == Some(&query)
        {
            self.ensure_file_popup().set_matches(&query, matches);
            return;
        }

        // Otherwise, only apply if user is still editing a token matching the query
        // and that token qualifies for auto-trigger (i.e., @ or ./).
        let current_opt = Self::current_completion_token(&self.textarea);
        let Some(current_token) = current_opt else { return; };
        if !current_token.starts_with(&query) { return; }

        self.ensure_file_popup().set_matches(&query, matches);
    }
    /// Close the file-search popup if it is currently active. Returns true if closed.
    pub(crate) fn close_file_popup_if_active(&mut self) -> bool {
        match self.active_popup {
            ActivePopup::File(_) => {
                self.active_popup = ActivePopup::None;
                self.file_popup_origin = None;
                self.current_file_query = None;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn file_popup_visible(&self) -> bool {
        matches!(self.active_popup, ActivePopup::File(_))
    }
    pub(super) fn confirm_file_popup_selection(&mut self) -> (InputResult, bool) {
        let ActivePopup::File(popup) = &mut self.active_popup else {
            return (InputResult::None, false);
        };

        let Some(sel) = popup.selected_match() else {
            return (InputResult::None, false);
        };

        let sel_path = sel.to_string();
        // Drop popup borrow before using self mutably again.
        self.insert_selected_path(&sel_path);
        self.active_popup = ActivePopup::None;
        self.file_popup_origin = None;
        self.current_file_query = None;
        (InputResult::None, true)
    }

    // popup_active removed; callers use explicit state or rely on App policy.

    /// Clamps `pos` to the nearest valid UTF-8 char boundary within `text`.
    pub(super) fn clamp_to_char_boundary(text: &str, pos: usize) -> usize {
        text.floor_char_boundary(pos.min(text.len()))
    }

    fn token_cursor_context(textarea: &TextArea) -> TokenCursorContext<'_> {
        let text = textarea.text();
        let safe_cursor = Self::clamp_to_char_boundary(text, textarea.cursor());
        let before_cursor = &text[..safe_cursor];
        let after_cursor = &text[safe_cursor..];
        let start_idx = before_cursor
            .char_indices()
            .rfind(|(_, c)| c.is_whitespace())
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);
        let end_rel_idx = after_cursor
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(after_cursor.len());
        let end_idx = safe_cursor + end_rel_idx;

        TokenCursorContext {
            text,
            safe_cursor,
            after_cursor,
            start_idx,
            end_idx,
        }
    }

    /// Handle key events when file search popup is visible.
    pub(super) fn handle_key_event_with_file_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        let ActivePopup::File(popup) = &mut self.active_popup else {
            return (InputResult::None, false);
        };

        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers, .. } => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    return self.handle_key_event_without_popup(key_event);
                }
                // If there are 0 or 1 items, let Up behave normally (cursor/history/scroll)
                if popup.match_count() <= 1 {
                    return self.handle_key_event_without_popup(key_event);
                }
                popup.move_up();
                (InputResult::None, true)
            }
            KeyEvent { code: KeyCode::Down, modifiers, .. } => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    return self.handle_key_event_without_popup(key_event);
                }
                // If there are 0 or 1 items, let Down behave normally (cursor/history/scroll)
                if popup.match_count() <= 1 {
                    return self.handle_key_event_without_popup(key_event);
                }
                popup.move_down();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // Hide popup without modifying text, remember token to avoid immediate reopen.
                if let Some(tok) = Self::current_completion_token(&self.textarea) {
                    self.dismissed_file_popup_token = Some(tok);
                }
                self.active_popup = ActivePopup::None;
                self.file_popup_origin = None;
                self.current_file_query = None;
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            }
            | KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => self.confirm_file_popup_selection(),
            input => self.handle_input_basic(input),
        }
    }

    /// Extract the `@token` that the cursor is currently positioned on, if any.
    ///
    /// The returned string **does not** include the leading `@`.
    ///
    /// Behavior:
    /// - The cursor may be anywhere inside the token (including on the
    ///   leading `@`). It does not need to be at the end of the line.
    /// - A token is delimited by ASCII whitespace (space, tab, newline).
    /// - If the token under the cursor starts with `@`, that token is
    ///   returned without the leading `@`. This includes the case where the
    ///   token is just "@" (empty query), which is used to trigger a UI hint
    fn current_at_token(textarea: &TextArea) -> Option<String> {
        let ctx = Self::token_cursor_context(textarea);

        // Detect whether we're on whitespace at the cursor boundary.
        let at_whitespace = ctx
            .after_cursor
            .chars()
            .next()
            .map(char::is_whitespace)
            .unwrap_or(false);

        // Left candidate: token containing the cursor position.
        let token_left = if ctx.start_idx < ctx.end_idx {
            Some(&ctx.text[ctx.start_idx..ctx.end_idx])
        } else {
            None
        };

        // Right candidate: token immediately after any whitespace from the cursor.
        let ws_len_right: usize = ctx
            .after_cursor
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(char::len_utf8)
            .sum();
        let start_right = ctx.safe_cursor + ws_len_right;
        let end_right_rel = ctx.text[start_right..]
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(ctx.text.len() - start_right);
        let end_right = start_right + end_right_rel;
        let token_right = if start_right < end_right {
            Some(&ctx.text[start_right..end_right])
        } else {
            None
        };

        let left_at = token_left
            .filter(|t| t.starts_with('@'))
            .map(|t| t[1..].to_string());
        let right_at = token_right
            .filter(|t| t.starts_with('@'))
            .map(|t| t[1..].to_string());

        if at_whitespace {
            if right_at.is_some() {
                return right_at;
            }
            if token_left.is_some_and(|t| t == "@") {
                return None;
            }
            return left_at;
        }
        if ctx.after_cursor.starts_with('@') {
            return right_at.or(left_at);
        }
        left_at.or(right_at)
    }

    /// Extract the completion token under the cursor for auto file search.
    ///
    /// Auto-trigger only for:
    /// - explicit @tokens (without the leading '@' in the return value)
    /// - tokens starting with "./" (relative paths)
    ///
    /// Returns the token text (without a leading '@' if present). Any other
    /// tokens should not auto-trigger completion; they may be handled on Tab.
    pub(super) fn current_completion_token(textarea: &TextArea) -> Option<String> {
        // Prefer explicit @tokens when present.
        if let Some(tok) = Self::current_at_token(textarea) {
            return Some(tok);
        }

        // Otherwise, consider the generic token under the cursor, but only
        // auto-trigger for tokens starting with "./".
        let ctx = Self::token_cursor_context(textarea);
        if ctx.start_idx >= ctx.end_idx {
            return None;
        }

        let token = &ctx.text[ctx.start_idx..ctx.end_idx];

        // Strip a leading '@' if the user typed it but we didn't catch it
        // (paranoia; current_at_token should have handled this case).
        let token_stripped = token.strip_prefix('@').unwrap_or(token);

        if token_stripped.starts_with("./") {
            return Some(token_stripped.to_string());
        }

        None
    }

    /// Extract the generic token under the cursor (no special rules).
    /// Used for Tab-triggered one-off file searches.
    pub(super) fn current_generic_token(textarea: &TextArea) -> Option<String> {
        let ctx = Self::token_cursor_context(textarea);
        if ctx.start_idx >= ctx.end_idx {
            return None;
        }

        Some(ctx.text[ctx.start_idx..ctx.end_idx].to_string())
    }

    /// Replace the active `@token` (the one under the cursor) with `path`.
    ///
    /// The algorithm mirrors `current_at_token` so replacement works no matter
    /// where the cursor is within the token and regardless of how many
    /// `@tokens` exist in the line.
    pub(crate) fn insert_selected_path(&mut self, path: &str) {
        let ctx = Self::token_cursor_context(&self.textarea);
        let text = ctx.text;
        let start_idx = ctx.start_idx;
        let end_idx = ctx.end_idx;

        // If the path contains whitespace, wrap it in double quotes so the
        // local prompt arg parser treats it as a single argument. Avoid adding
        // quotes when the path already contains one to keep behavior simple.
        let needs_quotes = path.chars().any(char::is_whitespace);
        let inserted = if needs_quotes {
            format!("\"{}\"", path.replace('"', "\\\""))
        } else {
            path.to_string()
        };

        // Replace the slice `[start_idx, end_idx)` with the chosen path and a trailing space.
        let mut new_text =
            String::with_capacity(text.len() - (end_idx - start_idx) + inserted.len() + 1);
        new_text.push_str(&text[..start_idx]);
        new_text.push_str(&inserted);
        new_text.push(' ');
        new_text.push_str(&text[end_idx..]);

        self.textarea.set_text(&new_text);
        let new_cursor = start_idx.saturating_add(inserted.len()).saturating_add(1);
        self.textarea.set_cursor(new_cursor);
    }

    /// Synchronize `self.file_search_popup` with the current text in the textarea.
    /// Note this is only called when self.active_popup is NOT Command.
    pub(super) fn sync_file_search_popup(&mut self) {
        // Determine if there is a token underneath the cursor worth completing.
        match Self::current_completion_token(&self.textarea) {
            Some(query) => {
                if self.dismissed_file_popup_token.as_ref() == Some(&query) {
                    return;
                }

                if !query.is_empty() {
                    self.app_event_tx
                        .send(crate::app_event::AppEvent::StartFileSearch(query.clone()));
                }

                {
                    let popup = self.ensure_file_popup();
                    if query.is_empty() {
                        popup.set_empty_prompt();
                    } else {
                        popup.set_query(&query);
                    }
                }

                self.current_file_query = Some(query);
                self.file_popup_origin = Some(FilePopupOrigin::Auto);
                self.dismissed_file_popup_token = None;
            }
            None => {
                // Allow manually-triggered popups (via Tab) to stay open while the
                // cursor remains within the same generic token. When the token
                // changes, trigger a new search; otherwise keep the popup stable.
                if let Some(FilePopupOrigin::Manual { token }) = &mut self.file_popup_origin
                    && let Some(generic) = Self::current_generic_token(&self.textarea) {
                        if generic.is_empty() {
                            self.active_popup = ActivePopup::None;
                            self.dismissed_file_popup_token = None;
                            self.file_popup_origin = None;
                            self.current_file_query = None;
                        } else if *token != generic {
                            *token = generic.clone();
                            self.ensure_file_popup().set_query(&generic);
                            self.current_file_query = Some(generic.clone());
                            self.app_event_tx
                                .send(crate::app_event::AppEvent::StartFileSearch(generic));
                        }
                        return;
                    }

                self.active_popup = ActivePopup::None;
                self.dismissed_file_popup_token = None;
                self.file_popup_origin = None;
                self.current_file_query = None;
            }
        }
    }

}
