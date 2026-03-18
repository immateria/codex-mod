use crossterm::event::{KeyCode, KeyEvent};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::model::{AgentsOverviewState, AgentsSettingsContent};

impl AgentsSettingsContent {
    pub(super) fn handle_overview_key(
        state: &mut AgentsOverviewState,
        key: KeyEvent,
        app_event_tx: &AppEventSender,
    ) -> bool {
        match key.code {
            KeyCode::Up => {
                if state.total_rows() == 0 {
                    return true;
                }
                if state.selected == 0 {
                    state.selected = state.total_rows().saturating_sub(1);
                } else {
                    state.selected -= 1;
                }
                app_event_tx.send(AppEvent::AgentsOverviewSelectionChanged {
                    index: state.selected,
                });
                true
            }
            KeyCode::Down => {
                let total = state.total_rows();
                if total == 0 {
                    return true;
                }
                state.selected = (state.selected + 1) % total;
                app_event_tx.send(AppEvent::AgentsOverviewSelectionChanged {
                    index: state.selected,
                });
                true
            }
            KeyCode::Enter => {
                let idx = state.selected;
                let add_agent_idx = state.rows.len();
                if idx == add_agent_idx {
                    app_event_tx.send(AppEvent::ShowAgentEditorNew);
                } else if idx < add_agent_idx {
                    let row = &state.rows[idx];
                    if !row.installed {
                        app_event_tx.send(AppEvent::RequestAgentInstall {
                            name: row.name.clone(),
                            selected_index: idx,
                        });
                    } else {
                        app_event_tx.send(AppEvent::ShowAgentEditor {
                            name: row.name.clone(),
                        });
                    }
                } else {
                    let cmd_idx = idx.saturating_sub(state.rows.len() + 1);
                    if cmd_idx < state.commands.len() {
                        if let Some(name) = state.commands.get(cmd_idx) {
                            app_event_tx.send(AppEvent::ShowSubagentEditorForName {
                                name: name.clone(),
                            });
                        }
                    } else {
                        app_event_tx.send(AppEvent::ShowSubagentEditorNew);
                    }
                }
                true
            }
            _ => false,
        }
    }
}

