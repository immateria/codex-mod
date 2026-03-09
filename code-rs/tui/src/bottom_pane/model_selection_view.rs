use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use crate::ui_interaction::{
    redraw_if,
    route_selectable_list_mouse_with_config,
    SelectableListMouseConfig,
    SelectableListMouseResult,
};
use super::settings_panel::{render_panel, PanelFrameStyle};
use super::BottomPane;
use crate::app_event::{AppEvent, ModelSelectionKind};
use crate::app_event_sender::AppEventSender;
use code_common::model_presets::ModelPreset;
use code_core::config_types::ContextMode;
use code_core::config_types::ReasoningEffort;
use code_core::config_types::ServiceTier;
use code_core::model_family::supports_extended_context;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::cell::RefCell;
use std::cmp::Ordering;

/// Flattened preset entry combining a model with a specific reasoning effort.
#[derive(Clone, Debug)]
struct FlatPreset {
    model: String,
    effort: ReasoningEffort,
    label: String,
    description: String,
}

impl FlatPreset {
    fn from_model_preset(preset: &ModelPreset) -> Vec<Self> {
        preset
            .supported_reasoning_efforts
            .iter()
            .map(|effort_preset| {
                let effort_label = Self::effort_label(effort_preset.effort.into());
                FlatPreset {
                    model: preset.model.to_string(),
                    effort: effort_preset.effort.into(),
                    label: format!("{} {}", preset.display_name, effort_label.to_lowercase()),
                    description: effort_preset.description.to_string(),
                }
            })
            .collect()
    }

    fn effort_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::XHigh => "XHigh",
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModelSelectionTarget {
    Session,
    Review,
    Planning,
    AutoDrive,
    ReviewResolve,
    AutoReview,
    AutoReviewResolve,
}

impl From<ModelSelectionTarget> for ModelSelectionKind {
    fn from(target: ModelSelectionTarget) -> Self {
        match target {
            ModelSelectionTarget::Session => ModelSelectionKind::Session,
            ModelSelectionTarget::Review => ModelSelectionKind::Review,
            ModelSelectionTarget::Planning => ModelSelectionKind::Planning,
            ModelSelectionTarget::AutoDrive => ModelSelectionKind::AutoDrive,
            ModelSelectionTarget::ReviewResolve => ModelSelectionKind::ReviewResolve,
            ModelSelectionTarget::AutoReview => ModelSelectionKind::AutoReview,
            ModelSelectionTarget::AutoReviewResolve => ModelSelectionKind::AutoReviewResolve,
        }
    }
}

impl ModelSelectionTarget {
    fn panel_title(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Select Model & Reasoning",
            ModelSelectionTarget::Review => "Select Review Model & Reasoning",
            ModelSelectionTarget::Planning => "Select Planning Model & Reasoning",
            ModelSelectionTarget::AutoDrive => "Select Auto Drive Model & Reasoning",
            ModelSelectionTarget::ReviewResolve => "Select Resolve Model & Reasoning",
            ModelSelectionTarget::AutoReview => "Select Auto Review Model & Reasoning",
            ModelSelectionTarget::AutoReviewResolve => "Select Auto Review Resolve Model & Reasoning",
        }
    }

    fn current_label(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Current model",
            ModelSelectionTarget::Review => "Review model",
            ModelSelectionTarget::Planning => "Planning model",
            ModelSelectionTarget::AutoDrive => "Auto Drive model",
            ModelSelectionTarget::ReviewResolve => "Resolve model",
            ModelSelectionTarget::AutoReview => "Auto Review model",
            ModelSelectionTarget::AutoReviewResolve => "Auto Review resolve model",
        }
    }

    fn reasoning_label(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Reasoning effort",
            ModelSelectionTarget::Review => "Review reasoning",
            ModelSelectionTarget::Planning => "Planning reasoning",
            ModelSelectionTarget::AutoDrive => "Auto Drive reasoning",
            ModelSelectionTarget::ReviewResolve => "Resolve reasoning",
            ModelSelectionTarget::AutoReview => "Auto Review reasoning",
            ModelSelectionTarget::AutoReviewResolve => "Auto Review resolve reasoning",
        }
    }

    fn supports_follow_chat(self) -> bool {
        !matches!(self, ModelSelectionTarget::Session)
    }

    fn supports_fast_mode(self) -> bool {
        matches!(self, ModelSelectionTarget::Session)
    }
}

pub(crate) struct ModelSelectionViewParams {
    pub presets: Vec<ModelPreset>,
    pub current_model: String,
    pub current_effort: ReasoningEffort,
    pub current_service_tier: Option<ServiceTier>,
    pub current_context_mode: Option<ContextMode>,
    pub use_chat_model: bool,
    pub target: ModelSelectionTarget,
}

