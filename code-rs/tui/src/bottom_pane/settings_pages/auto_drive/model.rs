use super::*;

use crate::app_event::{AppEvent, AutoDriveSettingsUpdate};
use code_core::config_types::default_auto_drive_model_routing_entries;

impl AutoDriveSettingsView {
    pub(super) const PANEL_TITLE: &'static str = "Auto Drive Settings";

    pub fn new(init: AutoDriveSettingsInit) -> Self {
        let AutoDriveSettingsInit {
            app_event_tx,
            model,
            model_reasoning,
            use_chat_model,
            review_enabled,
            agents_enabled,
            cross_check_enabled,
            qa_automation_enabled,
            model_routing_enabled,
            model_routing_entries,
            routing_model_options,
            continue_mode,
        } = init;
        let diagnostics_enabled = qa_automation_enabled && (review_enabled || cross_check_enabled);
        let normalized_entries = Self::sanitize_routing_entries(model_routing_entries);
        let model_routing_entries = if normalized_entries.is_empty() {
            default_auto_drive_model_routing_entries()
        } else {
            normalized_entries
        };
        let routing_model_options =
            Self::build_routing_model_options(routing_model_options, &model_routing_entries);

        Self {
            app_event_tx,
            main_state: ScrollState {
                selected_idx: Some(0),
                scroll_top: 0,
            },
            mode: AutoDriveSettingsMode::Main,
            hovered: None,
            model,
            model_reasoning,
            use_chat_model,
            review_enabled,
            agents_enabled,
            cross_check_enabled,
            qa_automation_enabled,
            diagnostics_enabled,
            model_routing_enabled,
            model_routing_entries,
            routing_model_options,
            routing_state: ScrollState {
                selected_idx: Some(0),
                scroll_top: 0,
            },
            routing_viewport_rows: Cell::new(8),
            continue_mode,
            status_message: None,
            closing: false,
        }
    }

    pub(super) fn option_count() -> usize {
        6
    }

    pub(super) fn routing_row_count(&self) -> usize {
        self.model_routing_entries.len().saturating_add(1)
    }

    pub(super) fn enabled_routing_entry_count(&self) -> usize {
        self.model_routing_entries
            .iter()
            .filter(|entry| entry.enabled)
            .count()
    }

    pub(super) fn default_routing_model() -> String {
        "gpt-5.3-codex".to_string()
    }

    fn normalize_routing_model(model: &str) -> Option<String> {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return None;
        }

        let normalized = trimmed.to_ascii_lowercase();
        if !normalized.starts_with("gpt-") {
            return None;
        }

