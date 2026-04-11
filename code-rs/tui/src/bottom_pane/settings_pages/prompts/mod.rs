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

use crate::timing::DEFAULT_VISIBLE_ROWS as DEFAULT_LIST_VIEWPORT_ROWS;

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
enum ConfirmAction {
    Delete,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Mode {
    List,
    Edit,
    ConfirmDelete { name: String, selected_idx: usize },
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
    focused_confirm_button: ConfirmAction,
    hovered_confirm_button: Option<ConfirmAction>,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(PromptsSettingsView);

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
