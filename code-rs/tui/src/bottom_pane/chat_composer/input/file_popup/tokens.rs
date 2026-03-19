use super::*;

pub(super) fn clamp_to_char_boundary(text: &str, pos: usize) -> usize {
    text.floor_char_boundary(pos.min(text.len()))
}

fn token_cursor_context(textarea: &TextArea) -> TokenCursorContext<'_> {
    let text = textarea.text();
    let safe_cursor = clamp_to_char_boundary(text, textarea.cursor());
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

/// Extract the `@token` that the cursor is currently positioned on, if any.
///
/// The returned string **does not** include the leading `@`.
fn current_at_token(textarea: &TextArea) -> Option<String> {
    let ctx = token_cursor_context(textarea);

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
    if let Some(tok) = current_at_token(textarea) {
        return Some(tok);
    }

    // Otherwise, consider the generic token under the cursor, but only
    // auto-trigger for tokens starting with "./".
    let ctx = token_cursor_context(textarea);
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
    let ctx = token_cursor_context(textarea);
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
pub(super) fn insert_selected_path(view: &mut ChatComposer, path: &str) {
    let ctx = token_cursor_context(&view.textarea);
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
    let mut new_text = String::with_capacity(text.len() - (end_idx - start_idx) + inserted.len() + 1);
    new_text.push_str(&text[..start_idx]);
    new_text.push_str(&inserted);
    new_text.push(' ');
    new_text.push_str(&text[end_idx..]);

    view.textarea.set_text(&new_text);
    let new_cursor = start_idx.saturating_add(inserted.len()).saturating_add(1);
    view.textarea.set_cursor(new_cursor);
}

