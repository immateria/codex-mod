use std::cell::Cell;
use std::collections::BTreeMap;

use code_core::config_types::FeaturesToml;

use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

mod input;
mod mouse;
mod pages;
mod pane_impl;
mod render;
#[cfg(test)]
mod tests;

use crate::timing::DEFAULT_LIST_VIEWPORT_ROWS;

#[derive(Clone, Debug)]
struct ExperimentalFeatureRow {
    key: &'static str,
    name: &'static str,
    description: &'static str,
    default_enabled: bool,
}

pub(crate) struct ExperimentalFeaturesSettingsView {
    rows: Vec<ExperimentalFeatureRow>,
    list_state: Cell<ScrollState>,
    list_viewport_rows: Cell<usize>,
    baseline_enabled: Vec<bool>,
    draft_enabled: Vec<bool>,
    dirty: bool,
    active_profile: Option<String>,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

crate::bottom_pane::chrome_view::impl_chrome_view!(ExperimentalFeaturesSettingsView);

impl ExperimentalFeaturesSettingsView {
    pub(crate) fn new(
        active_profile: Option<String>,
        features_effective: FeaturesToml,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut rows = Vec::new();
        for spec in code_features::FEATURES {
            let code_features::Stage::Experimental { name, menu_description, .. } = spec.stage else {
                continue;
            };
            rows.push(ExperimentalFeatureRow {
                key: spec.key,
                name,
                description: menu_description,
                default_enabled: spec.default_enabled,
            });
        }

        let mut baseline_enabled = Vec::with_capacity(rows.len());
        for row in &rows {
            baseline_enabled.push(
                features_effective
                    .get_bool(row.key)
                    .unwrap_or(row.default_enabled),
            );
        }
        let draft_enabled = baseline_enabled.clone();

        let list_state = ScrollState::with_first_selected();

        Self {
            rows,
            list_state: Cell::new(list_state),
            list_viewport_rows: Cell::new(DEFAULT_LIST_VIEWPORT_ROWS),
            baseline_enabled,
            draft_enabled,
            dirty: false,
            active_profile,
            app_event_tx,
            is_complete: false,
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn close(&mut self) {
        self.is_complete = true;
    }

    fn feature_count(&self) -> usize {
        self.rows.len()
    }

    fn toggle_selected(&mut self) -> bool {
        let total = self.feature_count();
        if total == 0 {
            return false;
        }
        let idx = self.list_state.get().selected_idx.unwrap_or(0).min(total - 1);
        if let Some(current) = self.draft_enabled.get_mut(idx) {
            *current = !*current;
            self.dirty = self.draft_enabled != self.baseline_enabled;
            true
        } else {
            false
        }
    }

    fn request_save(&mut self) -> bool {
        let mut updates: BTreeMap<String, bool> = BTreeMap::new();
        for (idx, row) in self.rows.iter().enumerate() {
            let enabled = self.draft_enabled.get(idx).copied().unwrap_or(row.default_enabled);
            updates.insert(row.key.to_string(), enabled);
        }
        self.app_event_tx
            .send(crate::app_event::AppEvent::UpdateFeatureFlags { updates });

        self.baseline_enabled = self.draft_enabled.clone();
        self.dirty = false;
        true
    }
}

