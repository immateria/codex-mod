use std::cell::Cell;

use code_protocol::custom_prompts::CustomPrompt;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;
use ratatui::style::Style;

use crate::components::form_text_field::FormTextField;

mod input;
mod model;
mod mouse;
mod pages;
mod pane_impl;
mod render;
#[cfg(test)]
mod tests;

const DEFAULT_LIST_VIEWPORT_ROWS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    List,
    Name,
    Body,
    Save,
    Delete,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    List,
    Edit,
}

pub(crate) struct PromptsSettingsView {
    prompts: Vec<CustomPrompt>,
    list_state: ScrollState,
    list_viewport_rows: Cell<usize>,
    focus: Focus,
    name_field: FormTextField,
    body_field: FormTextField,
    status: Option<(String, Style)>,
    app_event_tx: AppEventSender,
    is_complete: bool,
    mode: Mode,
}

pub(crate) type PromptsSettingsViewFramed<'v> = crate::bottom_pane::chrome_view::Framed<'v, PromptsSettingsView>;
pub(crate) type PromptsSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, PromptsSettingsView>;
pub(crate) type PromptsSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, PromptsSettingsView>;
pub(crate) type PromptsSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, PromptsSettingsView>;

impl PromptsSettingsView {
    fn list_row_count(&self) -> usize {
        // +1 for the trailing "Add new…" row.
        self.prompts.len().saturating_add(1)
    }

    fn selected_list_idx(&self) -> usize {
        self.list_state
            .selected_idx
            .unwrap_or(0)
            .min(self.list_row_count().saturating_sub(1))
    }

    fn selected_prompt_index(&self) -> Option<usize> {
        let idx = self.selected_list_idx();
        (idx < self.prompts.len()).then_some(idx)
    }

    fn clamp_list_state(&mut self) {
        let total = self.list_row_count();
        self.list_state.clamp_selection(total);
        let visible = self.list_viewport_rows.get().max(1);
        self.list_state.ensure_visible(total, visible);
    }
}
