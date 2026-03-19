use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::components::mode_guard::ModeGuard;

impl JsReplSettingsView {
    fn visible_budget(&self, total: usize) -> usize {
        if total == 0 {
            return 0;
        }
        let raw = self.viewport_rows.get();
        let effective = if raw == 0 {
            Self::DEFAULT_VISIBLE_ROWS
        } else {
            raw
        };
        effective.max(1).min(total)
    }

    pub(super) fn reconcile_selection_state(&mut self, total: usize) {
        if total == 0 {
            self.state.selected_idx = None;
            self.state.scroll_top = 0;
            return;
        }
        self.state.clamp_selection(total);
        let visible_budget = self.visible_budget(total);
        self.state.ensure_visible(total, visible_budget);
    }

    pub(super) fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::Transition, |mode| {
            matches!(mode, ViewMode::Transition)
        });
        match mode_guard.mode_mut() {
            ViewMode::Main => {
                let rows = self.build_rows();
                let total = rows.len();
                if total == 0 {
                    if matches!(key_event.code, KeyCode::Esc) {
                        self.is_complete = true;
                        return true;
                    }
                    return false;
                }

                self.reconcile_selection_state(total);
                let selected = self
                    .state
                    .selected_idx
                    .unwrap_or(0);

                let handled = match key_event.code {
                    KeyCode::Esc => {
                        self.is_complete = true;
                        true
                    }
                    KeyCode::Enter => {
                        if let Some(kind) = rows.get(selected).copied() {
                            self.activate_row(kind);
                            true
                        } else {
                            false
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.state.move_up_wrap_visible(total, self.visible_budget(total));
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.state.move_down_wrap_visible(total, self.visible_budget(total));
                        true
                    }
                    KeyCode::Home => {
                        self.state.selected_idx = Some(0);
                        self.state.scroll_top = 0;
                        true
                    }
                    KeyCode::End => {
                        if total > 0 {
                            self.state.selected_idx = Some(total - 1);
                            self.state.ensure_visible(total, self.visible_budget(total));
                        }
                        true
                    }
                    _ => false,
                };

                if matches!(self.mode, ViewMode::Transition) {
                    // Activation can add/remove optional rows; keep selection + scroll valid.
                    self.reconcile_selection_state(self.row_count());
                }
                handled
            }
            ViewMode::EditText { target, field } => match key_event {
                KeyEvent {
                    code: KeyCode::Char('s'),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::CONTROL) => {
                    match self.save_text_editor(*target, field) {
                        Ok(()) => {
                            self.mode = ViewMode::Main;
                            true
                        }
                        Err(err) => {
                            self.app_event_tx.send_background_event_with_ticket(
                                &self.ticket,
                                format!("JS REPL: {err}"),
                            );
                            true
                        }
                    }
                }
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.mode = ViewMode::Main;
                    true
                }
                _ => {
                    field.handle_key(key_event)
                }
            },
            ViewMode::EditList { target, field } => match key_event {
                KeyEvent {
                    code: KeyCode::Char('s'),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::CONTROL) => {
                    match self.save_list_editor(*target, field) {
                        Ok(()) => {
                            self.mode = ViewMode::Main;
                            true
                        }
                        Err(err) => {
                            self.app_event_tx.send_background_event_with_ticket(
                                &self.ticket,
                                format!("JS REPL: {err}"),
                            );
                            true
                        }
                    }
                }
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.mode = ViewMode::Main;
                    true
                }
                _ => {
                    field.handle_key(key_event)
                }
            },
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        match &mut self.mode {
            ViewMode::EditText { field, .. } | ViewMode::EditList { field, .. } => {
                field.handle_paste(text);
                true
            }
            ViewMode::Main | ViewMode::Transition => false,
        }
    }
}
