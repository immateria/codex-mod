use super::*;

pub(super) fn handle_backslash_continuation(view: &mut ChatComposer) -> bool {
    let text = view.textarea.text();
    let mut iter = text.char_indices().rev();
    let Some((last_idx, last_char)) = iter.next() else {
        return false;
    };
    if matches!(last_char, ' ' | '\t') {
        return false;
    }
    if last_char != '\\' {
        return false;
    }

    let trailing_backslashes = text[..last_idx]
        .chars()
        .rev()
        .take_while(|c| *c == '\\')
        .count()
        + 1;
    if trailing_backslashes % 2 == 0 {
        return false;
    }

    let line_start = text[..last_idx].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    let line_before = &text[line_start..last_idx];
    let indent_end = line_before
        .bytes()
        .take_while(|&byte| byte == b' ' || byte == b'\t')
        .count();
    let indentation = &line_before[..indent_end];
    let replacement = if indentation.is_empty() {
        String::from("\n")
    } else {
        format!("\n{indentation}")
    };
    let backslash_end = last_idx + '\\'.len_utf8();
    view.textarea.replace_range(last_idx..backslash_end, &replacement);

    view.history.reset_navigation();
    view.post_paste_space_guard = None;
    if !view.pending_pastes.is_empty() {
        view.pending_pastes
            .retain(|(placeholder, _)| view.textarea.text().contains(placeholder));
    }
    view.typed_anything = true;
    true
}

/// Handle generic Input events that modify the textarea content.
pub(super) fn handle_input_basic(view: &mut ChatComposer, input: KeyEvent) -> (InputResult, bool) {
    if view.should_suppress_post_paste_space(&input) {
        return (InputResult::None, false);
    }

    // Special handling for backspace on placeholders
    if let KeyEvent {
        code: KeyCode::Backspace,
        ..
    } = input
        && view.try_remove_placeholder_at_cursor()
    {
        // Text was modified, reset history navigation
        view.history.reset_navigation();
        return (InputResult::None, true);
    }

    let text_before = view.textarea.text().to_string();

    // Normal input handling
    view.textarea.input(input);
    let text_after = view.textarea.text();
    let changed = text_before != text_after;

    if changed
        || view
            .post_paste_space_guard
            .as_ref()
            .map(|guard| view.textarea.cursor() != guard.cursor_pos)
            .unwrap_or(false)
    {
        view.post_paste_space_guard = None;
    }

    // If text changed, reset history navigation state
    if changed {
        view.history.reset_navigation();
        if !text_after.is_empty() {
            view.typed_anything = true;
        }
    }

    // Check if any placeholders were removed and remove their corresponding pending pastes
    if !view.pending_pastes.is_empty() {
        view.pending_pastes
            .retain(|(placeholder, _)| text_after.contains(placeholder));
    }

    (InputResult::None, true)
}