pub(crate) struct ModelSelectionView {
    flat_presets: Vec<FlatPreset>,
    selected_index: usize,
    hovered_index: Option<usize>,
    current_model: String,
    current_effort: ReasoningEffort,
    current_service_tier: Option<ServiceTier>,
    current_context_mode: Option<ContextMode>,
    use_chat_model: bool,
    app_event_tx: AppEventSender,
    is_complete: bool,
    target: ModelSelectionTarget,
    /// Cached (entry_index, rect) pairs from last render for mouse hit testing
    item_rects: RefCell<Vec<(usize, Rect)>>,
    /// Scroll offset for rendering when content exceeds available height
    scroll_offset: usize,
    /// Last render area height to track available space
    last_render_height: RefCell<u16>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum EntryKind {
    FastMode,
    ContextMode,
    FollowChat,
    Preset(usize),
}

impl ModelSelectionView {
    pub fn new(params: ModelSelectionViewParams, app_event_tx: AppEventSender) -> Self {
        let ModelSelectionViewParams {
            presets,
            current_model,
            current_effort,
            current_service_tier,
            current_context_mode,
            use_chat_model,
            target,
        } = params;
        let flat_presets: Vec<FlatPreset> = presets
            .iter()
            .flat_map(FlatPreset::from_model_preset)
            .collect();

        let initial_index = Self::initial_selection(
            target.supports_fast_mode(),
            target.supports_fast_mode(),
            target.supports_follow_chat(),
            use_chat_model,
            &flat_presets,
            &current_model,
            current_effort,
        );
        Self {
            flat_presets,
            selected_index: initial_index,
            hovered_index: None,
            current_model,
            current_effort,
            current_service_tier,
            current_context_mode,
            use_chat_model,
            app_event_tx,
            is_complete: false,
            target,
            item_rects: RefCell::new(Vec::new()),
            scroll_offset: 0,
            last_render_height: RefCell::new(0),
        }
    }

    pub(crate) fn update_presets(&mut self, presets: Vec<ModelPreset>) {
        let include_fast_mode = self.target.supports_fast_mode();
        let include_context_mode = self.target.supports_fast_mode();
        let include_follow_chat = self.target.supports_follow_chat();
        let previous_entries = self.entries();
        let previous_selected = previous_entries.get(self.selected_index).copied();
        let previous_flat = self.flat_presets.clone();

        self.flat_presets = presets
            .iter()
            .flat_map(FlatPreset::from_model_preset)
            .collect();

        let mut next_selected: Option<usize> = None;
        match previous_selected {
            Some(EntryKind::FastMode) => {
                if include_fast_mode {
                    next_selected = Some(0);
                }
            }
            Some(EntryKind::ContextMode) => {
                if include_context_mode {
                    next_selected = Some(usize::from(include_fast_mode));
                }
            }
            Some(EntryKind::FollowChat) => {
                if include_follow_chat {
                    next_selected = Some(
                        usize::from(include_fast_mode) + usize::from(include_context_mode),
                    );
                }
            }
            Some(EntryKind::Preset(idx)) => {
                if let Some(old) = previous_flat.get(idx)
                    && let Some((new_idx, _)) = self
                        .flat_presets
                        .iter()
                        .enumerate()
                        .find(|(_, preset)| {
                            preset.model.eq_ignore_ascii_case(&old.model)
                                && preset.effort == old.effort
                        })
                    {
                        let prefix = usize::from(include_fast_mode)
                            + usize::from(include_context_mode)
                            + usize::from(include_follow_chat);
                        next_selected = Some(new_idx + prefix);
                    }
            }
            None => {}
        }

        self.selected_index = next_selected.unwrap_or_else(|| {
            Self::initial_selection(
                include_fast_mode,
                include_context_mode,
                include_follow_chat,
                self.use_chat_model,
                &self.flat_presets,
                &self.current_model,
                self.current_effort,
            )
        });

        let total = self.entries().len();
        if total == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= total {
            self.selected_index = total - 1;
        }
    }

    fn initial_selection(
        include_fast_mode: bool,
        include_context_mode: bool,
        include_follow_chat: bool,
        use_chat_model: bool,
        flat_presets: &[FlatPreset],
        current_model: &str,
        current_effort: ReasoningEffort,
    ) -> usize {
        if include_follow_chat && use_chat_model {
            return usize::from(include_fast_mode) + usize::from(include_context_mode);
        }

        if let Some((idx, _)) = flat_presets.iter().enumerate().find(|(_, preset)| {
            preset.model.eq_ignore_ascii_case(current_model) && preset.effort == current_effort
        }) {
            return idx
                + usize::from(include_fast_mode)
                + usize::from(include_context_mode)
                + usize::from(include_follow_chat);
        }

        if let Some((idx, _)) = flat_presets
            .iter()
            .enumerate()
            .find(|(_, preset)| preset.model.eq_ignore_ascii_case(current_model))
        {
            return idx
                + usize::from(include_fast_mode)
                + usize::from(include_context_mode)
                + usize::from(include_follow_chat);
        }

        if include_follow_chat {
            if flat_presets.is_empty() {
                usize::from(include_fast_mode) + usize::from(include_context_mode)
            } else {
                usize::from(include_fast_mode) + usize::from(include_context_mode) + 1
            }
        } else if include_fast_mode {
            if flat_presets.is_empty() {
                0
            } else {
                usize::from(include_fast_mode) + usize::from(include_context_mode)
            }
        } else {
            0
        }
    }

    fn supports_extended_context(&self) -> bool {
        supports_extended_context(&self.current_model)
    }

