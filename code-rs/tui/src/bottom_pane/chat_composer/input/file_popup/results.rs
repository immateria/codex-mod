use super::*;

pub(super) fn on_file_search_result(view: &mut ChatComposer, query: String, matches: Vec<FileMatch>) {
    // Handle one-off Tab-triggered case first: only open if matches exist.
    if view.pending_tab_file_query.as_ref() == Some(&query) {
        // If the user kept typing while the search was in-flight, resync to the
        // latest token before applying stale results.
        if let Some(current_token) = super::tokens::current_generic_token(&view.textarea) {
            if super::tokens::current_completion_token(&view.textarea).is_some() {
                // A new auto-triggerable token (e.g., @ or ./) should be handled by the
                // standard auto completion path instead of hijacking the manual popup.
                view.pending_tab_file_query = None;
                view.file_popup_origin = None;
                view.current_file_query = None;
                return;
            }
            if !current_token.is_empty() && current_token != query {
                view.pending_tab_file_query = Some(current_token.clone());
                super::popup::ensure_file_popup(view).set_query(&current_token);
                view.current_file_query = Some(current_token.clone());
                view.file_popup_origin = Some(FilePopupOrigin::Manual {
                    token: current_token.clone(),
                });
                view.app_event_tx
                    .send(crate::app_event::AppEvent::StartFileSearch(current_token));
                return;
            }
        }

        // Clear pending regardless of result to avoid repeats.
        view.pending_tab_file_query = None;

        if matches.is_empty() {
            view.dismissed_file_popup_token = None;
            // Clear the waiting state so the popup shows "no matches" instead of spinning forever.
            super::popup::ensure_file_popup(view).set_matches(&query, Vec::new());
            return; // do not open popup when no matches to avoid flicker
        }

        let popup = super::popup::ensure_file_popup(view);
        popup.set_query(&query);
        popup.set_matches(&query, matches);
        view.current_file_query = Some(query.clone());
        view.file_popup_origin = Some(FilePopupOrigin::Manual { token: query });
        view.dismissed_file_popup_token = None;
        return;
    }

    if matches!(view.file_popup_origin, Some(FilePopupOrigin::Manual { .. }))
        && view.current_file_query.as_ref() == Some(&query)
    {
        super::popup::ensure_file_popup(view).set_matches(&query, matches);
        return;
    }

    // Otherwise, only apply if user is still editing a token matching the query
    // and that token qualifies for auto-trigger (i.e., @ or ./).
    let current_opt = super::tokens::current_completion_token(&view.textarea);
    let Some(current_token) = current_opt else { return; };
    if !current_token.starts_with(&query) { return; }

    super::popup::ensure_file_popup(view).set_matches(&query, matches);
}

