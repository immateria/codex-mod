use super::*;

use code_core::config_types::{FunctionKeyHotkey, SettingsMenuOpenMode, TuiHotkey};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::components::mode_guard::ModeGuard;

impl InterfaceSettingsView {
    fn open_hotkey_capture(&mut self, row: RowKind) {
        self.mode = ViewMode::CaptureHotkey { row, error: None };
    }

    pub(super) fn activate_selected_row(&mut self) {
        let row = self.selected_row();
        if row.is_hotkey_row() {
            self.open_hotkey_capture(row);
            return;
        }

        match row {
            RowKind::OpenMode => self.cycle_open_mode_next(),
            RowKind::OverlayMinWidth => self.open_width_editor(),
            RowKind::HotkeyScope => self.cycle_hotkey_scope_next(),
            RowKind::ShowConfigToml => self.show_config_toml(),
            RowKind::ShowCodeHome => self.show_code_home(),
            RowKind::Apply => self.apply_settings(),
            RowKind::Close => self.is_complete = true,
            other => unreachable!("activate_selected_row missing handler for row: {other:?}"),
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

        self.state.clamp_selection(total);
        let selected = self.state.selected_idx.unwrap_or(0).min(total.saturating_sub(1));
        let current_row = rows.get(selected).copied();

        let visible = self.main_viewport_rows.get().max(1);

        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.status = None;
                self.state.move_up_wrap_visible(total, visible);
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.status = None;
                self.state.move_down_wrap_visible(total, visible);
                true
            }
            KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. } => {
                self.status = None;
                match current_row {
                    Some(RowKind::OpenMode) => {
                        // Reverse cycle.
                        self.settings.open_mode = match self.settings.open_mode {
                            SettingsMenuOpenMode::Auto => SettingsMenuOpenMode::Bottom,
                            SettingsMenuOpenMode::Overlay => SettingsMenuOpenMode::Auto,
                            SettingsMenuOpenMode::Bottom => SettingsMenuOpenMode::Overlay,
                        };
                        self.dirty_settings = true;
                    }
                    Some(RowKind::OverlayMinWidth) => self.adjust_min_width(-5),
                    Some(RowKind::HotkeyScope) => self.cycle_hotkey_scope_prev(),
                    Some(row) if row.is_hotkey_row() => {
                        self.adjust_hotkey_for_row(row, false);
                    }
                    _ => {}
                }
                true
            }
            KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                self.status = None;
                match current_row {
                    Some(RowKind::OpenMode) => self.cycle_open_mode_next(),
                    Some(RowKind::OverlayMinWidth) => self.adjust_min_width(5),
                    Some(RowKind::HotkeyScope) => self.cycle_hotkey_scope_next(),
                    Some(row) if row.is_hotkey_row() => {
                        self.adjust_hotkey_for_row(row, true);
                    }
                    _ => {}
                }
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
            | KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                if current_row.is_some() {
                    self.activate_selected_row();
                    self.state.ensure_visible(total, visible);
                    true
                } else {
                    false
                }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    fn process_key_event_capture_hotkey(&mut self, row: RowKind, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent { code: KeyCode::Char('d'), modifiers, .. }
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                self.set_hotkey_for_row(row, TuiHotkey::disabled());
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent { code: KeyCode::Char('l'), modifiers, .. }
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                if row.supports_legacy_hotkey() {
                    self.set_hotkey_for_row(row, TuiHotkey::legacy());
                    self.mode = ViewMode::Main;
                } else {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("Legacy is only available for history shortcuts.".to_owned()),
                    };
                }
                true
            }
            KeyEvent { code: KeyCode::Char('i'), modifiers, .. }
                if (modifiers.is_empty() || modifiers == KeyModifiers::SHIFT)
                    && !matches!(self.hotkey_scope, HotkeyScope::Global) =>
            {
                self.clear_hotkey_override_for_row(row);
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent {
                code: KeyCode::F(n),
                modifiers,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                if n == 1 {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("F1 is reserved for the Help overlay.".to_owned()),
                    };
                    return true;
                }
                let max_key = self.hotkey_scope.max_function_key();
                if n > max_key {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some(format!("This scope supports up to F{max_key}.")),
                    };
                    return true;
                }
                let Some(fk) = FunctionKeyHotkey::from_u8(n) else {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("Unsupported function key.".to_owned()),
                    };
                    return true;
                };
                let hk = TuiHotkey::Function(fk);
                self.set_hotkey_for_row(row, hk);
                self.mode = ViewMode::Main;
                true
            }
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                let mods = modifiers.difference(KeyModifiers::SHIFT);
                if mods.intersects(KeyModifiers::SUPER) {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("Super modifier is not supported for hotkeys.".to_owned()),
                    };
                    return true;
                }
                let ctrl = mods.contains(KeyModifiers::CONTROL);
                let alt = mods.contains(KeyModifiers::ALT);
                if !ctrl && !alt {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("Use Ctrl/Alt+letter or a function key.".to_owned()),
                    };
                    return true;
                }
                if !c.is_ascii_alphabetic() {
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some("Hotkey chords currently support ASCII letters only.".to_owned()),
                    };
                    return true;
                }

                let hk = TuiHotkey::Chord(code_core::config_types::TuiHotkeyChord {
                    ctrl,
                    alt,
                    key: c.to_ascii_lowercase(),
                });
                if hk.is_reserved_for_statusline_shortcuts() {
                    let label = hk.display_name();
                    self.mode = ViewMode::CaptureHotkey {
                        row,
                        error: Some(format!(
                            "{label} is reserved and cannot be remapped.",
                            label = label.as_ref()
                        )),
                    };
                    return true;
                }
                self.set_hotkey_for_row(row, hk);
                self.mode = ViewMode::Main;
                true
            }
            _ => {
                self.mode = ViewMode::CaptureHotkey { row, error: None };
                true
            }
        }
    }

    fn process_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mut mode_guard = ModeGuard::replace(&mut self.mode, ViewMode::Transition, |mode| {
            matches!(mode, ViewMode::Transition)
        });

        match mode_guard.mode_mut() {
            ViewMode::Main => self.process_key_event_main(key_event),
            ViewMode::EditWidth { field, error } => {
                let handled = match key_event {
                    KeyEvent { code: KeyCode::Esc, .. } => {
                        self.mode = ViewMode::Main;
                        true
                    }
                    KeyEvent {
                        code: KeyCode::Enter,
                        modifiers: KeyModifiers::NONE,
                        ..
                    } => match self.save_width_editor(field) {
                        Ok(()) => {
                            self.mode = ViewMode::Main;
                            true
                        }
                        Err(err) => {
                            *error = Some(err);
                            true
                        }
                    },
                    KeyEvent {
                        code: KeyCode::Char('s'),
                        modifiers,
                        ..
                    } if modifiers.contains(KeyModifiers::CONTROL) => match self.save_width_editor(field)
                    {
                        Ok(()) => {
                            self.mode = ViewMode::Main;
                            true
                        }
                        Err(err) => {
                            *error = Some(err);
                            true
                        }
                    },
                    _ => field.handle_key(key_event),
                };
                handled
            }
            ViewMode::CaptureHotkey { row, .. } => {
                let row = *row;
                self.process_key_event_capture_hotkey(row, key_event)
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
