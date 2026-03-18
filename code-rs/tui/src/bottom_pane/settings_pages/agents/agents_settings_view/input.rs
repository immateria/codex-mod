use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;

use super::model::{Focus, SubagentEditorView};

impl SubagentEditorView {
    pub(super) fn handle_key_event_internal(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent {
                code: KeyCode::Esc,
                ..
            } => {
                self.is_complete = true;
                self.confirm_delete = false;
                self.app_event_tx.send(AppEvent::ShowAgentsOverview);
                true
            }
            KeyEvent {
                code: KeyCode::Tab,
                ..
            } => {
                self.focus_next();
                true
            }
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => {
                self.focus_prev();
                true
            }
            KeyEvent { code: KeyCode::Up, modifiers, .. } => {
                if self.focus == Focus::Instructions {
                    let at_start = self.orch_field.cursor_is_at_start();
                    let _ = self.orch_field.handle_key(KeyEvent {
                        code: KeyCode::Up,
                        modifiers,
                        ..key_event
                    });
                    if at_start {
                        self.focus_prev();
                    }
                } else {
                    self.focus_prev();
                }
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers, .. } => {
                if self.focus == Focus::Instructions {
                    let at_end = self.orch_field.cursor_is_at_end();
                    let _ = self.orch_field.handle_key(KeyEvent {
                        code: KeyCode::Down,
                        modifiers,
                        ..key_event
                    });
                    if at_end {
                        self.focus_next();
                    }
                } else {
                    self.focus_next();
                }
                true
            }
            KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. }
                if matches!(self.focus, Focus::Save | Focus::Delete | Focus::Cancel) =>
            {
                self.move_action_left();
                true
            }
            KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. }
                if matches!(self.focus, Focus::Save | Focus::Delete | Focus::Cancel) =>
            {
                self.move_action_right();
                true
            }
            KeyEvent {
                code: KeyCode::Left | KeyCode::Right | KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == Focus::Mode => {
                self.read_only = !self.read_only;
                true
            }
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == Focus::Agents => {
                if self.agent_cursor > 0 {
                    self.agent_cursor -= 1;
                }
                true
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == Focus::Agents => {
                if self.agent_cursor + 1 < self.available_agents.len() {
                    self.agent_cursor += 1;
                }
                true
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == Focus::Agents => {
                let idx = self
                    .agent_cursor
                    .min(self.available_agents.len().saturating_sub(1));
                self.toggle_agent_at(idx);
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == Focus::Agents => {
                let idx = self
                    .agent_cursor
                    .min(self.available_agents.len().saturating_sub(1));
                self.toggle_agent_at(idx);
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == Focus::Save && !self.confirm_delete => {
                self.save();
                self.is_complete = true;
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == Focus::Delete && !self.confirm_delete => {
                self.enter_confirm_delete();
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == Focus::Delete && self.confirm_delete => {
                self.delete_current();
                true
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } if self.focus == Focus::Cancel => {
                if self.confirm_delete {
                    self.exit_confirm_delete();
                } else {
                    self.is_complete = true;
                    self.app_event_tx.send(AppEvent::ShowAgentsOverview);
                }
                true
            }
            KeyEvent { code: KeyCode::Char('s'), modifiers, .. }
                if modifiers.contains(KeyModifiers::CONTROL) && !self.confirm_delete =>
            {
                self.save();
                self.is_complete = true;
                true
            }
            ev @ KeyEvent { .. } if self.focus == Focus::Name => {
                let _ = self.name_field.handle_key(ev);
                true
            }
            ev @ KeyEvent { .. } if self.focus == Focus::Instructions => {
                let _ = self.orch_field.handle_key(ev);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key_event_internal(key_event)
    }
}

