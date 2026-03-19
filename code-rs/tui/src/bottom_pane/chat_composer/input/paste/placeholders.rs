use super::*;

pub(super) fn try_remove_placeholder_at_cursor_inner(view: &mut ChatComposer) -> bool {
    let text = view.textarea.text();
    let p = ChatComposer::clamp_to_char_boundary(text, view.textarea.cursor());

    // Find any placeholder that ends at the cursor position
    let placeholder_to_remove = view.pending_pastes.iter().find_map(|(ph, _)| {
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
        view.textarea.replace_range(p - placeholder.len()..p, "");
        view.pending_pastes.retain(|(ph, _)| ph != &placeholder);
        true
    } else {
        false
    }
}

