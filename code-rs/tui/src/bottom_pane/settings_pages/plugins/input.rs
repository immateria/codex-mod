use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl PluginsSettingsView {
    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        if matches!(key_event.modifiers, KeyModifiers::CONTROL) {
            return false;
        }

        match self.mode.clone() {
            Mode::List => self.handle_key_list(key_event),
            Mode::Detail { key } => self.handle_key_detail(key_event, key),
            Mode::ConfirmUninstall { plugin_id_key, key } => {
                self.handle_key_confirm_uninstall(key_event, plugin_id_key, key)
            }
        }
    }

    fn handle_key_list(&mut self, key_event: KeyEvent) -> bool {
        let snapshot = self.shared_snapshot();
        let plugin_rows = Self::plugin_rows_from_snapshot(&snapshot);
        let plugin_count = plugin_rows.len();

        match key_event.code {
            KeyCode::Esc => {
                self.is_complete = true;
                true
            }
            KeyCode::Up => {
                let mut state = self.list_state.get();
                state.move_up_wrap_visible(plugin_count, self.list_viewport_rows.get().max(1));
                self.list_state.set(state);
                true
            }
            KeyCode::Down => {
                let mut state = self.list_state.get();
                state.move_down_wrap_visible(plugin_count, self.list_viewport_rows.get().max(1));
                self.list_state.set(state);
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let Some(selected) = self.selected_plugin_row(&plugin_rows) else {
                    return false;
                };
                let key = PluginDetailKey::new(selected.marketplace_path, selected.plugin_name);
                self.mode = Mode::Detail { key: key.clone() };
                self.focused_detail_button = DetailAction::Back;
                self.hovered_detail_button = None;
                self.request_plugin_detail(PluginReadRequest {
                    plugin_name: key.plugin_name.clone(),
                    marketplace_path: key.marketplace_path.clone(),
                });
                true
            }
            KeyCode::Char('r') => {
                self.request_plugin_list(/*force_remote_sync*/ false);
                true
            }
            KeyCode::Char('R') => {
                self.request_plugin_list(/*force_remote_sync*/ true);
                true
            }
            _ => false,
        }
    }

    fn handle_key_detail(&mut self, key_event: KeyEvent, key: PluginDetailKey) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.mode = Mode::List;
                true
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
                self.cycle_detail_focus(key, key_event.code);
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_detail_action(key),
            _ => false,
        }
    }

    fn handle_key_confirm_uninstall(
        &mut self,
        key_event: KeyEvent,
        plugin_id_key: String,
        key: PluginDetailKey,
    ) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.mode = Mode::Detail { key };
                true
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
                self.focused_confirm_button = match self.focused_confirm_button {
                    ConfirmAction::Uninstall => ConfirmAction::Cancel,
                    ConfirmAction::Cancel => ConfirmAction::Uninstall,
                };
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.focused_confirm_button {
                ConfirmAction::Cancel => {
                    self.mode = Mode::Detail { key };
                    true
                }
                ConfirmAction::Uninstall => {
                    self.mode = Mode::List;
                    self.request_uninstall_plugin(plugin_id_key, /*force_remote_sync*/ false);
                    true
                }
            },
            _ => false,
        }
    }

    fn cycle_detail_focus(&mut self, key: PluginDetailKey, direction: KeyCode) {
        let snapshot = self.shared_snapshot();
        let Some(detail_state) = snapshot.details.get(&key).cloned() else {
            self.focused_detail_button = DetailAction::Back;
            return;
        };

        let (installed, enabled) = match detail_state {
            crate::chatwidget::PluginsDetailState::Ready(outcome) => {
                (outcome.plugin.installed, outcome.plugin.enabled)
            }
            _ => (false, false),
        };

        let buttons = self.detail_button_specs(installed, enabled);
        let available = buttons.iter().map(|b| b.id).collect::<Vec<_>>();
        if available.is_empty() {
            self.focused_detail_button = DetailAction::Back;
            return;
        }

        if !available.contains(&self.focused_detail_button) {
            self.focused_detail_button = available[0];
            return;
        }

        let idx = available
            .iter()
            .position(|id| *id == self.focused_detail_button)
            .unwrap_or(0);
        let next = match direction {
            KeyCode::Left => idx.checked_sub(1).unwrap_or(available.len().saturating_sub(1)),
            _ => (idx + 1) % available.len(),
        };
        self.focused_detail_button = available[next];
    }

    pub(super) fn activate_detail_action(&mut self, key: PluginDetailKey) -> bool {
        let snapshot = self.shared_snapshot();
        let Some(detail_state) = snapshot.details.get(&key).cloned() else {
            return false;
        };
        let crate::chatwidget::PluginsDetailState::Ready(outcome) = detail_state else {
            if matches!(self.focused_detail_button, DetailAction::Back) {
                self.mode = Mode::List;
                return true;
            }
            return false;
        };

        match self.focused_detail_button {
            DetailAction::Back => {
                self.mode = Mode::List;
                true
            }
            DetailAction::Install => {
                self.request_install_plugin(
                    PluginInstallRequest {
                        plugin_name: key.plugin_name.clone(),
                        marketplace_path: key.marketplace_path.clone(),
                    },
                    /*force_remote_sync*/ false,
                );
                true
            }
            DetailAction::Uninstall => {
                self.mode = Mode::ConfirmUninstall {
                    plugin_id_key: outcome.plugin.id.clone(),
                    key,
                };
                self.focused_confirm_button = ConfirmAction::Cancel;
                self.hovered_confirm_button = None;
                true
            }
            DetailAction::Enable => {
                self.request_set_plugin_enabled(outcome.plugin.id.clone(), true);
                true
            }
            DetailAction::Disable => {
                self.request_set_plugin_enabled(outcome.plugin.id.clone(), false);
                true
            }
        }
    }
}
