use std::time::Duration;

use code_common::model_presets::ModelPreset;
use code_core::config_types::ContextMode;
use code_core::protocol::{AutoContextPhase, TokenUsage};
use code_file_search::FileMatch;
use code_protocol::custom_prompts::CustomPrompt;
use code_protocol::skills::Skill;

use crate::app_event::AppEvent;
#[cfg(not(any(test, feature = "test-helpers")))]
use crate::thread_spawner;

use super::settings_pages;
use super::{AgentHintLabel, AutoReviewFooterStatus, BottomPane, ChatComposer};

impl<'a> BottomPane<'a> {
    fn schedule_redraw_after(&self, dur: Duration, task_name: &'static str) {
        let redraw_delay = dur + Duration::from_millis(120);
        let tx = self.app_event_tx.clone();

        // Snapshot tests construct a `ChatWidget` without an App event loop, so
        // spinning up long-lived timer threads creates noise and can exhaust the
        // lightweight thread budget. In that environment, just ask the App to
        // schedule a frame instead of spawning a sleeper thread.
        #[cfg(any(test, feature = "test-helpers"))]
        {
            let _ = task_name;
            tx.send(AppEvent::ScheduleFrameIn(redraw_delay));
            return;
        }

        #[cfg(not(any(test, feature = "test-helpers")))]
        {
            let tx_thread = tx.clone();
            if thread_spawner::spawn_lightweight(task_name, move || {
                std::thread::sleep(redraw_delay);
                tx_thread.send(AppEvent::RequestRedraw);
            })
            .is_none()
            {
                tx.send(AppEvent::ScheduleFrameIn(redraw_delay));
            }
        }
    }

    pub fn set_has_chat_history(&mut self, has_history: bool) {
        self.composer.set_has_chat_history(has_history);
    }

    pub(crate) fn set_custom_prompts(&mut self, prompts: Vec<CustomPrompt>) {
        self.custom_prompts = prompts.clone();
        self.composer.set_custom_prompts(prompts);
    }

    pub(crate) fn set_subagent_commands(&mut self, names: Vec<String>) {
        self.composer.set_subagent_commands(names);
    }

    pub(crate) fn custom_prompts(&self) -> &[CustomPrompt] {
        &self.custom_prompts
    }

    pub(crate) fn set_skills(&mut self, skills: Vec<Skill>) {
        self.skills = skills;
    }

    pub(crate) fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// Enable or disable compact compose mode. When enabled, the spacer line
    /// above the input composer is removed so the history can scroll into that
    /// row. This is typically toggled when the user scrolls up.
    pub(crate) fn set_compact_compose(&mut self, compact: bool) {
        self.compact_compose = compact;
        let new_enabled = if self.force_top_spacer { true } else { !compact };
        if self.top_spacer_enabled != new_enabled {
            self.top_spacer_enabled = new_enabled;
            self.request_redraw();
        }
    }

    pub(crate) fn set_force_top_spacer(&mut self, enabled: bool) {
        if self.force_top_spacer == enabled {
            return;
        }
        self.force_top_spacer = enabled;
        let new_enabled = enabled || !self.compact_compose;
        if self.top_spacer_enabled != new_enabled {
            self.top_spacer_enabled = new_enabled;
            self.request_redraw();
        }
    }

    pub(crate) fn top_spacer_enabled(&self) -> bool {
        self.top_spacer_enabled
    }

    /// Update the status indicator text. Shows status as overlay above composer
    /// to allow continued input while processing.
    pub(crate) fn update_status_text(&mut self, text: String) {
        if let Some(view) = self.active_view.as_mut() {
            let _ = view.update_status_text(text.clone());
        }

        // Pass status message to composer for dynamic title display
        self.composer.update_status_message(text);
        self.request_redraw();
    }

    /// Show an ephemeral footer notice for a custom duration.
    pub(crate) fn flash_footer_notice_for(&mut self, text: String, dur: Duration) {
        self.composer.flash_footer_notice_for(text, dur);
        // Ask app to clear it slightly after expiry to avoid flicker on boundary
        self.app_event_tx
            .send(AppEvent::ScheduleFrameIn(dur + Duration::from_millis(100)));
        self.request_redraw();
    }

    pub(crate) fn set_standard_terminal_hint(&mut self, hint: Option<String>) {
        self.composer.set_standard_terminal_hint(hint);
        self.request_redraw();
    }

    pub(crate) fn standard_terminal_hint(&self) -> Option<&str> {
        self.composer.standard_terminal_hint()
    }

    pub(crate) fn set_auto_review_status(&mut self, status: Option<AutoReviewFooterStatus>) {
        self.composer.set_auto_review_status(status);
        self.request_redraw();
    }

    pub(crate) fn set_agent_hint_label(&mut self, label: AgentHintLabel) {
        self.composer.set_agent_hint_label(label);
        self.request_redraw();
    }

    #[cfg(test)]
    pub(crate) fn auto_review_status(&self) -> Option<AutoReviewFooterStatus> {
        self.composer.auto_review_status()
    }

    pub(crate) fn show_ctrl_c_quit_hint(&mut self) {
        self.ctrl_c_quit_hint = true;
        self.composer.set_ctrl_c_quit_hint(true);
        self.request_redraw();
    }

    pub(crate) fn clear_ctrl_c_quit_hint(&mut self) {
        if self.ctrl_c_quit_hint {
            self.ctrl_c_quit_hint = false;
            self.composer.set_ctrl_c_quit_hint(false);
            self.request_redraw();
        }
    }

