use super::*;

pub(super) fn sync_file_search_popup(view: &mut ChatComposer) {
    // Determine if there is a token underneath the cursor worth completing.
    match super::tokens::current_completion_token(&view.textarea) {
        Some(query) => {
            if view.dismissed_file_popup_token.as_ref() == Some(&query) {
                return;
            }

            if !query.is_empty() {
                view.app_event_tx
                    .send(crate::app_event::AppEvent::StartFileSearch(query.clone()));
            }

            {
                let popup = super::popup::ensure_file_popup(view);
                if query.is_empty() {
                    popup.set_empty_prompt();
                } else {
                    popup.set_query(&query);
                }
            }

            view.current_file_query = Some(query);
            view.file_popup_origin = Some(FilePopupOrigin::Auto);
            view.dismissed_file_popup_token = None;
        }
        None => {
            // Allow manually-triggered popups (via Tab) to stay open while the
            // cursor remains within the same generic token. When the token
            // changes, trigger a new search; otherwise keep the popup stable.
            if let Some(FilePopupOrigin::Manual { token }) = &mut view.file_popup_origin
                && let Some(generic) = super::tokens::current_generic_token(&view.textarea) {
                    if generic.is_empty() {
                        view.active_popup = ActivePopup::None;
                        view.dismissed_file_popup_token = None;
                        view.file_popup_origin = None;
                        view.current_file_query = None;
                    } else if *token != generic {
                        *token = generic.clone();
                        super::popup::ensure_file_popup(view).set_query(&generic);
                        view.current_file_query = Some(generic.clone());
                        view.app_event_tx
                            .send(crate::app_event::AppEvent::StartFileSearch(generic));
                    }
                    return;
                }

            view.active_popup = ActivePopup::None;
            view.dismissed_file_popup_token = None;
            view.file_popup_origin = None;
            view.current_file_query = None;
        }
    }
}

