use super::*;

pub(super) fn handle_paste_inner(view: &mut ChatComposer, pasted: String) -> bool {
    view.post_paste_space_guard = None;
    // If the pasted text looks like a base64/data-URI image, decode it and insert as a path.
    if let Ok((path, info)) = try_decode_base64_image_to_temp_png(&pasted) {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image.png");
        let placeholder = format!("[image: {filename}]");
        // Insert placeholder and notify chat widget about the mapping.
        view.textarea.insert_str(&placeholder);
        view.textarea.insert_str(" ");
        view.typed_anything = true; // Mark that user has interacted via paste
        view.app_event_tx.send(crate::app_event::AppEvent::RegisterPastedImage {
            placeholder: placeholder.clone(),
            path,
        });
        view.flash_footer_notice(format!(
            "Added image {}x{} (PNG)",
            info.width, info.height
        ));
    } else if pasted.len() > LARGE_PASTE_CHAR_THRESHOLD {
        let char_count = pasted.chars().count();
        if char_count > LARGE_PASTE_CHAR_THRESHOLD {
            let placeholder = format!("[Pasted Content {char_count} chars]");
            view.textarea.insert_str(&placeholder);
            view.pending_pastes.push((placeholder, pasted));
            view.typed_anything = true; // Mark that user has interacted via paste
        } else {
            view.textarea.insert_str(&pasted);
            view.typed_anything = true; // Mark that user has interacted via paste
            super::space_guard::maybe_start_post_paste_space_guard(view, &pasted);
        }
    } else if handle_paste_image_path(view, &pasted) {
        view.textarea.insert_str(" ");
        view.typed_anything = true; // Mark that user has interacted via paste
    } else if pasted.trim().is_empty() {
        // No textual content pasted — try reading an image directly from the OS clipboard.
        match paste_image_to_temp_png() {
            Ok((path, info)) => {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("image.png");
                let placeholder = format!("[image: {filename}]");
                view.textarea.insert_str(&placeholder);
                view.textarea.insert_str(" ");
                view.typed_anything = true; // Mark that user has interacted via paste
                view.app_event_tx.send(crate::app_event::AppEvent::RegisterPastedImage {
                    placeholder: placeholder.clone(),
                    path,
                });
                // Give a small visual confirmation in the footer.
                view.flash_footer_notice(format!(
                    "Added image {}x{} (PNG)",
                    info.width, info.height
                ));
            }
            Err(_) => {
                // Fall back to doing nothing special; keep composer unchanged.
            }
        }
    } else {
        view.textarea.insert_str(&pasted);
        view.typed_anything = true; // Mark that user has interacted via paste
        super::space_guard::maybe_start_post_paste_space_guard(view, &pasted);
    }
    view.sync_command_popup();
    if matches!(view.active_popup, ActivePopup::Command(_)) {
        view.dismissed_file_popup_token = None;
    } else {
        view.sync_file_search_popup();
    }
    true
}

/// Heuristic handling for pasted paths: if the pasted text looks like a
/// filesystem path (including file:// URLs and Windows paths), insert the
/// normalized path directly into the composer and return true. The caller
/// will add a trailing space to separate from subsequent input.
fn handle_paste_image_path(view: &mut ChatComposer, pasted: &str) -> bool {
    if let Some(path) = normalize_pasted_path(pasted) {
        // Insert the normalized path verbatim. We don't attempt to load the
        // file or special-case images here; higher layers handle attachments.
        view.textarea.insert_str(&path.to_string_lossy());
        return true;
    }
    false
}

