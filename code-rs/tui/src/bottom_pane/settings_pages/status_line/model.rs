use std::collections::HashSet;

use code_core::config_types::StatusLineLane;
use strum::IntoEnumIterator;

use crate::app_event::AppEvent;
use crate::ui_interaction::{wrap_next, wrap_prev};

use super::{StatusLineChoice, StatusLineItem, StatusLineSetupView};

impl StatusLineSetupView {
    pub(crate) fn new(
        top_current: Option<&[String]>,
        bottom_current: Option<&[String]>,
        primary_lane: StatusLineLane,
        initial_lane: StatusLineLane,
        app_event_tx: crate::app_event_sender::AppEventSender,
    ) -> Self {
        Self {
            app_event_tx,
            top_choices: Self::build_choices(top_current),
            bottom_choices: Self::build_choices(bottom_current),
            top_selected_index: 0,
            bottom_selected_index: 0,
            active_lane: initial_lane,
            primary_lane,
            complete: false,
        }
    }

    fn build_choices(current: Option<&[String]>) -> Vec<StatusLineChoice> {
        let mut seen = HashSet::<String>::new();
        let mut choices = Vec::<StatusLineChoice>::new();

        if let Some(ids) = current {
            for id in ids {
                let Ok(item) = id.parse::<StatusLineItem>() else {
                    continue;
                };
                let key = item.to_string();
                if !seen.insert(key) {
                    continue;
                }
                choices.push(StatusLineChoice {
                    item,
                    enabled: true,
                });
            }
        }

        for item in StatusLineItem::iter() {
            let key = item.to_string();
            if seen.contains(&key) {
                continue;
            }
            choices.push(StatusLineChoice {
                item,
                enabled: false,
            });
        }

        choices
    }

    fn selected_ids_for_lane(&self, lane: StatusLineLane) -> Vec<StatusLineItem> {
        self.choices_for_lane(lane)
            .iter()
            .filter_map(|choice| choice.enabled.then_some(choice.item))
            .collect()
    }

    pub(super) fn preview_text_for_lane(&self, lane: StatusLineLane) -> String {
        self.choices_for_lane(lane)
            .iter()
            .filter(|choice| choice.enabled)
            .map(|choice| choice.item.sample())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub(super) fn choices_for_lane(&self, lane: StatusLineLane) -> &[StatusLineChoice] {
        match lane {
            StatusLineLane::Top => &self.top_choices,
            StatusLineLane::Bottom => &self.bottom_choices,
        }
    }

    pub(super) fn choices_for_active_lane(&self) -> &[StatusLineChoice] {
        self.choices_for_lane(self.active_lane)
    }

    pub(super) fn choices_for_active_lane_mut(&mut self) -> &mut Vec<StatusLineChoice> {
        match self.active_lane {
            StatusLineLane::Top => &mut self.top_choices,
            StatusLineLane::Bottom => &mut self.bottom_choices,
        }
    }

    pub(super) fn selected_index_for_active_lane(&self) -> usize {
        match self.active_lane {
            StatusLineLane::Top => self.top_selected_index,
            StatusLineLane::Bottom => self.bottom_selected_index,
        }
    }

    pub(super) fn set_selected_index_for_active_lane(&mut self, value: usize) {
        match self.active_lane {
            StatusLineLane::Top => self.top_selected_index = value,
            StatusLineLane::Bottom => self.bottom_selected_index = value,
        }
    }

    pub(super) fn switch_active_lane(&mut self) {
        self.active_lane = match self.active_lane {
            StatusLineLane::Top => StatusLineLane::Bottom,
            StatusLineLane::Bottom => StatusLineLane::Top,
        };

        let len = self.choices_for_active_lane().len();
        if len == 0 {
            self.set_selected_index_for_active_lane(0);
            return;
        }

        let idx = self.selected_index_for_active_lane().min(len.saturating_sub(1));
        self.set_selected_index_for_active_lane(idx);
    }

    pub(super) fn toggle_primary_lane(&mut self) {
        self.primary_lane = match self.primary_lane {
            StatusLineLane::Top => StatusLineLane::Bottom,
            StatusLineLane::Bottom => StatusLineLane::Top,
        };
    }

    pub(super) fn move_selection_up(&mut self) {
        let len = self.choices_for_active_lane().len();
        let idx = wrap_prev(self.selected_index_for_active_lane(), len);
        self.set_selected_index_for_active_lane(idx);
    }

    pub(super) fn move_selection_down(&mut self) {
        let len = self.choices_for_active_lane().len();
        let idx = wrap_next(self.selected_index_for_active_lane(), len);
        self.set_selected_index_for_active_lane(idx);
    }

    pub(super) fn toggle_selected(&mut self) {
        let idx = self.selected_index_for_active_lane();
        if let Some(choice) = self.choices_for_active_lane_mut().get_mut(idx) {
            choice.enabled = !choice.enabled;
        }
    }

    pub(super) fn move_selected_left(&mut self) {
        let idx = self.selected_index_for_active_lane();
        if idx == 0 {
            return;
        }
        self.choices_for_active_lane_mut().swap(idx, idx - 1);
        self.set_selected_index_for_active_lane(idx - 1);
    }

    pub(super) fn move_selected_right(&mut self) {
        let idx = self.selected_index_for_active_lane();
        let len = self.choices_for_active_lane().len();
        if idx + 1 >= len {
            return;
        }
        self.choices_for_active_lane_mut().swap(idx, idx + 1);
        self.set_selected_index_for_active_lane(idx + 1);
    }

    pub(super) fn confirm(&mut self) {
        self.app_event_tx.send(AppEvent::StatusLineSetup {
            top_items: self.selected_ids_for_lane(StatusLineLane::Top),
            bottom_items: self.selected_ids_for_lane(StatusLineLane::Bottom),
            primary: self.primary_lane,
        });
        self.complete = true;
    }

    pub(super) fn cancel(&mut self) {
        self.app_event_tx.send(AppEvent::StatusLineSetupCancelled);
        self.complete = true;
    }

    pub(super) fn lane_label(lane: StatusLineLane) -> &'static str {
        match lane {
            StatusLineLane::Top => "Top",
            StatusLineLane::Bottom => "Bottom",
        }
    }
}

