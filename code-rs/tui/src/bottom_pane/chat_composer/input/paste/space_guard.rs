use super::*;

pub(super) fn maybe_start_post_paste_space_guard(view: &mut ChatComposer, pasted: &str) {
    if !pasted.ends_with(' ') {
        return;
    }
    let cursor_pos = view.textarea.cursor();
    // Ensure the character immediately before the cursor is a literal space.
    if cursor_pos == 0 {
        return;
    }
    if let Some(slice) = view.textarea.text().as_bytes().get(cursor_pos - 1) && *slice == b' ' {
        view.post_paste_space_guard = Some(PostPasteSpaceGuard {
            expires_at: Instant::now() + Duration::from_secs(2),
            cursor_pos,
        });
    }
}

pub(super) fn should_suppress_post_paste_space_inner(
    view: &mut ChatComposer,
    event: &KeyEvent,
) -> bool {
    if event.kind != KeyEventKind::Press {
        return false;
    }
    if event.code != KeyCode::Char(' ') {
        return false;
    }
    let unshifted_space = event.modifiers == KeyModifiers::NONE || event.modifiers == KeyModifiers::SHIFT;
    if !unshifted_space {
        return false;
    }
    let Some(guard) = &view.post_paste_space_guard else {
        return false;
    };
    let now = Instant::now();
    if now > guard.expires_at {
        view.post_paste_space_guard = None;
        return false;
    }
    if view.textarea.cursor() != guard.cursor_pos {
        view.post_paste_space_guard = None;
        return false;
    }
    let text = view.textarea.text();
    if guard.cursor_pos == 0 || guard.cursor_pos > text.len() {
        view.post_paste_space_guard = None;
        return false;
    }
    if text.as_bytes()[guard.cursor_pos - 1] != b' ' {
        view.post_paste_space_guard = None;
        return false;
    }
    view.post_paste_space_guard = None;
    true
}

