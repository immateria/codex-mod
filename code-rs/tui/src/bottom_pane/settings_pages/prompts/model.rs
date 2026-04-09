use super::*;

use crate::components::form_text_field::InputFilter;

impl PromptsSettingsView {
    pub(super) const DEFAULT_HEIGHT: u16 = 20;

    pub fn new(prompts: Vec<CustomPrompt>, app_event_tx: AppEventSender) -> Self {
        let mut name_field = FormTextField::new_single_line();
        name_field.set_filter(InputFilter::Id);
        let body_field = FormTextField::new_multi_line();

        let list_state = ScrollState::with_first_selected();
        Self {
            prompts,
            list_state,
            list_viewport_rows: Cell::new(DEFAULT_LIST_VIEWPORT_ROWS),
            focus: Focus::List,
            name_field,
            body_field,
            status: None,
            app_event_tx,
            is_complete: false,
            mode: Mode::List,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.is_complete
    }
}
