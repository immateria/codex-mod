use super::*;

pub(super) fn starts_with_slash_name(line: &str, name: &str) -> bool {
    let trimmed = line.trim_start();
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    let Some(suffix) = rest.strip_prefix(name) else {
        return false;
    };
    suffix.is_empty() || suffix.starts_with(char::is_whitespace)
}

pub(super) fn apply_subagent_completion(view: &mut ChatComposer, name: &str) -> bool {
    let first_line = view.textarea.text().lines().next().unwrap_or("");
    if starts_with_slash_name(first_line, name) {
        return true;
    }
    view.textarea.set_text(&format!("/{name} "));
    let new_cursor = view.textarea.text().len();
    view.textarea.set_cursor(new_cursor);
    false
}

