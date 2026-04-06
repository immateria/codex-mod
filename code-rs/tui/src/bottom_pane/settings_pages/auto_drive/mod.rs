use crate::app_event::AutoContinueMode;
use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;
use code_core::config_types::{AutoDriveModelRoutingEntry, ReasoningEffort};
use std::cell::Cell;

mod input;
mod model;
mod mouse;
mod pages;
mod pane_impl;
mod render;
#[cfg(test)]
mod tests;

const ROUTING_REASONING_LEVELS: [ReasoningEffort; 5] = [
    ReasoningEffort::Minimal,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::XHigh,
];

const ROUTING_DESCRIPTION_MAX_CHARS: usize = 200;

#[derive(Clone)]
enum AutoDriveSettingsMode {
    Main,
    RoutingList,
    RoutingEditor(RoutingEditorState),
}

pub(crate) struct AutoDriveSettingsInit {
    pub app_event_tx: AppEventSender,
    pub model: String,
    pub model_reasoning: ReasoningEffort,
    pub use_chat_model: bool,
    pub review_enabled: bool,
    pub agents_enabled: bool,
    pub cross_check_enabled: bool,
    pub qa_automation_enabled: bool,
    pub model_routing_enabled: bool,
    pub model_routing_entries: Vec<AutoDriveModelRoutingEntry>,
    pub routing_model_options: Vec<String>,
    pub continue_mode: AutoContinueMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HoverTarget {
    MainOption(usize),
    RoutingRow(usize),
    RoutingEditor(RoutingEditorField),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RoutingEditorField {
    Model,
    Enabled,
    Reasoning,
    Description,
    Save,
    Cancel,
}

impl RoutingEditorField {
    fn all() -> &'static [RoutingEditorField] {
        &[
            RoutingEditorField::Model,
            RoutingEditorField::Enabled,
            RoutingEditorField::Reasoning,
            RoutingEditorField::Description,
            RoutingEditorField::Save,
            RoutingEditorField::Cancel,
        ]
    }

    fn next(self) -> Self {
        let fields = Self::all();
        let current_idx = fields.iter().position(|field| *field == self).unwrap_or(0);
        fields
            .get((current_idx + 1) % fields.len())
            .copied()
            .unwrap_or(RoutingEditorField::Model)
    }

    fn previous(self) -> Self {
        let fields = Self::all();
        let current_idx = fields.iter().position(|field| *field == self).unwrap_or(0);
        if current_idx == 0 {
            fields.last().copied().unwrap_or(RoutingEditorField::Model)
        } else {
            fields
                .get(current_idx - 1)
                .copied()
                .unwrap_or(RoutingEditorField::Model)
        }
    }
}

#[derive(Clone)]
struct RoutingEditorState {
    index: Option<usize>,
    model_cursor: usize,
    enabled: bool,
    reasoning_cursor: usize,
    reasoning_enabled: [bool; ROUTING_REASONING_LEVELS.len()],
    description: String,
    selected_field: RoutingEditorField,
}

impl RoutingEditorState {
    fn from_entry(
        index: Option<usize>,
        entry: Option<&AutoDriveModelRoutingEntry>,
        model_options: &[String],
    ) -> Self {
        let mut reasoning_enabled = [false; ROUTING_REASONING_LEVELS.len()];
        let mut model_cursor = 0;
        let mut enabled = true;
        let mut description = String::new();

        if let Some(existing) = entry {
            for (idx, level) in ROUTING_REASONING_LEVELS.iter().enumerate() {
                reasoning_enabled[idx] = existing.reasoning_levels.contains(level);
            }
            if let Some(found) = model_options
                .iter()
                .position(|model| model.eq_ignore_ascii_case(&existing.model))
            {
                model_cursor = found;
            }
            enabled = existing.enabled;
            description = existing.description.clone();
        } else if let Some(high_idx) = ROUTING_REASONING_LEVELS
            .iter()
            .position(|level| *level == ReasoningEffort::High)
        {
            reasoning_enabled[high_idx] = true;
        }

        Self {
            index,
            model_cursor,
            enabled,
            reasoning_cursor: 0,
            reasoning_enabled,
            description,
            selected_field: RoutingEditorField::Model,
        }
    }

    fn selected_reasoning_levels(&self) -> Vec<ReasoningEffort> {
        ROUTING_REASONING_LEVELS
            .iter()
            .enumerate()
            .filter_map(|(idx, level)| self.reasoning_enabled[idx].then_some(*level))
            .collect()
    }

    fn toggle_reasoning_at_cursor(&mut self) {
        if self.reasoning_cursor >= self.reasoning_enabled.len() {
            self.reasoning_cursor = self.reasoning_enabled.len().saturating_sub(1);
        }
        if let Some(slot) = self.reasoning_enabled.get_mut(self.reasoning_cursor) {
            *slot = !*slot;
        }
    }
}

pub(crate) struct AutoDriveSettingsView {
    app_event_tx: AppEventSender,
    main_state: ScrollState,
    mode: AutoDriveSettingsMode,
    hovered: Option<HoverTarget>,
    model: String,
    model_reasoning: ReasoningEffort,
    use_chat_model: bool,
    review_enabled: bool,
    agents_enabled: bool,
    cross_check_enabled: bool,
    qa_automation_enabled: bool,
    diagnostics_enabled: bool,
    model_routing_enabled: bool,
    model_routing_entries: Vec<AutoDriveModelRoutingEntry>,
    routing_model_options: Vec<String>,
    routing_state: ScrollState,
    routing_viewport_rows: Cell<usize>,
    continue_mode: AutoContinueMode,
    status_message: Option<String>,
    closing: bool,
}

pub(crate) type AutoDriveSettingsViewFramed<'v> =
    crate::bottom_pane::chrome_view::Framed<'v, AutoDriveSettingsView>;
pub(crate) type AutoDriveSettingsViewContentOnly<'v> =
    crate::bottom_pane::chrome_view::ContentOnly<'v, AutoDriveSettingsView>;
pub(crate) type AutoDriveSettingsViewFramedMut<'v> =
    crate::bottom_pane::chrome_view::FramedMut<'v, AutoDriveSettingsView>;
pub(crate) type AutoDriveSettingsViewContentOnlyMut<'v> =
    crate::bottom_pane::chrome_view::ContentOnlyMut<'v, AutoDriveSettingsView>;

impl AutoDriveSettingsView {
    pub(crate) fn framed(&self) -> AutoDriveSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> AutoDriveSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> AutoDriveSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> AutoDriveSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub fn is_view_complete(&self) -> bool {
        self.closing
    }
}
