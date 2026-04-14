use super::*;

mod completion;
mod keys;
mod sync;

impl ChatComposer {
    pub(crate) fn sync_skill_popup(&mut self) {
        sync::sync_skill_popup_inner(self);
    }

    pub(super) fn handle_key_event_with_skill_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        keys::handle_key_event_with_skill_popup_inner(self, key_event)
    }
}
