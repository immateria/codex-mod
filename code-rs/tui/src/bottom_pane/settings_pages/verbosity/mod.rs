use code_core::config_types::TextVerbosity;

use crate::app_event_sender::AppEventSender;

mod input;
mod model;
mod mouse;
mod pane_impl;
mod pages;
mod render;
#[cfg(test)]
mod tests;

const VERBOSITY_OPTIONS: [(TextVerbosity, &str, &str); 3] = [
    (TextVerbosity::Low, "Low", "Concise responses"),
    (TextVerbosity::Medium, "Medium", "Balanced detail (default)"),
    (TextVerbosity::High, "High", "Detailed responses"),
];

/// Interactive UI for selecting text verbosity level.
pub(crate) struct VerbositySelectionView {
    current_verbosity: TextVerbosity,
    selected_idx: usize,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl VerbositySelectionView {
    pub fn new(current_verbosity: TextVerbosity, app_event_tx: AppEventSender) -> Self {
        let selected_idx = VERBOSITY_OPTIONS
            .iter()
            .position(|(verbosity, _, _)| *verbosity == current_verbosity)
            .unwrap_or(0);
        Self {
            current_verbosity,
            selected_idx,
            app_event_tx,
            is_complete: false,
        }
    }
}