        Some(normalized)
    }

    fn normalize_routing_reasoning_levels(levels: &[ReasoningEffort]) -> Vec<ReasoningEffort> {
        let mut normalized = Vec::new();
        for level in ROUTING_REASONING_LEVELS {
            if levels.contains(&level) {
                normalized.push(level);
            }
        }
        normalized
    }

    pub(super) fn sanitize_routing_entries(
        entries: Vec<AutoDriveModelRoutingEntry>,
    ) -> Vec<AutoDriveModelRoutingEntry> {
        let mut normalized_entries = Vec::new();
        for entry in entries {
            let Some(model) = Self::normalize_routing_model(&entry.model) else {
                continue;
            };
            let reasoning_levels = Self::normalize_routing_reasoning_levels(&entry.reasoning_levels);
            if reasoning_levels.is_empty() {
                continue;
            }

            normalized_entries.push(AutoDriveModelRoutingEntry {
                model,
                enabled: entry.enabled,
                reasoning_levels,
                description: entry.description.trim().to_string(),
            });
        }
        normalized_entries
    }

    fn build_routing_model_options(
        model_options: Vec<String>,
        entries: &[AutoDriveModelRoutingEntry],
    ) -> Vec<String> {
        let mut normalized = Vec::new();
        for model in model_options {
            let Some(canonical) = Self::normalize_routing_model(&model) else {
                continue;
            };
            if !normalized.contains(&canonical) {
                normalized.push(canonical);
            }
        }

        for entry in entries {
            if !normalized.contains(&entry.model) {
                normalized.push(entry.model.clone());
            }
        }

        if normalized.is_empty() {
            normalized.push(Self::default_routing_model());
        }

        normalized
    }

    pub(super) fn route_entry_summary(entry: &AutoDriveModelRoutingEntry) -> String {
        let levels = entry
            .reasoning_levels
            .iter()
            .map(|level| Self::reasoning_label(*level).to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("/");
        let mut description = if entry.description.trim().is_empty() {
            "(no description)".to_string()
        } else {
            entry.description.trim().to_string()
        };
        const DESCRIPTION_MAX_CHARS: usize = 64;
        if description.chars().count() > DESCRIPTION_MAX_CHARS {
            let trimmed: String = description
                .chars()
                .take(DESCRIPTION_MAX_CHARS.saturating_sub(3))
                .collect();
            description = format!("{trimmed}...");
        }
        format!("{} · {levels} · {description}", entry.model)
    }

    pub(super) fn set_hovered(&mut self, hovered: Option<HoverTarget>) -> bool {
        if self.hovered == hovered {
            return false;
        }
        self.hovered = hovered;
        true
    }

    pub(super) fn clear_hovered(&mut self) {
        self.hovered = None;
    }

    pub(super) fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    pub(super) fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    pub(super) fn send_update(&self) {
        self.app_event_tx
            .send(AppEvent::AutoDriveSettingsChanged(AutoDriveSettingsUpdate {
                review_enabled: self.review_enabled,
                agents_enabled: self.agents_enabled,
                cross_check_enabled: self.cross_check_enabled,
                qa_automation_enabled: self.qa_automation_enabled,
                model_routing_enabled: self.model_routing_enabled,
                model_routing_entries: self.model_routing_entries.clone(),
                continue_mode: self.continue_mode,
            }));
    }

    pub fn set_model(&mut self, model: String, effort: ReasoningEffort) {
        self.model = model;
        self.model_reasoning = effort;
    }

    pub fn set_use_chat_model(&mut self, use_chat: bool, model: String, effort: ReasoningEffort) {
        self.use_chat_model = use_chat;
        if use_chat {
            self.model = model;
            self.model_reasoning = effort;
        }
    }

    fn set_diagnostics(&mut self, enabled: bool) {
        self.review_enabled = enabled;
        self.cross_check_enabled = enabled;
        self.qa_automation_enabled = enabled;
        self.diagnostics_enabled = self.qa_automation_enabled && (self.review_enabled || self.cross_check_enabled);
    }

    pub(super) fn reasoning_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::XHigh => "XHigh",
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
        }
    }

    pub(super) fn format_model_label(model: &str) -> String {
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

    pub(super) fn cycle_continue_mode(&mut self, forward: bool) {
        self.continue_mode = if forward {
            self.continue_mode.cycle_forward()
        } else {
            self.continue_mode.cycle_backward()
        };
        self.send_update();
    }

    fn open_routing_list(&mut self) {
        self.mode = AutoDriveSettingsMode::RoutingList;
        let rows = self.routing_row_count();
        self.routing_state.clamp_selection(rows);
        let visible = self.routing_viewport_rows.get().max(1);
        self.routing_state.ensure_visible(rows, visible);
        self.clear_status_message();
        self.clear_hovered();
    }

    pub(super) fn open_routing_editor(&mut self, index: Option<usize>) {
        let entry = index.and_then(|idx| self.model_routing_entries.get(idx));
        let state = RoutingEditorState::from_entry(index, entry, &self.routing_model_options);
        self.mode = AutoDriveSettingsMode::RoutingEditor(state);
        self.clear_status_message();
        self.clear_hovered();
    }

    pub(super) fn close_routing_editor(&mut self) {
        self.mode = AutoDriveSettingsMode::RoutingList;
        self.clear_status_message();
        self.clear_hovered();
    }

    pub(super) fn try_toggle_routing_entry_enabled(&mut self, index: usize) {
        let Some(entry) = self.model_routing_entries.get(index).cloned() else {
            return;
        };

        if entry.enabled && self.model_routing_enabled && self.enabled_routing_entry_count() <= 1 {
            self.set_status_message("At least one routing entry must stay enabled.");
            return;
        }

        if let Some(target) = self.model_routing_entries.get_mut(index) {
            target.enabled = !target.enabled;
            self.send_update();
            self.clear_status_message();
        }
    }

    pub(super) fn try_remove_routing_entry(&mut self, index: usize) {
        let Some(entry) = self.model_routing_entries.get(index).cloned() else {
            return;
        };

        if self.model_routing_enabled && entry.enabled && self.enabled_routing_entry_count() <= 1 {
            self.set_status_message("At least one routing entry must stay enabled.");
            return;
        }

        self.model_routing_entries.remove(index);
        if self.model_routing_entries.is_empty() {
            self.model_routing_entries = default_auto_drive_model_routing_entries();
        }
        let row_count = self.routing_row_count();
        self.routing_state.clamp_selection(row_count);
        let visible = self.routing_viewport_rows.get().max(1);
        self.routing_state.ensure_visible(row_count, visible);
        self.send_update();
        self.clear_status_message();
    }

    fn try_set_model_routing_enabled(&mut self, enabled: bool) {
        if enabled && self.enabled_routing_entry_count() == 0 {
            self.set_status_message("Enable at least one routing entry before turning routing on.");
            return;
        }
        self.model_routing_enabled = enabled;
        self.send_update();
        self.clear_status_message();
    }

    pub(super) fn toggle_selected(&mut self) {
        let selected = self
            .main_state
            .selected_idx
            .unwrap_or(0)
            .min(Self::option_count().saturating_sub(1));
        match selected {
            0 => {
                self.app_event_tx.send(AppEvent::ShowAutoDriveModelSelector);
            }
            1 => {
                self.agents_enabled = !self.agents_enabled;
                self.send_update();
            }
            2 => {
                let next = !self.diagnostics_enabled;
                self.set_diagnostics(next);
                self.send_update();
            }
            3 => {
                self.try_set_model_routing_enabled(!self.model_routing_enabled);
            }
            4 => {
                self.open_routing_list();
            }
            5 => self.cycle_continue_mode(true),
            _ => {}
        }
    }

    pub(super) fn close(&mut self) {
        if !self.closing {
            self.closing = true;
            self.app_event_tx.send(AppEvent::CloseAutoDriveSettings);
        }
    }

}
