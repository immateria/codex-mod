use super::*;

use crate::components::form_text_field::InputFilter;

impl PromptsSettingsView {
    pub(super) const DEFAULT_HEIGHT: u16 = 20;

    pub fn new(prompts: Vec<CustomPrompt>, app_event_tx: AppEventSender) -> Self {
        let mut name_field = FormTextField::new_single_line();
        name_field.set_filter(InputFilter::Id);
        let body_field = FormTextField::new_multi_line();

        Self {
            prompts,
            selected: 0,
            focus: Focus::List,
            name_field,
            body_field,
            status: None,
            app_event_tx,
            is_complete: false,
            mode: Mode::List,
        }
    }

    pub(crate) fn framed(&self) -> PromptsSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> PromptsSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> PromptsSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> PromptsSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub fn is_complete(&self) -> bool {
        self.is_complete
    }
}

