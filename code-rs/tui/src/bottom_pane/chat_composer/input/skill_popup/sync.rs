use super::*;

fn current_dollar_token(textarea: &crate::components::textarea::TextArea) -> Option<String> {
    let text = textarea.text();
    let cursor = text.len().min(textarea.cursor());
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
    let token = &text[start..end];
    token.strip_prefix('$')
        .filter(|r| !r.is_empty())
        .map(str::to_owned)
}

pub(super) fn sync_skill_popup_inner(view: &mut ChatComposer) {
    if matches!(view.active_popup, ActivePopup::Command(_) | ActivePopup::File(_)) {
        return;
    }

    let token = current_dollar_token(&view.textarea);

    match (&mut view.active_popup, token) {
        (ActivePopup::Skill(popup), Some(tok)) => {
            popup.on_text_change(&tok);
            if popup.match_count() == 0 {
                view.active_popup = ActivePopup::None;
            }
        }
        (ActivePopup::Skill(_), None) => {
            view.active_popup = ActivePopup::None;
        }
        (ActivePopup::None, Some(tok))
            if !view.history.is_browsing() && !view.available_skills.is_empty() =>
        {
            let mut popup =
                crate::bottom_pane::chat_composer::popups::SkillPopup::new();
            popup.set_skills(view.available_skills.clone());
            popup.on_text_change(&tok);
            if popup.match_count() > 0 {
                view.active_popup = ActivePopup::Skill(popup);
                view.app_event_tx
                    .send(crate::app_event::AppEvent::ComposerExpanded);
            }
        }
        _ => {}
    }
}
