mod input;
mod model;
mod mouse;
mod pages;
mod pane_impl;
mod render;

#[cfg(test)]
mod tests;

use code_core::config_types::{AutoResolveAttemptLimit, ReasoningEffort};
use std::cell::Cell;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

const DEFAULT_VISIBLE_ROWS: usize = 8;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SelectionKind {
    ReviewEnabled,
    ReviewModel,
    ReviewResolveModel,
    ReviewAttempts,
    AutoReviewEnabled,
    AutoReviewModel,
    AutoReviewResolveModel,
    AutoReviewAttempts,
}

#[derive(Clone, Debug)]
struct ReviewListModel {
    /// Selection index -> semantic kind.
    selection_kinds: Vec<SelectionKind>,
    /// Selection index -> absolute line index within the flattened run list.
    selection_line: Vec<usize>,
    /// Selection index -> inclusive (section_start_line, section_end_line).
    section_bounds: Vec<(usize, usize)>,
    /// Total line count across all runs.
    total_lines: usize,
}

pub(crate) struct ReviewSettingsView {
    review_use_chat_model: bool,
    review_model: String,
    review_reasoning: ReasoningEffort,
    review_resolve_use_chat_model: bool,
    review_resolve_model: String,
    review_resolve_reasoning: ReasoningEffort,
    review_auto_resolve_enabled: bool,
    review_followups: u32,
    review_followups_index: usize,

    auto_review_enabled: bool,
    auto_review_use_chat_model: bool,
    auto_review_model: String,
    auto_review_reasoning: ReasoningEffort,
    auto_review_resolve_use_chat_model: bool,
    auto_review_resolve_model: String,
    auto_review_resolve_reasoning: ReasoningEffort,
    auto_review_followups: u32,
    auto_review_followups_index: usize,

    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
    viewport_rows: Cell<usize>,
    pending_notice: Option<String>,
}

pub(crate) type ReviewSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, ReviewSettingsView>;
pub(crate) type ReviewSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, ReviewSettingsView>;
pub(crate) type ReviewSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, ReviewSettingsView>;
pub(crate) type ReviewSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, ReviewSettingsView>;

pub(crate) struct ReviewSettingsInit {
    pub review_use_chat_model: bool,
    pub review_model: String,
    pub review_reasoning: ReasoningEffort,
    pub review_resolve_use_chat_model: bool,
    pub review_resolve_model: String,
    pub review_resolve_reasoning: ReasoningEffort,
    pub review_auto_resolve_enabled: bool,
    pub review_followups: u32,
    pub auto_review_enabled: bool,
    pub auto_review_use_chat_model: bool,
    pub auto_review_model: String,
    pub auto_review_reasoning: ReasoningEffort,
    pub auto_review_resolve_use_chat_model: bool,
    pub auto_review_resolve_model: String,
    pub auto_review_resolve_reasoning: ReasoningEffort,
    pub auto_review_followups: u32,
    pub app_event_tx: AppEventSender,
}

impl ReviewSettingsView {
    pub fn set_review_model(&mut self, model: String, effort: ReasoningEffort) {
        self.review_model = model;
        self.review_reasoning = effort;
    }

    pub fn set_review_use_chat_model(&mut self, use_chat: bool) {
        self.review_use_chat_model = use_chat;
    }

    pub fn set_review_resolve_model(&mut self, model: String, effort: ReasoningEffort) {
        self.review_resolve_model = model;
        self.review_resolve_reasoning = effort;
    }

    pub fn set_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        self.review_resolve_use_chat_model = use_chat;
    }

    pub fn set_auto_review_model(&mut self, model: String, effort: ReasoningEffort) {
        self.auto_review_model = model;
        self.auto_review_reasoning = effort;
    }

    pub fn set_auto_review_use_chat_model(&mut self, use_chat: bool) {
        self.auto_review_use_chat_model = use_chat;
    }

    pub fn set_auto_review_resolve_model(&mut self, model: String, effort: ReasoningEffort) {
        self.auto_review_resolve_model = model;
        self.auto_review_resolve_reasoning = effort;
    }

    pub fn set_auto_review_resolve_use_chat_model(&mut self, use_chat: bool) {
        self.auto_review_resolve_use_chat_model = use_chat;
    }

    pub fn set_review_followups(&mut self, attempts: u32) {
        if let Some(idx) = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == attempts)
        {
            self.review_followups_index = idx;
        }
        self.review_followups = attempts;
    }

    pub fn set_auto_review_followups(&mut self, attempts: u32) {
        if let Some(idx) = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == attempts)
        {
            self.auto_review_followups_index = idx;
        }
        self.auto_review_followups = attempts;
    }

    pub fn new(init: ReviewSettingsInit) -> Self {
        let ReviewSettingsInit {
            review_use_chat_model,
            review_model,
            review_reasoning,
            review_resolve_use_chat_model,
            review_resolve_model,
            review_resolve_reasoning,
            review_auto_resolve_enabled,
            review_followups,
            auto_review_enabled,
            auto_review_use_chat_model,
            auto_review_model,
            auto_review_reasoning,
            auto_review_resolve_use_chat_model,
            auto_review_resolve_model,
            auto_review_resolve_reasoning,
            auto_review_followups,
            app_event_tx,
        } = init;
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);

        let default_index = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == AutoResolveAttemptLimit::DEFAULT)
            .unwrap_or(0);

        let review_followups_index = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == review_followups)
            .unwrap_or(default_index);

        let auto_review_followups_index = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == auto_review_followups)
            .unwrap_or(default_index);

        Self {
            review_use_chat_model,
            review_model,
            review_reasoning,
            review_resolve_use_chat_model,
            review_resolve_model,
            review_resolve_reasoning,
            review_auto_resolve_enabled,
            review_followups,
            review_followups_index,
            auto_review_enabled,
            auto_review_use_chat_model,
            auto_review_model,
            auto_review_reasoning,
            auto_review_resolve_use_chat_model,
            auto_review_resolve_model,
            auto_review_resolve_reasoning,
            auto_review_followups,
            auto_review_followups_index,
            app_event_tx,
            state,
            is_complete: false,
            viewport_rows: Cell::new(0),
            pending_notice: None,
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub(crate) fn framed(&self) -> ReviewSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> ReviewSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> ReviewSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> ReviewSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }
}

