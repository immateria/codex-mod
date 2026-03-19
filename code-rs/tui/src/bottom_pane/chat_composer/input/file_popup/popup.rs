use super::*;

pub(super) fn ensure_file_popup<'a>(view: &'a mut ChatComposer) -> &'a mut FileSearchPopup {
    if !matches!(view.active_popup, ActivePopup::File(_)) {
        view.active_popup = ActivePopup::File(FileSearchPopup::new());
    }
    let ActivePopup::File(popup) = &mut view.active_popup else {
        unreachable!("ensure_file_popup always installs a File popup");
    };
    popup
}

pub(super) fn close_file_popup_if_active(view: &mut ChatComposer) -> bool {
    match view.active_popup {
        ActivePopup::File(_) => {
            view.active_popup = ActivePopup::None;
            view.file_popup_origin = None;
            view.current_file_query = None;
            true
        }
        _ => false,
    }
}

pub(super) fn file_popup_visible(view: &ChatComposer) -> bool {
    matches!(view.active_popup, ActivePopup::File(_))
}

pub(super) fn confirm_file_popup_selection(view: &mut ChatComposer) -> (InputResult, bool) {
    let sel_path = {
        let ActivePopup::File(popup) = &mut view.active_popup else {
            return (InputResult::None, false);
        };

        let Some(sel) = popup.selected_match() else {
            return (InputResult::None, false);
        };

        sel.to_string()
    };

    view.insert_selected_path(&sel_path);
    view.active_popup = ActivePopup::None;
    view.file_popup_origin = None;
    view.current_file_query = None;
    (InputResult::None, true)
}

