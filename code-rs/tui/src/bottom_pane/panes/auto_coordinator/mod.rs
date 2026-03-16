use std::time::{Duration, Instant};

use crate::app_event_sender::AppEventSender;
use crate::auto_drive_style::AutoDriveStyle;

mod input;
mod pane_impl;
mod render;
mod style;
mod text;

#[derive(Clone, Debug)]
pub(crate) struct CountdownState {
    pub remaining: u8,
}

#[derive(Clone, Debug)]
pub(crate) struct AutoCoordinatorButton {
    pub label: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct AutoActiveViewModel {
    pub status_lines: Vec<String>,
    pub cli_prompt: Option<String>,
    pub cli_context: Option<String>,
    pub show_composer: bool,
    pub editing_prompt: bool,
    pub awaiting_submission: bool,
    pub waiting_for_response: bool,
    pub coordinator_waiting: bool,
    pub waiting_for_review: bool,
    pub countdown: Option<CountdownState>,
    pub button: Option<AutoCoordinatorButton>,
    pub manual_hint: Option<String>,
    pub ctrl_switch_hint: String,
    pub cli_running: bool,
    pub turns_completed: usize,
    pub started_at: Option<Instant>,
    pub elapsed: Option<Duration>,
    pub status_sent_to_user: Option<String>,
    pub status_title: Option<String>,
    pub session_tokens: Option<u64>,
    pub intro_started_at: Option<Instant>,
    pub intro_reduced_motion: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum AutoCoordinatorViewModel {
    Active(AutoActiveViewModel),
}

pub(crate) struct AutoCoordinatorView {
    model: AutoCoordinatorViewModel,
    app_event_tx: AppEventSender,
    status_message: Option<String>,
    style: AutoDriveStyle,
}

impl AutoCoordinatorView {
    const MIN_COMPOSER_VIEWPORT: u16 = 3;
    const HEADER_HEIGHT: u16 = 1;

    pub fn new(model: AutoCoordinatorViewModel, app_event_tx: AppEventSender, style: AutoDriveStyle) -> Self {
        Self {
            model,
            app_event_tx,
            status_message: None,
            style,
        }
    }

    pub fn update_model(&mut self, model: AutoCoordinatorViewModel) {
        self.model = model;
    }

    pub fn set_style(&mut self, style: AutoDriveStyle) {
        self.style = style;
    }

    #[cfg(test)]
    pub(crate) fn model(&self) -> &AutoCoordinatorViewModel {
        &self.model
    }

    pub(crate) fn composer_visible(&self) -> bool {
        matches!(&self.model, AutoCoordinatorViewModel::Active(model) if model.show_composer)
    }
}

