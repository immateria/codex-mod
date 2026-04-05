use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;
use crate::components::mode_guard::ModeGuard;
use crate::native_picker::{pick_path, NativePickerKind};

impl ShellEscalationSettingsView {
    fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
        self.recompute_dirty();
    }

    fn normalize_opt_string(value: Option<String>) -> Option<String> {
        let trimmed = value.as_deref().unwrap_or_default().trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    }

    fn normalize_text_field(text: &str) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
    }

    fn open_text_editor(&mut self, target: EditTarget) {
        self.editor_notice = None;

        let mut field = FormTextField::new_single_line();
        match target {
            EditTarget::ZshPath => field.set_placeholder("/abs/path/to/patched/zsh"),
            EditTarget::WrapperOverride => field.set_placeholder("/abs/path/to/codex-execve-wrapper"),
        }

        let before = match target {
            EditTarget::ZshPath => self.zsh_path.clone().unwrap_or_default(),
            EditTarget::WrapperOverride => self.wrapper_override.clone().unwrap_or_default(),
        };
        field.set_text(&before);

        self.mode = ViewMode::EditText {
            target,
            field,
        };
    }

    fn save_text_editor(&mut self, target: EditTarget, field: &FormTextField) {
        let value = Self::normalize_text_field(field.text());
        match target {
            EditTarget::ZshPath => self.zsh_path = value,
            EditTarget::WrapperOverride => self.wrapper_override = value,
        }
        self.recompute_dirty();
    }

    fn request_save(&mut self) {
        let zsh_path = Self::normalize_opt_string(self.zsh_path.clone());
        let wrapper_override = Self::normalize_opt_string(self.wrapper_override.clone());

        self.app_event_tx.send(AppEvent::UpdateShellEscalationSettings {
            enabled: self.enabled,
            zsh_path: zsh_path.clone(),
            wrapper: wrapper_override.clone(),
        });

        self.baseline_enabled = self.enabled;
        self.baseline_zsh_path = zsh_path;
        self.baseline_wrapper_override = wrapper_override;
        self.recompute_dirty();
    }

    pub(super) fn activate_row(&mut self, row: RowKind) {
        match row {
            RowKind::Enabled => self.toggle_enabled(),
            RowKind::ZshPath => self.open_text_editor(EditTarget::ZshPath),
            RowKind::WrapperOverride => self.open_text_editor(EditTarget::WrapperOverride),
            RowKind::Apply => self.request_save(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn process_key_event_main(&mut self, key_event: KeyEvent) -> bool {
        let rows = self.build_rows();
        let total = rows.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
                return true;
            }
            return false;
        }

        self.reconcile_selection_state();
        let selected = self.state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
        let current_row = rows.get(selected).copied();

        let handled = match key_event {
            KeyEvent {
                code: KeyCode::Esc,
                ..
            } => {
                self.is_complete = true;
                true
            }
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.move_up_wrap_visible(total, self.visible_budget(total));
                true
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.move_down_wrap_visible(total, self.visible_budget(total));
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Some(kind) = current_row {
                    self.activate_row(kind);
                    true
                } else {
                    false
                }
            }
            KeyEvent {
                code: KeyCode::Char('s'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.request_save();
                true
            }
            _ => false,
        };

        self.reconcile_selection_state();
        handled
    }

    fn process_key_event_edit(&mut self, key_event: KeyEvent, target: EditTarget, field: &mut FormTextField) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.mode = ViewMode::Main;
                self.editor_notice = None;
                true
            }
            KeyEvent { code: KeyCode::Enter, .. } => {
                self.save_text_editor(target, field);
                self.mode = ViewMode::Main;
                self.editor_notice = None;
                true
            }
            KeyEvent {
                code: KeyCode::Char('s'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_text_editor(target, field);
                self.mode = ViewMode::Main;
                self.editor_notice = None;
                self.request_save();
                true
            }
            KeyEvent { code: KeyCode::Char('p'), modifiers: KeyModifiers::NONE, .. } => {
                self.editor_notice = None;
                if !crate::platform_caps::supports_native_picker() {
                    self.editor_notice = Some("Not supported on Android; type the path.".to_string());
                    return true;
                }
                let title = match target {
                    EditTarget::ZshPath => "Select patched zsh binary",
                    EditTarget::WrapperOverride => "Select execve wrapper binary",
                };
                match pick_path(NativePickerKind::File, title) {
                    Ok(Some(path)) => {
                        field.set_text(&path.to_string_lossy());
                        true
                    }
                    Ok(None) => true,
                    Err(err) => {
                        self.editor_notice = Some(format!("Picker failed: {err:#}"));
                        true
                    }
                }
            }
            _ => field.handle_key(key_event),
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        if self.is_complete {
            return true;
        }

        // Only reserve Ctrl+S; allow other control chords to bubble so global
        // bindings (and text fields) stay predictable.
        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && !matches!(key_event.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            return false;
        }

        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::Transition, |mode| {
            matches!(mode, ViewMode::Transition)
        });
        match mode_guard.mode_mut() {
            ViewMode::Main => self.process_key_event_main(key_event),
            ViewMode::EditText { target, field } => {
                self.process_key_event_edit(key_event, *target, field)
            }
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }

    pub(crate) fn handle_paste_direct(&mut self, text: String) -> bool {
        if self.is_complete {
            return false;
        }

        let ViewMode::EditText { field, .. } = &mut self.mode else {
            return false;
        };
        if text.is_empty() {
            return false;
        }
        field.handle_paste(text);
        true
    }
}
