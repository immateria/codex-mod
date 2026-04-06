use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;
use crate::components::mode_guard::ModeGuard;

impl ExecLimitsSettingsView {
    fn open_edit_for(&mut self, target: EditTarget) {
        let mut field = FormTextField::new_single_line();
        field.set_placeholder("number, auto, or disabled");
        match target {
            EditTarget::PidsMax => {
                if let code_core::config::ExecLimitToml::Value(v) = self.settings.pids_max {
                    field.set_text(&v.to_string());
                }
            }
            EditTarget::MemoryMax => {
                if let code_core::config::ExecLimitToml::Value(v) = self.settings.memory_max_mb {
                    field.set_text(&v.to_string());
                }
            }
        }
        self.mode = ViewMode::Edit {
            target,
            field,
            error: None,
        };
    }

    fn cycle_limit(&mut self, target: EditTarget) {
        let value = match target {
            EditTarget::PidsMax => self.settings.pids_max,
            EditTarget::MemoryMax => self.settings.memory_max_mb,
        };

        match value {
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto) => {
                self.set_limit(
                    target,
                    code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Disabled),
                );
            }
            code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Disabled) => {
                self.open_edit_for(target);
            }
            code_core::config::ExecLimitToml::Value(_) => {
                // When already custom, Enter edits instead of cycling away.
                self.open_edit_for(target);
            }
        }
    }

    fn set_limit(&mut self, target: EditTarget, value: code_core::config::ExecLimitToml) {
        match target {
            EditTarget::PidsMax => self.settings.pids_max = value,
            EditTarget::MemoryMax => self.settings.memory_max_mb = value,
        }
    }

    pub(super) fn activate_row(&mut self, row: RowKind) {
        match row {
            RowKind::PidsMax => self.cycle_limit(EditTarget::PidsMax),
            RowKind::MemoryMax => self.cycle_limit(EditTarget::MemoryMax),
            RowKind::ResetBothAuto => {
                self.settings.pids_max = code_core::config::ExecLimitToml::Mode(
                    code_core::config::ExecLimitModeToml::Auto,
                );
                self.settings.memory_max_mb = code_core::config::ExecLimitToml::Mode(
                    code_core::config::ExecLimitModeToml::Auto,
                );
            }
            RowKind::DisableBoth => {
                self.settings.pids_max = code_core::config::ExecLimitToml::Mode(
                    code_core::config::ExecLimitModeToml::Disabled,
                );
                self.settings.memory_max_mb = code_core::config::ExecLimitToml::Mode(
                    code_core::config::ExecLimitModeToml::Disabled,
                );
            }
            RowKind::Apply => {
                self.app_event_tx
                    .send(AppEvent::SetExecLimitsSettings(self.settings.clone()));
                self.last_applied = self.settings.clone();
                self.last_apply_at = Some(Instant::now());
            }
            RowKind::Close => self.is_complete = true,
        }
    }

    fn process_key_event_main(&mut self, key_event: KeyEvent) -> bool {
        let rows = Self::build_rows();
        let len = rows.len();
        let mut state = self.state.get();
        state.clamp_selection(len);
        let visible = self.viewport_rows.get().max(1);

        let handled = match key_event.code {
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            KeyCode::Up => {
                state.move_up_wrap_visible(len, visible);
                true
            }
            KeyCode::Down => {
                state.move_down_wrap_visible(len, visible);
                true
            }
            KeyCode::Enter => {
                let selected_idx = state.selected_idx.unwrap_or(0).min(len.saturating_sub(1));
                if let Some(selected) = rows.get(selected_idx).copied() {
                    self.activate_row(selected);
                    true
                } else {
                    false
                }
            }
            KeyCode::Char('a') if key_event.modifiers.is_empty() => {
                let selected_idx = state.selected_idx.unwrap_or(0).min(len.saturating_sub(1));
                let Some(&selected) = rows.get(selected_idx) else {
                    self.state.set(state);
                    return false;
                };
                let target = match selected {
                    RowKind::PidsMax => Some(EditTarget::PidsMax),
                    RowKind::MemoryMax => Some(EditTarget::MemoryMax),
                    _ => None,
                };
                let Some(target) = target else {
                    self.state.set(state);
                    return false;
                };
                self.set_limit(
                    target,
                    code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Auto),
                );
                true
            }
            KeyCode::Char('d') if key_event.modifiers.is_empty() => {
                let selected_idx = state.selected_idx.unwrap_or(0).min(len.saturating_sub(1));
                let Some(&selected) = rows.get(selected_idx) else {
                    self.state.set(state);
                    return false;
                };
                let target = match selected {
                    RowKind::PidsMax => Some(EditTarget::PidsMax),
                    RowKind::MemoryMax => Some(EditTarget::MemoryMax),
                    _ => None,
                };
                let Some(target) = target else {
                    self.state.set(state);
                    return false;
                };
                self.set_limit(
                    target,
                    code_core::config::ExecLimitToml::Mode(code_core::config::ExecLimitModeToml::Disabled),
                );
                true
            }
            _ => false,
        };

        self.state.set(state);
        handled
    }

    pub(super) fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::Transition, |mode| {
            matches!(mode, ViewMode::Transition)
        });

        match mode_guard.mode_mut() {
            ViewMode::Main => self.process_key_event_main(key_event),
            ViewMode::Edit {
                target,
                field,
                error,
            } => {
                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Esc, _) => {
                        self.mode = ViewMode::Main;
                        true
                    }
                    (KeyCode::Char('s'), KeyModifiers::CONTROL) | (KeyCode::Enter, _) => {
                        let text = field.text().trim();
                        if text.is_empty() {
                            *error = Some("Enter a number, or \"auto\"/\"disabled\"".to_string());
                            return true;
                        }

                        let lowered = text.to_ascii_lowercase();
                        if lowered == "auto" {
                            self.set_limit(
                                *target,
                                code_core::config::ExecLimitToml::Mode(
                                    code_core::config::ExecLimitModeToml::Auto,
                                ),
                            );
                            self.mode = ViewMode::Main;
                            return true;
                        }
                        if lowered == "disabled" || lowered == "disable" {
                            self.set_limit(
                                *target,
                                code_core::config::ExecLimitToml::Mode(
                                    code_core::config::ExecLimitModeToml::Disabled,
                                ),
                            );
                            self.mode = ViewMode::Main;
                            return true;
                        }

                        let parsed: u64 = match text.parse() {
                            Ok(v) if v >= 1 => v,
                            Ok(_) => {
                                *error = Some("Value must be >= 1 (or \"disabled\")".to_string());
                                return true;
                            }
                            Err(_) => {
                                *error = Some(
                                    "Value must be an integer (or \"auto\"/\"disabled\")".to_string(),
                                );
                                return true;
                            }
                        };

                        self.set_limit(*target, code_core::config::ExecLimitToml::Value(parsed));
                        self.mode = ViewMode::Main;
                        true
                    }
                    _ => {
                        field.handle_key(key_event)
                    }
                }
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.process_key_event(key_event)
    }
}