    fn format_model_header(model: &str) -> String {
        let mut parts = Vec::new();
        for (idx, part) in model.split('-').enumerate() {
            if idx == 0 {
                parts.push(part.to_ascii_uppercase());
                continue;
            }

            let mut chars = part.chars();
            let formatted = match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    let mut s = String::new();
                    s.push(first.to_ascii_uppercase());
                    s.push_str(chars.as_str());
                    s
                }
                Some(first) => {
                    let mut s = String::new();
                    s.push(first);
                    s.push_str(chars.as_str());
                    s
                }
                None => String::new(),
            };
            parts.push(formatted);
        }

        parts.join("-")
    }

    fn entries(&self) -> Vec<EntryKind> {
        let mut entries = Vec::new();
        if self.target.supports_fast_mode() {
            entries.push(EntryKind::FastMode);
            entries.push(EntryKind::ContextMode);
        }
        if self.target.supports_follow_chat() {
            entries.push(EntryKind::FollowChat);
        }
        for idx in self.sorted_indices() {
            entries.push(EntryKind::Preset(idx));
        }
        entries
    }

    fn move_selection_up(&mut self) {
        let total = self.entries().len();
        if total == 0 {
            return;
        }
        self.selected_index = if self.selected_index == 0 {
            total - 1
        } else {
            self.selected_index.saturating_sub(1)
        };
        self.hovered_index = None; // Clear hover when using keyboard
        self.ensure_selected_visible();
    }

    fn move_selection_down(&mut self) {
        let total = self.entries().len();
        if total == 0 {
            return;
        }
        self.selected_index = (self.selected_index + 1) % total;
        self.hovered_index = None; // Clear hover when using keyboard
        self.ensure_selected_visible();
    }

    /// Ensure the selected item is visible within the scroll window
    fn ensure_selected_visible(&mut self) {
        let visible_height = *self.last_render_height.borrow() as usize;
        if visible_height == 0 {
            return;
        }
        // Reserve space for header (3 lines) and footer (2 lines)
        let content_height = visible_height.saturating_sub(5);
        if content_height == 0 {
            return;
        }

        // Get the line number where the selected item would be rendered
        let selected_line = self.get_entry_line(self.selected_index);

        // Scroll up if selected is above visible area
        if selected_line < self.scroll_offset {
            self.scroll_offset = selected_line;
        }
        // Scroll down if selected is below visible area
        let visible_end = self.scroll_offset + content_height;
        if selected_line >= visible_end {
            self.scroll_offset = selected_line.saturating_sub(content_height) + 1;
        }
    }

    /// Get the line number where an entry would be rendered (0-indexed from content start)
    fn get_entry_line(&self, entry_index: usize) -> usize {
        let entries = self.entries();
        let mut line: usize = 3;

        if self.target.supports_fast_mode() {
            if entry_index == 0 {
                return line + 2;
            }
            line += 4;

            if entry_index == 1 {
                return line + 2;
            }
            line += 4;
        }

        // "Follow Chat Mode" section if applicable
        if self.target.supports_follow_chat() {
            // Header + description + entry + spacer = 4 lines
            let follow_chat_index = if self.target.supports_fast_mode() { 2 } else { 0 };
            if entry_index == follow_chat_index {
                return line + 2;
            }
            line += 4;
        }

        let mut previous_model: Option<&str> = None;
        for (idx, entry) in entries.iter().enumerate() {
            if matches!(entry, EntryKind::FastMode) {
                continue;
            }
            if matches!(entry, EntryKind::FollowChat) {
                continue; // Already handled
            }
            let EntryKind::Preset(preset_index) = entry else { continue };
            let flat_preset = &self.flat_presets[*preset_index];
            let is_new_model = previous_model
                .map(|m| !m.eq_ignore_ascii_case(&flat_preset.model))
                .unwrap_or(true);

            if is_new_model {
                if previous_model.is_some() {
                    line += 1; // Spacer between models
                }
                line += 1; // Model header
                if Self::model_description(&flat_preset.model).is_some() {
                    line += 1; // Model description
                }
                previous_model = Some(&flat_preset.model);
            }

            if idx == entry_index {
                return line;
            }
            line += 1; // The entry itself
        }
        line
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        let total_lines = self.content_line_count() as usize;
        let visible_height = *self.last_render_height.borrow() as usize;
        let content_height = visible_height.saturating_sub(5); // Header + footer
        let max_scroll = total_lines.saturating_sub(content_height);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    fn select_item(&mut self, index: usize) {
        let total = self.entries().len();
        if index >= total {
            return;
        }
        self.selected_index = index;
        self.confirm_selection();
    }

    /// Find which entry index a screen position corresponds to
    fn hit_test(&self, x: u16, y: u16) -> Option<usize> {
        let item_rects = self.item_rects.borrow();
        for (entry_idx, rect) in item_rects.iter() {
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                return Some(*entry_idx);
            }
        }
        None
    }

    fn confirm_selection(&mut self) {
        let entries = self.entries();
        if let Some(entry) = entries.get(self.selected_index) {
            match entry {
                EntryKind::FastMode => {
                    let next_service_tier = if matches!(self.current_service_tier, Some(ServiceTier::Fast)) {
                        None
                    } else {
                        Some(ServiceTier::Fast)
                    };
                    self.current_service_tier = next_service_tier;
                    self.app_event_tx.send(AppEvent::UpdateServiceTierSelection {
                        service_tier: next_service_tier,
                    });
                }
                EntryKind::ContextMode => {
                    let next_context_mode = match self.current_context_mode {
                        None | Some(ContextMode::Disabled) => Some(ContextMode::OneM),
                        Some(ContextMode::OneM) => Some(ContextMode::Auto),
                        Some(ContextMode::Auto) => Some(ContextMode::Disabled),
                    };
                    self.current_context_mode = next_context_mode;
                    self.app_event_tx.send(AppEvent::UpdateSessionContextModeSelection {
                        context_mode: next_context_mode,
                    });
                }
                EntryKind::FollowChat => {
                    self.use_chat_model = true;
                    match self.target {
                        ModelSelectionTarget::Session => {}
                        ModelSelectionTarget::Review => {
                            self.app_event_tx.send(AppEvent::UpdateReviewUseChatModel(true));
                        }
                        ModelSelectionTarget::Planning => {
                            self
                                .app_event_tx
                                .send(AppEvent::UpdatePlanningUseChatModel(true));
                        }
                        ModelSelectionTarget::AutoDrive => {
                            self
                                .app_event_tx
                                .send(AppEvent::UpdateAutoDriveUseChatModel(true));
                        }
                        ModelSelectionTarget::ReviewResolve => {
                            self
                                .app_event_tx
                                .send(AppEvent::UpdateReviewResolveUseChatModel(true));
                        }
                        ModelSelectionTarget::AutoReview => {
                            self
                                .app_event_tx
                                .send(AppEvent::UpdateAutoReviewUseChatModel(true));
                        }
                        ModelSelectionTarget::AutoReviewResolve => {
                            self
                                .app_event_tx
                                .send(AppEvent::UpdateAutoReviewResolveUseChatModel(true));
                        }
                    }
                    self.send_closed(true);
                }
                EntryKind::Preset(idx) => {
                    if let Some(flat_preset) = self.flat_presets.get(*idx) {
                        self.current_model = flat_preset.model.clone();
                        self.current_effort = flat_preset.effort;
                        self.use_chat_model = false;
                        match self.target {
                            ModelSelectionTarget::Session => {
                                self.app_event_tx.send(AppEvent::UpdateModelSelection {
                                    model: flat_preset.model.clone(),
                                    effort: Some(flat_preset.effort),
                                });
                            }
                            ModelSelectionTarget::Review => {
                                self
                                    .app_event_tx
                                    .send(AppEvent::UpdateReviewModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::Planning => {
                                self
                                    .app_event_tx
                                    .send(AppEvent::UpdatePlanningModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::AutoDrive => {
                                self
                                    .app_event_tx
                                    .send(AppEvent::UpdateAutoDriveModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::ReviewResolve => {
                                self
                                    .app_event_tx
                                    .send(AppEvent::UpdateReviewResolveModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::AutoReview => {
                                self
                                    .app_event_tx
                                    .send(AppEvent::UpdateAutoReviewModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::AutoReviewResolve => {
                                self
                                    .app_event_tx
                                    .send(AppEvent::UpdateAutoReviewResolveModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                        }
                    }
                    self.send_closed(true);
                }
            }
        }
    }

    fn content_line_count(&self) -> u16 {
        let mut lines: u16 = 3;
        if self.target.supports_fast_mode() {
            lines = lines.saturating_add(8);
        }
        if self.target.supports_follow_chat() {
            // Header + description + entry + spacer
            lines = lines.saturating_add(4);
        }

        let mut previous_model: Option<&str> = None;
        for idx in self.sorted_indices() {
            let flat_preset = &self.flat_presets[idx];
            let is_new_model = previous_model
                .map(|prev| !prev.eq_ignore_ascii_case(&flat_preset.model))
                .unwrap_or(true);

            if is_new_model {
                if previous_model.is_some() {
                    lines = lines.saturating_add(1);
                }
                lines = lines.saturating_add(1);
                if Self::model_description(&flat_preset.model).is_some() {
                    lines = lines.saturating_add(1);
                }
                previous_model = Some(&flat_preset.model);
            }

            lines = lines.saturating_add(1);
        }

        lines.saturating_add(2)
    }

    fn sorted_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.flat_presets.len()).collect();
        indices.sort_by(|&a, &b| Self::compare_presets(&self.flat_presets[a], &self.flat_presets[b]));
        indices
    }

    fn compare_presets(a: &FlatPreset, b: &FlatPreset) -> Ordering {
        let model_rank = Self::model_rank(&a.model).cmp(&Self::model_rank(&b.model));
        if model_rank != Ordering::Equal {
            return model_rank;
        }

        let model_name_rank = a
            .model
            .to_ascii_lowercase()
            .cmp(&b.model.to_ascii_lowercase());
        if model_name_rank != Ordering::Equal {
            return model_name_rank;
        }

        let effort_rank = Self::effort_rank(a.effort).cmp(&Self::effort_rank(b.effort));
        if effort_rank != Ordering::Equal {
            return effort_rank;
        }

        a.label.cmp(&b.label)
    }

    fn model_rank(model: &str) -> u8 {
        if model.eq_ignore_ascii_case("gpt-5.4") {
            0
        } else if model.eq_ignore_ascii_case("gpt-5.3-codex") {
            1
        } else if model.eq_ignore_ascii_case("gpt-5.3-codex-spark") {
            2
        } else if model.eq_ignore_ascii_case("gpt-5.2-codex") {
            3
        } else if model.eq_ignore_ascii_case("gpt-5.2") {
            4
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex-max") {
            5
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex") {
            6
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex-mini") {
            7
        } else if model.eq_ignore_ascii_case("gpt-5.1") {
            8
        } else {
            9
        }
    }

    fn model_description(model: &str) -> Option<&'static str> {
        if model.eq_ignore_ascii_case("gpt-5.4") {
            Some("Brings together flagship reasoning, coding, and tool use in a single frontier model.")
        } else if model.eq_ignore_ascii_case("gpt-5.3-codex") {
            Some("Frontier agentic coding, 25% faster than previous models.")
        } else if model.eq_ignore_ascii_case("gpt-5.3-codex-spark") {
            Some("Fast codex variant tuned for responsive coding loops and smaller edits.")
        } else if model.eq_ignore_ascii_case("gpt-5.2-codex") {
            Some("Frontier agentic coding model.")
        } else if model.eq_ignore_ascii_case("gpt-5.2") {
            Some("Latest frontier model with improvements across knowledge, reasoning, and coding.")
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex-max") {
            Some("Latest Codex-optimized flagship for deep and fast reasoning.")
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex") {
            Some("Optimized for Code.")
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex-mini") {
            Some("Optimized for Code. Cheaper, faster, but less capable.")
        } else if model.eq_ignore_ascii_case("gpt-5.1") {
            Some("Broad world knowledge with strong general reasoning.")
        } else {
            None
        }
    }

    fn effort_rank(effort: ReasoningEffort) -> u8 {
        match effort {
            ReasoningEffort::XHigh => 0,
            ReasoningEffort::High => 1,
            ReasoningEffort::Medium => 2,
            ReasoningEffort::Low => 3,
            ReasoningEffort::Minimal => 4,
            ReasoningEffort::None => 5,
        }
    }

    fn effort_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::XHigh => "XHigh",
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
        }
    }
}

impl ModelSelectionView {
    fn entry_count(&self) -> usize {
        self.flat_presets.len()
            + usize::from(self.target.supports_fast_mode())
            + usize::from(self.target.supports_fast_mode())
            + usize::from(self.target.supports_follow_chat())
    }

    fn handle_mouse_event_shared(&mut self, mouse_event: MouseEvent) -> ConditionalUpdate {
        let mut selected = self.selected_index;
        let result = route_selectable_list_mouse_with_config(
            mouse_event,
            &mut selected,
            self.entry_count(),
            |x, y| self.hit_test(x, y),
            SelectableListMouseConfig {
                hover_select: false,
                scroll_select: false,
                ..SelectableListMouseConfig::default()
            },
        );
        self.selected_index = selected;

        if matches!(result, SelectableListMouseResult::Activated) {
            self.select_item(self.selected_index);
            return ConditionalUpdate::NeedsRedraw;
        }

        match mouse_event.kind {
            MouseEventKind::Moved => {
                let new_hover = self.hit_test(mouse_event.column, mouse_event.row);
                if new_hover != self.hovered_index {
                    self.hovered_index = new_hover;
                    return ConditionalUpdate::NeedsRedraw;
                }
            }
            MouseEventKind::ScrollUp => {
                self.scroll_up();
                return ConditionalUpdate::NeedsRedraw;
            }
            MouseEventKind::ScrollDown => {
                self.scroll_down();
                return ConditionalUpdate::NeedsRedraw;
            }
            _ => {}
        }

        if result.handled() {
            ConditionalUpdate::NeedsRedraw
        } else {
            ConditionalUpdate::NoRedraw
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.move_selection_up();
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.move_selection_down();
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                self.confirm_selection();
                true
            }
            KeyEvent { code: KeyCode::Esc, modifiers: KeyModifiers::NONE, .. } => {
                self.send_closed(false);
                true
            }
            _ => false,
        }
    }

    /// Handle mouse events directly without needing a BottomPane reference.
    /// Used when embedded in settings overlay.
    pub(crate) fn handle_mouse_event_direct(&mut self, mouse_event: MouseEvent, _area: Rect) -> ConditionalUpdate {
        self.handle_mouse_event_shared(mouse_event)
    }

    fn send_closed(&mut self, accepted: bool) {
        if self.is_complete {
            return;
        }
        self.app_event_tx.send(AppEvent::ModelSelectionClosed {
            target: self.target.into(),
            accepted,
        });
        self.is_complete = true;
    }

    fn render_panel_body(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Store render height for scroll calculations
        *self.last_render_height.borrow_mut() = area.height;

        // Clear item rects and rebuild during render
        let mut item_rects = self.item_rects.borrow_mut();
        item_rects.clear();

        let padded = Rect {
            x: area.x.saturating_add(1),
            y: area.y,
            width: area.width.saturating_sub(1),
            height: area.height,
        };

        let mut lines: Vec<Line> = Vec::new();
        // Track absolute line numbers for item_rects (before scroll offset)
        let mut current_line: usize = 0;

        lines.push(Line::from(vec![
            Span::styled(
                format!("{}: ", self.target.current_label()),
                Style::default().fg(crate::colors::text_dim()),
            ),
            Span::styled(
                if self.target.supports_follow_chat() && self.use_chat_model {
                    "Follow Chat Mode".to_string()
                } else {
                    Self::format_model_header(&self.current_model)
                },
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        current_line += 1;

        lines.push(Line::from(vec![
            Span::styled(
                format!("{}: ", self.target.reasoning_label()),
                Style::default().fg(crate::colors::text_dim()),
            ),
            Span::styled(
                if self.target.supports_follow_chat() && self.use_chat_model {
                    "From chat".to_string()
                } else {
                    Self::effort_label(self.current_effort).to_string()
                },
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        current_line += 1;

        lines.push(Line::from(""));
        current_line += 1;

        if self.target.supports_fast_mode() {
            let fast_index = 0;
            let is_selected = self.selected_index == fast_index;
            let is_hovered = self.hovered_index == Some(fast_index);
            let is_highlighted = is_selected || is_hovered;
            let fast_enabled = matches!(self.current_service_tier, Some(ServiceTier::Fast));
            let status = if fast_enabled { "enabled" } else { "disabled" };

            let header_style = Style::default()
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD);
            let desc_style = Style::default().fg(crate::colors::text_dim());
            lines.push(Line::from(vec![Span::styled("Fast mode", header_style)]));
            current_line += 1;

            lines.push(Line::from(vec![Span::styled(
                "Same model, but 1.5x faster responses (uses 2x credits)",
                desc_style,
            )]));
            current_line += 1;

            let mut label_style = Style::default().fg(crate::colors::text());
            if is_highlighted {
                label_style = label_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            if fast_enabled {
                label_style = label_style.fg(crate::colors::success());
            }

            let arrow = if is_selected { "› " } else { "  " };
            let arrow_style = if is_highlighted {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::text_dim())
            };

            let screen_line = current_line.saturating_sub(self.scroll_offset);
            if current_line >= self.scroll_offset && screen_line < area.height as usize {
                item_rects.push((fast_index, Rect {
                    x: padded.x,
                    y: padded.y + screen_line as u16,
                    width: padded.width,
                    height: 1,
                }));
            }

            lines.push(Line::from(vec![
                Span::styled(arrow, arrow_style),
                Span::styled(format!("Fast mode: {status}"), label_style),
            ]));
            current_line += 1;

            lines.push(Line::from(""));
            current_line += 1;

            let context_index = 1;
            let is_selected = self.selected_index == context_index;
            let is_hovered = self.hovered_index == Some(context_index);
            let is_highlighted = is_selected || is_hovered;
            let context_status = match self.current_context_mode {
                Some(ContextMode::OneM) => "enabled",
                Some(ContextMode::Auto) => "auto",
                Some(ContextMode::Disabled) | None => "disabled",
            };
            let context_available = self.supports_extended_context();

            let header_style = Style::default()
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD);
            let desc_style = Style::default().fg(crate::colors::text_dim());
            lines.push(Line::from(vec![Span::styled("Mode Settings", header_style)]));
            current_line += 1;

            lines.push(Line::from(vec![Span::styled(
                "Fast mode speeds up replies. 1M Context is available on supported models.",
                desc_style,
            )]));
            current_line += 1;

            let mut label_style = Style::default().fg(crate::colors::text());
            if is_highlighted {
                label_style = label_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            if self.current_context_mode.is_some() {
                label_style = label_style.fg(crate::colors::success());
            }
            if !context_available {
                label_style = label_style.fg(crate::colors::text_dim());
            }

            let context_arrow = if is_selected { "› " } else { "  " };
            let context_arrow_style = if is_highlighted {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::text_dim())
            };

            let screen_line = current_line.saturating_sub(self.scroll_offset);
            if current_line >= self.scroll_offset && screen_line < area.height as usize {
                item_rects.push((context_index, Rect {
                    x: padded.x,
                    y: padded.y + screen_line as u16,
                    width: padded.width,
                    height: 1,
                }));
            }

            lines.push(Line::from(vec![
                Span::styled(context_arrow, context_arrow_style),
                Span::styled(format!("1M Context: {context_status}"), label_style),
            ]));
            current_line += 1;

            if !context_available {
                lines.push(Line::from(vec![Span::styled(
                    "Unavailable for this model. Saved settings apply automatically on supported models.",
                    desc_style,
                )]));
                current_line += 1;
            }

            lines.push(Line::from(""));
            current_line += 1;
        }

        if self.target.supports_follow_chat() {
            let follow_chat_index = if self.target.supports_fast_mode() { 2 } else { 0 };
            let is_selected = self.selected_index == follow_chat_index;
            let is_hovered = self.hovered_index == Some(follow_chat_index);
            let is_highlighted = is_selected || is_hovered;

            let header_style = Style::default()
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD);
            let desc_style = Style::default().fg(crate::colors::text_dim());
            lines.push(Line::from(vec![Span::styled("Follow Chat Mode", header_style)]));
            current_line += 1;

            lines.push(Line::from(vec![Span::styled(
                "Use the active chat model and reasoning; stays in sync as chat changes.",
                desc_style,
            )]));
            current_line += 1;

            let mut label_style = Style::default().fg(crate::colors::text());
            if is_highlighted {
                label_style = label_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            let mut arrow_style = Style::default().fg(crate::colors::text_dim());
            if is_highlighted {
                arrow_style = label_style;
            }
            let indent_style = if is_highlighted {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mut status = String::new();
            if self.use_chat_model {
                status.push_str("(current)");
            }
            let arrow = if is_selected { "› " } else { "  " };
            let mut spans = vec![
                Span::styled(arrow, arrow_style),
                Span::styled("   ", indent_style),
                Span::styled("Use chat model", label_style),
            ];
            if !status.is_empty() {
                spans.push(Span::raw(format!("  {status}")));
            }

            // Store the rect for the "Follow Chat Mode" entry.
            // Adjust y position by scroll offset for screen coordinates
            let screen_line = current_line.saturating_sub(self.scroll_offset);
            if current_line >= self.scroll_offset && screen_line < area.height as usize {
                item_rects.push((follow_chat_index, Rect {
                    x: padded.x,
                    y: padded.y + screen_line as u16,
                    width: padded.width,
                    height: 1,
                }));
            }

            lines.push(Line::from(spans));
            current_line += 1;

            lines.push(Line::from(""));
            current_line += 1;
        }

        let mut previous_model: Option<&str> = None;
        let entries = self.entries();
        for (entry_idx, entry) in entries.iter().enumerate() {
            if matches!(entry, EntryKind::FastMode | EntryKind::ContextMode | EntryKind::FollowChat) {
                continue;
            }
            let EntryKind::Preset(preset_index) = entry else { continue };
            let flat_preset = &self.flat_presets[*preset_index];
            if previous_model
                .map(|m| !m.eq_ignore_ascii_case(&flat_preset.model))
                .unwrap_or(true)
            {
                if previous_model.is_some() {
                    lines.push(Line::from(""));
                    current_line += 1;
                }
                lines.push(Line::from(vec![Span::styled(
                    Self::format_model_header(&flat_preset.model),
                    Style::default()
                        .fg(crate::colors::text_bright())
                        .add_modifier(Modifier::BOLD),
                )]));
                current_line += 1;

                if let Some(desc) = Self::model_description(&flat_preset.model) {
                    lines.push(Line::from(vec![Span::styled(
                        desc,
                        Style::default().fg(crate::colors::text_dim()),
                    )]));
                    current_line += 1;
                }
                previous_model = Some(&flat_preset.model);
            }

            let is_selected = entry_idx == self.selected_index;
            let is_hovered = self.hovered_index == Some(entry_idx);
            let is_highlighted = is_selected || is_hovered;
            let is_current = !self.use_chat_model
                && flat_preset.model.eq_ignore_ascii_case(&self.current_model)
                && flat_preset.effort == self.current_effort;
            let label = Self::effort_label(flat_preset.effort);
            let mut row_text = label.to_string();
            if is_current {
                row_text.push_str(" (current)");
            }

            let mut indent_style = Style::default();
            if is_highlighted {
                indent_style = indent_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }

            let mut label_style = Style::default().fg(crate::colors::text());
            if is_highlighted {
                label_style = label_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            if is_current {
                label_style = label_style.fg(crate::colors::success());
            }

            let mut divider_style = Style::default().fg(crate::colors::text_dim());
            if is_highlighted {
                divider_style = divider_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }

            let mut description_style = Style::default().fg(crate::colors::dim());
            if is_highlighted {
                description_style = description_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }

            // Store the rect for this entry - adjust y by scroll offset
            let screen_line = current_line.saturating_sub(self.scroll_offset);
            if current_line >= self.scroll_offset && screen_line < area.height as usize {
                item_rects.push((entry_idx, Rect {
                    x: padded.x,
                    y: padded.y + screen_line as u16,
                    width: padded.width,
                    height: 1,
                }));
            }

            lines.push(Line::from(vec![
                Span::styled("   ", indent_style),
                Span::styled(row_text, label_style),
                Span::styled(" - ", divider_style),
                Span::styled(&flat_preset.description, description_style),
            ]));
            current_line += 1;
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(crate::colors::light_blue())),
            Span::raw(" Navigate  "),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::raw(" Select  "),
            Span::styled("Esc", Style::default().fg(crate::colors::error())),
            Span::raw(" Cancel"),
        ]));

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .scroll((self.scroll_offset as u16, 0))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .render(padded, buf);
    }

    pub(crate) fn render_without_frame(&self, area: Rect, buf: &mut Buffer) {
        self.render_panel_body(area, buf);
    }
}

impl<'a> BottomPaneView<'a> for ModelSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_direct(key_event);
    }

    fn handle_key_event_with_result(
        &mut self,
        _pane: &mut BottomPane<'a>,
        key_event: KeyEvent,
    ) -> ConditionalUpdate {
        redraw_if(self.handle_key_event_direct(key_event))
    }

    fn handle_mouse_event(
        &mut self,
        _pane: &mut BottomPane<'a>,
        mouse_event: MouseEvent,
        _area: Rect,
    ) -> ConditionalUpdate {
        self.handle_mouse_event_shared(mouse_event)
    }

    fn update_hover(&mut self, mouse_pos: (u16, u16), _area: Rect) -> bool {
        let new_hover = self.hit_test(mouse_pos.0, mouse_pos.1);
        if new_hover != self.hovered_index {
            self.hovered_index = new_hover;
            true
        } else {
            false
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let content_lines = self.content_line_count();
        let total = content_lines.saturating_add(2);
        total.max(9)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        render_panel(
            area,
            buf,
            self.target.panel_title(),
            PanelFrameStyle::bottom_pane(),
            |inner, buf| self.render_panel_body(inner, buf),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use code_common::model_presets::ReasoningEffortPreset;
    use std::sync::mpsc;

    fn preset(model: &str) -> ModelPreset {
        preset_with_effort(model, ReasoningEffort::Medium)
    }

    fn preset_with_effort(model: &str, effort: ReasoningEffort) -> ModelPreset {
        ModelPreset {
            id: model.to_string(),
            model: model.to_string(),
            display_name: model.to_string(),
            description: format!("preset for {model}"),
            default_reasoning_effort: effort.into(),
            supported_reasoning_efforts: vec![ReasoningEffortPreset {
                effort: effort.into(),
                description: effort.to_string().to_ascii_lowercase(),
            }],
            supported_text_verbosity: &[],
            is_default: false,
            upgrade: None,
            pro_only: false,
            show_in_picker: true,
        }
    }

    fn make_view(target: ModelSelectionTarget, presets: Vec<ModelPreset>) -> ModelSelectionView {
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        ModelSelectionView::new(
            ModelSelectionViewParams {
                presets,
                current_model: "unknown-model".to_string(),
                current_effort: ReasoningEffort::Medium,
                current_service_tier: None,
                current_context_mode: None,
                use_chat_model: false,
                target,
            },
            AppEventSender::new(tx),
        )
    }

    #[test]
    fn session_initial_selection_prefers_first_preset_after_fast_mode() {
        let view = make_view(ModelSelectionTarget::Session, vec![preset("gpt-5.3-codex")]);
        assert_eq!(view.selected_index, 2);
    }

    #[test]
    fn session_initial_selection_with_no_presets_uses_fast_mode() {
        let view = make_view(ModelSelectionTarget::Session, Vec::new());
        assert_eq!(view.selected_index, 0);
    }

    #[test]
    fn entry_count_includes_fast_mode() {
        let view = make_view(ModelSelectionTarget::Session, vec![preset("gpt-5.3-codex")]);
        assert_eq!(view.entry_count(), 3);
    }

    #[test]
    fn get_entry_line_accounts_for_header_and_fast_block() {
        let view = make_view(ModelSelectionTarget::Session, vec![preset("gpt-5.3-codex")]);
        assert_eq!(view.get_entry_line(0), 5);
        assert_eq!(view.get_entry_line(1), 9);
        assert_eq!(view.get_entry_line(2), 15);
    }

    #[test]
    fn selecting_preset_updates_local_current_model_state() {
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            ModelSelectionViewParams {
                presets: vec![preset_with_effort("gpt-5.3-codex", ReasoningEffort::High)],
                current_model: "gpt-5.4".to_string(),
                current_effort: ReasoningEffort::Medium,
                current_service_tier: None,
                current_context_mode: None,
                use_chat_model: false,
                target: ModelSelectionTarget::Session,
            },
            AppEventSender::new(tx),
        );

        view.select_item(2);

        assert_eq!(view.current_model, "gpt-5.3-codex");
        assert_eq!(view.current_effort, ReasoningEffort::High);
        assert!(!view.use_chat_model);
    }

    #[test]
    fn selecting_follow_chat_updates_local_follow_chat_state() {
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            ModelSelectionViewParams {
                presets: vec![preset("gpt-5.3-codex")],
                current_model: "gpt-5.4".to_string(),
                current_effort: ReasoningEffort::Medium,
                current_service_tier: None,
                current_context_mode: None,
                use_chat_model: false,
                target: ModelSelectionTarget::Review,
            },
            AppEventSender::new(tx),
        );

        view.select_item(0);

        assert!(view.use_chat_model);
    }

    #[test]
    fn selecting_context_mode_sends_session_context_mode_update() {
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            ModelSelectionViewParams {
                presets: vec![preset("gpt-5.4")],
                current_model: "gpt-5.4".to_string(),
                current_effort: ReasoningEffort::Medium,
                current_service_tier: None,
                current_context_mode: Some(ContextMode::Disabled),
                use_chat_model: false,
                target: ModelSelectionTarget::Session,
            },
            AppEventSender::new(tx),
        );

        view.select_item(1);

        assert_eq!(view.current_context_mode, Some(ContextMode::OneM));
        match rx.recv().expect("context mode event") {
            AppEvent::UpdateSessionContextModeSelection { context_mode } => {
                assert_eq!(context_mode, Some(ContextMode::OneM));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
