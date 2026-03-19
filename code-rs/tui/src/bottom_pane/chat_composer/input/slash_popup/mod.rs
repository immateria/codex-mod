use super::*;

mod completion;
mod confirm;
mod keys;
mod sync;

enum SlashPopupSelection {
    Builtin(SlashCommand),
    UserPrompt(Option<String>),
    Subagent(Option<String>),
}

impl ChatComposer {
    pub(crate) fn confirm_slash_popup_selection(&mut self) -> (InputResult, bool) {
        confirm::confirm_slash_popup_selection_inner(self)
    }

    /// Handle a key event when the slash-command popup is visible.
    pub(super) fn handle_key_event_with_slash_popup(
        &mut self,
        key_event: KeyEvent,
    ) -> (InputResult, bool) {
        keys::handle_key_event_with_slash_popup_inner(self, key_event)
    }

    /// Synchronize `self.command_popup` with the current text in the
    /// textarea. This must be called after every modification that can change
    /// the text so the popup is shown/updated/hidden as appropriate.
    pub(crate) fn sync_command_popup(&mut self) {
        sync::sync_command_popup_inner(self);
    }

    pub(crate) fn set_custom_prompts(&mut self, prompts: Vec<CustomPrompt>) {
        sync::set_custom_prompts_inner(self, prompts);
    }

}
