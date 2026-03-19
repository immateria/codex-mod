use super::*;

pub(super) fn clear_text_inner(view: &mut ChatComposer) {
    view.textarea.set_text("");
    view.pending_pastes.clear();
    view.history.reset_navigation();
    view.post_paste_space_guard = None;
}

pub(super) fn flush_paste_burst_if_due_inner(view: &mut ChatComposer) -> bool {
    view.paste_burst.flush_if_due(Instant::now())
}

pub(super) fn is_in_paste_burst_inner(view: &ChatComposer) -> bool {
    view.paste_burst.is_active(Instant::now())
}

pub(super) fn recommended_paste_flush_delay_inner() -> Duration {
    PasteBurst::recommended_flush_delay()
}

