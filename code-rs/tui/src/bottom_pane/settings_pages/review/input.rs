use super::*;

use code_core::config_types::AutoResolveAttemptLimit;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;
use crate::bottom_pane::settings_ui::line_runs::scroll_top_for_section;

impl ReviewSettingsView {
    fn toggle_review_auto_resolve(&mut self) {
        self.review_auto_resolve_enabled = !self.review_auto_resolve_enabled;
        self.app_event_tx
            .send(AppEvent::UpdateReviewAutoResolveEnabled(self.review_auto_resolve_enabled));
    }

    fn adjust_review_followups(&mut self, forward: bool) {
        let allowed = AutoResolveAttemptLimit::ALLOWED;
        if allowed.is_empty() {
            return;
        }

        let len = allowed.len();
        let mut next = self.review_followups_index;
        next = if forward {
            (next + 1) % len
        } else if next == 0 {
            len.saturating_sub(1)
        } else {
            next - 1
        };

        if next == self.review_followups_index {
            return;
        }

        self.review_followups_index = next;
        self.review_followups = allowed[next];
        self.app_event_tx
            .send(AppEvent::UpdateReviewAutoResolveAttempts(self.review_followups));
    }

    fn adjust_auto_review_followups(&mut self, forward: bool) {
        let allowed = AutoResolveAttemptLimit::ALLOWED;
        if allowed.is_empty() {
            return;
        }

        let len = allowed.len();
        let mut next = self.auto_review_followups_index;
        next = if forward {
            (next + 1) % len
        } else if next == 0 {
            len.saturating_sub(1)
        } else {
            next - 1
        };

        if next == self.auto_review_followups_index {
            return;
        }

        self.auto_review_followups_index = next;
        self.auto_review_followups = allowed[next];
        self.app_event_tx
            .send(AppEvent::UpdateAutoReviewFollowupAttempts(self.auto_review_followups));
    }

    fn toggle_auto_review(&mut self) {
        self.auto_review_enabled = !self.auto_review_enabled;
        self.app_event_tx
            .send(AppEvent::UpdateAutoReviewEnabled(self.auto_review_enabled));
    }

    fn open_review_model_selector(&self) {
        self.app_event_tx.send(AppEvent::ShowReviewModelSelector);
    }

    fn open_review_resolve_model_selector(&self) {
        self.app_event_tx
            .send(AppEvent::ShowReviewResolveModelSelector);
    }

    fn open_auto_review_model_selector(&self) {
        self.app_event_tx
            .send(AppEvent::ShowAutoReviewModelSelector);
    }

    fn open_auto_review_resolve_model_selector(&self) {
        self.app_event_tx
            .send(AppEvent::ShowAutoReviewResolveModelSelector);
    }

    pub(super) fn activate_selection_kind(&mut self, kind: SelectionKind) {
        match kind {
            SelectionKind::ReviewEnabled => self.toggle_review_auto_resolve(),
            SelectionKind::ReviewAttempts => self.adjust_review_followups(true),
            SelectionKind::ReviewModel => self.open_review_model_selector(),
            SelectionKind::ReviewResolveModel => self.open_review_resolve_model_selector(),
            SelectionKind::AutoReviewEnabled => self.toggle_auto_review(),
            SelectionKind::AutoReviewAttempts => self.adjust_auto_review_followups(true),
            SelectionKind::AutoReviewModel => self.open_auto_review_model_selector(),
            SelectionKind::AutoReviewResolveModel => self.open_auto_review_resolve_model_selector(),
        }
    }

    pub(super) fn ensure_selected_visible(&mut self, model: &ReviewListModel, body_height: usize) {
        let total = model.selection_kinds.len();
        if total == 0 {
            self.state.reset();
            return;
        }

        let sel_idx = self
            .state
            .selected_idx
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let selected_line = model.selection_line.get(sel_idx).copied().unwrap_or(0);
        let selected_line = selected_line.min(model.total_lines.saturating_sub(1));
        let (section_start, section_end) = model
            .section_bounds
            .get(sel_idx)
            .copied()
            .unwrap_or((0, model.total_lines.saturating_sub(1)));
        let section_end = section_end.min(model.total_lines.saturating_sub(1));
        let section_start = section_start.min(section_end);

        self.state.scroll_top = scroll_top_for_section(
            self.state.scroll_top,
            body_height,
            selected_line,
            section_start,
            section_end,
        );
    }

    pub(super) fn handle_key_event_impl(&mut self, key_event: KeyEvent) -> bool {
        let model = self.build_model();
        let total = model.selection_kinds.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
                return true;
            }
            return false;
        }
        self.state.clamp_selection(total);
        let body_height_hint = match self.viewport_rows.get() {
            0 => DEFAULT_VISIBLE_ROWS,
            other => other,
        };
        self.ensure_selected_visible(&model, body_height_hint);
        let current_kind = self
            .state
            .selected_idx
            .and_then(|sel| model.selection_kinds.get(sel))
            .copied();
        let handled = match key_event {
            KeyEvent { code: KeyCode::Up, .. } => {
                self.state.move_up_wrap(total);
                true
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                self.state.move_down_wrap(total);
                true
            }
            KeyEvent { code: KeyCode::Left, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::ReviewEnabled => self.toggle_review_auto_resolve(),
                        SelectionKind::ReviewAttempts => self.adjust_review_followups(false),
                        SelectionKind::AutoReviewEnabled => self.toggle_auto_review(),
                        SelectionKind::AutoReviewAttempts => self.adjust_auto_review_followups(false),
                        SelectionKind::ReviewModel
                        | SelectionKind::ReviewResolveModel
                        | SelectionKind::AutoReviewModel
                        | SelectionKind::AutoReviewResolveModel => {}
                    }
                }
                true
            }
            KeyEvent { code: KeyCode::Right, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::ReviewEnabled => self.toggle_review_auto_resolve(),
                        SelectionKind::ReviewAttempts => self.adjust_review_followups(true),
                        SelectionKind::AutoReviewEnabled => self.toggle_auto_review(),
                        SelectionKind::AutoReviewAttempts => self.adjust_auto_review_followups(true),
                        SelectionKind::ReviewModel
                        | SelectionKind::ReviewResolveModel
                        | SelectionKind::AutoReviewModel
                        | SelectionKind::AutoReviewResolveModel => {}
                    }
                }
                true
            }
            KeyEvent { code: KeyCode::Char(' '), .. }
            | KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                if let Some(kind) = current_kind {
                    self.activate_selection_kind(kind);
                }
                true
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        };

        self.state.clamp_selection(total);
        self.ensure_selected_visible(&model, body_height_hint);
        handled
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key_event_impl(key_event)
    }
}