    pub(crate) fn ctrl_c_quit_hint_visible(&self) -> bool {
        self.ctrl_c_quit_hint
    }

    pub fn set_task_running(&mut self, running: bool) {
        self.is_task_running = running;
        self.composer.set_task_running(running);

        if running {
            // No longer need separate status widget - title shows in composer
            self.request_redraw();
        } else {
            // Status now shown in composer title
            // Drop the status view when a task completes, but keep other
            // modal views (e.g. approval dialogs).
            if let Some(mut view) = self.active_view.take() {
                let kind = self.active_view_kind;
                if !view.should_hide_when_task_is_done() {
                    self.active_view = Some(view);
                    self.active_view_kind = kind;
                } else {
                    self.clear_active_view_state();
                }
            }
        }
    }

    pub(crate) fn composer_is_empty(&self) -> bool {
        self.composer.is_empty()
    }

    pub(crate) fn composer_text(&self) -> String {
        self.composer.text().to_string()
    }

    pub(crate) fn is_task_running(&self) -> bool {
        self.is_task_running
    }

    /// Update the *context-window remaining* indicator in the composer. This
    /// is forwarded directly to the underlying `ChatComposer`.
    pub(crate) fn set_token_usage(
        &mut self,
        last_token_usage: TokenUsage,
        model_context_window: Option<u64>,
        context_mode: Option<ContextMode>,
    ) {
        self.composer
            .set_token_usage(last_token_usage, model_context_window, context_mode);
        self.request_redraw();
    }

    pub(crate) fn set_auto_context_phase(&mut self, phase: Option<AutoContextPhase>) {
        self.composer.set_auto_context_phase(phase);
        self.request_redraw();
    }

    /// Sends a redraw request to the app event channel.
    pub(crate) fn request_redraw(&self) {
        self.app_event_tx.send(AppEvent::RequestRedraw)
    }

    /// Request redraw and notify that the bottom pane view changed.
    /// This bypasses height manager hysteresis for immediate height recalculation.
    pub(super) fn request_redraw_with_height_change(&self) {
        self.app_event_tx.send(AppEvent::BottomPaneViewChanged);
    }

    pub(crate) fn update_model_selection_presets(&mut self, presets: Vec<ModelPreset>) {
        let Some(model_view) = self.active_view_as::<settings_pages::model::ModelSelectionView>()
        else {
            return;
        };

        model_view.update_presets(presets);
        self.request_redraw();
    }

    // Immediate redraw path removed; all UI updates flow through the
    // debounced RequestRedraw/App::Redraw scheduler to reduce thrash.

    pub(crate) fn flash_footer_notice(&mut self, text: String) {
        self.composer.flash_footer_notice(text);
        // Ask app to schedule a redraw shortly to clear the notice automatically
        self.app_event_tx.send(AppEvent::ScheduleFrameIn(
            ChatComposer::DEFAULT_FOOTER_NOTICE_DURATION + Duration::from_millis(100),
        ));
        self.request_redraw();
    }

    /// Convenience setters for individual hints
    pub(crate) fn set_reasoning_hint(&mut self, show: bool) {
        self.composer.set_show_reasoning_hint(show);
        self.request_redraw();
    }

    pub(crate) fn set_reasoning_state(&mut self, shown: bool) {
        self.composer.set_reasoning_state(shown);
        self.request_redraw();
    }

    pub(crate) fn set_diffs_hint(&mut self, show: bool) {
        self.composer.set_show_diffs_hint(show);
        self.request_redraw();
    }

    pub(crate) fn request_redraw_in(&self, dur: Duration) {
        self.app_event_tx.send(AppEvent::ScheduleFrameIn(dur));
    }

    // --- History helpers ---

    pub(crate) fn set_history_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.composer.set_history_metadata(log_id, entry_count);
    }

    pub(crate) fn set_input_focus(&mut self, has_focus: bool) {
        self.has_input_focus = has_focus;
        self.composer.set_has_focus(has_focus);
        self.composer.set_ctrl_c_quit_hint(self.ctrl_c_quit_hint);
    }

    pub(crate) fn on_history_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
    ) {
        let updated = self
            .composer
            .on_history_entry_response(log_id, offset, entry);

        if updated {
            self.request_redraw();
        }
    }

    pub(crate) fn on_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.composer.on_file_search_result(query, matches);
        self.request_redraw();
    }

    /// Ensure input focus is maintained, especially after redraws or content updates
    pub(crate) fn ensure_input_focus(&mut self) {
        // Only ensure focus if there's no active modal view
        if self.active_view.is_none() {
            if !self.has_input_focus {
                self.set_input_focus(true);
            } else {
                self.composer.set_ctrl_c_quit_hint(self.ctrl_c_quit_hint);
            }
        }
    }

    pub(crate) fn set_access_mode_label(&mut self, label: Option<String>) {
        self.composer.set_access_mode_label(label);
        // Hide the "(Shift+Tab change)" suffix after a short time for persistent modes.
        let dur = Duration::from_secs(4);
        self.composer.set_access_mode_hint_for(dur);
        self.schedule_redraw_after(dur, "access-hint");
        self.request_redraw();
    }

    pub(crate) fn set_access_mode_label_ephemeral(&mut self, label: String, dur: Duration) {
        self.composer.set_access_mode_label_ephemeral(label, dur);
        self.schedule_redraw_after(dur, "access-hint-ephemeral");
        self.request_redraw();
    }
}
