use super::*;

pub(super) fn apply_skill_completion(view: &mut ChatComposer, skill_name: &str) {
    let text = view.textarea.text().to_owned();
    let cursor = text.len().min(view.textarea.cursor());

    let before = &text[..cursor];
    let start = before
        .char_indices()
        .rfind(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let after = &text[cursor..];
    let end_rel = after
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map_or(after.len(), |(i, _)| i);
    let end = cursor + end_rel;

    let replacement = format!("${skill_name} ");
    let mut new_text =
        String::with_capacity(text.len() - (end - start) + replacement.len());
    new_text.push_str(&text[..start]);
    new_text.push_str(&replacement);
    new_text.push_str(&text[end..]);

    view.textarea.set_text(&new_text);
    let new_cursor = start + replacement.len();
    view.textarea.set_cursor(new_cursor);
}
