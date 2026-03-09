use super::*;

impl ChatComposer {
    pub fn handle_paste(&mut self, pasted: String) -> bool {
        self.post_paste_space_guard = None;
        // If the pasted text looks like a base64/data-URI image, decode it and insert as a path.
        if let Ok((path, info)) = try_decode_base64_image_to_temp_png(&pasted) {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("image.png");
            let placeholder = format!("[image: {filename}]");
            // Insert placeholder and notify chat widget about the mapping.
            self.textarea.insert_str(&placeholder);
            self.textarea.insert_str(" ");
            self.typed_anything = true; // Mark that user has interacted via paste
            self.app_event_tx
                .send(crate::app_event::AppEvent::RegisterPastedImage { placeholder: placeholder.clone(), path });
            self.flash_footer_notice(format!("Added image {}x{} (PNG)", info.width, info.height));
        } else if pasted.len() > LARGE_PASTE_CHAR_THRESHOLD {
            let char_count = pasted.chars().count();
            if char_count > LARGE_PASTE_CHAR_THRESHOLD {
                let placeholder = format!("[Pasted Content {char_count} chars]");
                self.textarea.insert_str(&placeholder);
                self.pending_pastes.push((placeholder, pasted));
                self.typed_anything = true; // Mark that user has interacted via paste
            } else {
                self.textarea.insert_str(&pasted);
                self.typed_anything = true; // Mark that user has interacted via paste
                self.maybe_start_post_paste_space_guard(&pasted);
            }
        } else if self.handle_paste_image_path(&pasted) {
            self.textarea.insert_str(" ");
            self.typed_anything = true; // Mark that user has interacted via paste
        } else if pasted.trim().is_empty() {
            // No textual content pasted — try reading an image directly from the OS clipboard.
            match paste_image_to_temp_png() {
                Ok((path, info)) => {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("image.png");
                    let placeholder = format!("[image: {filename}]");
                    self.textarea.insert_str(&placeholder);
                    self.textarea.insert_str(" ");
                    self.typed_anything = true; // Mark that user has interacted via paste
                    self.app_event_tx
                        .send(crate::app_event::AppEvent::RegisterPastedImage { placeholder: placeholder.clone(), path });
                    // Give a small visual confirmation in the footer.
                    self.flash_footer_notice(format!(
                        "Added image {}x{} (PNG)",
                        info.width, info.height
                    ));
                }
                Err(_) => {
                    // Fall back to doing nothing special; keep composer unchanged.
                }
            }
        } else {
            self.textarea.insert_str(&pasted);
            self.typed_anything = true; // Mark that user has interacted via paste
            self.maybe_start_post_paste_space_guard(&pasted);
        }
        self.sync_command_popup();
        if matches!(self.active_popup, ActivePopup::Command(_)) {
            self.dismissed_file_popup_token = None;
        } else {
            self.sync_file_search_popup();
        }
        true
    }

    /// Heuristic handling for pasted paths: if the pasted text looks like a
    /// filesystem path (including file:// URLs and Windows paths), insert the
    /// normalized path directly into the composer and return true. The caller
    /// will add a trailing space to separate from subsequent input.
    fn handle_paste_image_path(&mut self, pasted: &str) -> bool {
        if let Some(path) = normalize_pasted_path(pasted) {
            // Insert the normalized path verbatim. We don't attempt to load the
            // file or special-case images here; higher layers handle attachments.
            self.textarea.insert_str(&path.to_string_lossy());
            return true;
        }
        false
    }

    fn maybe_start_post_paste_space_guard(&mut self, pasted: &str) {
        if !pasted.ends_with(' ') {
            return;
        }
        let cursor_pos = self.textarea.cursor();
        // Ensure the character immediately before the cursor is a literal space.
        if cursor_pos == 0 {
            return;
        }
        if let Some(slice) = self
            .textarea
            .text()
            .as_bytes()
            .get(cursor_pos - 1)
            && *slice == b' ' {
                self.post_paste_space_guard = Some(PostPasteSpaceGuard {
                    expires_at: Instant::now() + Duration::from_secs(2),
                    cursor_pos,
                });
            }
    }

    pub(super) fn should_suppress_post_paste_space(&mut self, event: &KeyEvent) -> bool {
        if event.kind != KeyEventKind::Press {
            return false;
        }
        if event.code != KeyCode::Char(' ') {
            return false;
        }
        let unshifted_space = event.modifiers == KeyModifiers::NONE
            || event.modifiers == KeyModifiers::SHIFT;
        if !unshifted_space {
            return false;
        }
        let Some(guard) = &self.post_paste_space_guard else {
            return false;
        };
        let now = Instant::now();
        if now > guard.expires_at {
            self.post_paste_space_guard = None;
            return false;
        }
        if self.textarea.cursor() != guard.cursor_pos {
            self.post_paste_space_guard = None;
            return false;
        }
        let text = self.textarea.text();
        if guard.cursor_pos == 0 || guard.cursor_pos > text.len() {
            self.post_paste_space_guard = None;
            return false;
        }
        if text.as_bytes()[guard.cursor_pos - 1] != b' ' {
            self.post_paste_space_guard = None;
            return false;
        }
        self.post_paste_space_guard = None;
        true
    }


    /// Clear all composer input and reset transient state like pending pastes
    /// and history navigation.
    pub(crate) fn clear_text(&mut self) {
        self.textarea.set_text("");
        self.pending_pastes.clear();
        self.history.reset_navigation();
        self.post_paste_space_guard = None;
    }

    /// Retire any expired paste-burst timing window.
    pub(crate) fn flush_paste_burst_if_due(&mut self) -> bool {
        self.paste_burst.flush_if_due(Instant::now())
    }

    pub(crate) fn is_in_paste_burst(&self) -> bool {
        self.paste_burst.is_active(Instant::now())
    }

    pub(crate) fn recommended_paste_flush_delay() -> Duration {
        PasteBurst::recommended_flush_delay()
    }
    /// Attempts to remove a placeholder if the cursor is at the end of one.
    /// Returns true if a placeholder was removed.
    pub(super) fn try_remove_placeholder_at_cursor(&mut self) -> bool {
        let text = self.textarea.text();
        let p = Self::clamp_to_char_boundary(text, self.textarea.cursor());

        // Find any placeholder that ends at the cursor position
        let placeholder_to_remove = self.pending_pastes.iter().find_map(|(ph, _)| {
            if p < ph.len() {
                return None;
            }
            let potential_ph_start = p - ph.len();
            // Use `get` to avoid panicking on non-char-boundary indices.
            match text.get(potential_ph_start..p) {
                Some(slice) if slice == ph => Some(ph.clone()),
                _ => None,
            }
        });

        if let Some(placeholder) = placeholder_to_remove {
            self.textarea.replace_range(p - placeholder.len()..p, "");
            self.pending_pastes.retain(|(ph, _)| ph != &placeholder);
            true
        } else {
            false
        }
    }

}
