use std::cell::Cell;

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::SettingsSection;
use crate::components::scroll_state::ScrollState;

mod input;
mod model;
mod mouse;
mod pane_impl;
mod pages;
mod render;
#[cfg(test)]
mod tests;

pub(crate) struct SettingsOverviewView {
    rows: Vec<(SettingsSection, Option<String>)>,
    scroll: ScrollState,
    viewport_rows: Cell<usize>,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl SettingsOverviewView {
    pub(crate) fn new(
        rows: Vec<(SettingsSection, Option<String>)>,
        initial_section: SettingsSection,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut scroll = ScrollState::new();
        if !rows.is_empty() {
            let selected = rows
                .iter()
                .position(|(section, _)| *section == initial_section)
                .unwrap_or(0);
            scroll.selected_idx = Some(selected);
        }
        Self {
            rows,
            scroll,
            viewport_rows: Cell::new(12),
            app_event_tx,
            is_complete: false,
        }
    }
}

