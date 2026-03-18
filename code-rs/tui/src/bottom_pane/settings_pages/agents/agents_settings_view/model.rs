use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::settings_ui::buttons::{
    standard_button_specs,
    SettingsButtonKind,
    StandardButtonSpec,
};
use crate::components::form_text_field::FormTextField;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Focus {
    Name,
    Mode,
    Agents,
    Instructions,
    Save,
    Delete,
    Cancel,
}

#[derive(Debug)]
pub struct SubagentEditorView {
    pub(super) name_field: FormTextField,
    pub(super) read_only: bool,
    pub(super) selected_agent_indices: Vec<usize>,
    pub(super) agent_cursor: usize,
    pub(super) orch_field: FormTextField,
    pub(super) available_agents: Vec<String>,
    pub(super) is_new: bool,
    pub(super) focus: Focus,
    pub(super) is_complete: bool,
    pub(super) app_event_tx: AppEventSender,
    pub(super) confirm_delete: bool,
}

impl SubagentEditorView {
    fn build_with(
        available_agents: Vec<String>,
        existing: Vec<code_core::config_types::SubagentCommandConfig>,
        app_event_tx: AppEventSender,
        name: &str,
    ) -> Self {
        let mut me = Self {
            name_field: FormTextField::new_single_line(),
            read_only: if name.is_empty() {
                false
            } else {
                code_core::slash_commands::default_read_only_for(name)
            },
            selected_agent_indices: Vec::new(),
            agent_cursor: 0,
            orch_field: FormTextField::new_multi_line(),
            available_agents,
            is_new: name.is_empty(),
            focus: Focus::Name,
            is_complete: false,
            app_event_tx,
            confirm_delete: false,
        };
        // Always seed the name field with the provided name
        if !name.is_empty() {
            me.name_field.set_text(name);
        }
        // Restrict ID field to [A-Za-z0-9_-]
        me.name_field
            .set_filter(crate::components::form_text_field::InputFilter::Id);
        // Seed from existing config if present
        if let Some(cfg) = existing.iter().find(|c| c.name.eq_ignore_ascii_case(name)) {
            me.name_field.set_text(&cfg.name);
            me.read_only = cfg.read_only;
            me.orch_field
                .set_text(&cfg.orchestrator_instructions.clone().unwrap_or_default());
            let set: std::collections::HashSet<String> = cfg.agents.iter().cloned().collect();
            for (idx, a) in me.available_agents.iter().enumerate() {
                if set.contains(a) {
                    me.selected_agent_indices.push(idx);
                }
            }
        } else {
            // No user config yet; provide sensible defaults from core for built-ins
            if !name.is_empty() {
                me.read_only = code_core::slash_commands::default_read_only_for(name);
                if let Some(instr) = code_core::slash_commands::default_instructions_for(name) {
                    me.orch_field.set_text(&instr);
                    // Start cursor at the top so the first lines are visible.
                    me.orch_field.move_cursor_to_start();
                }
            }
            // Default selection: when no explicit config exists, preselect all available agents.
            if me.selected_agent_indices.is_empty() {
                me.selected_agent_indices = (0..me.available_agents.len()).collect();
            }
        }
        me
    }

    pub fn new_with_data(
        name: String,
        available_agents: Vec<String>,
        existing: Vec<code_core::config_types::SubagentCommandConfig>,
        is_new: bool,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut s = Self::build_with(available_agents, existing, app_event_tx, &name);
        s.is_new = is_new;
        s
    }

    pub(super) fn toggle_agent_at(&mut self, idx: usize) {
        if let Some(pos) = self.selected_agent_indices.iter().position(|i| *i == idx) {
            self.selected_agent_indices.remove(pos);
        } else {
            self.selected_agent_indices.push(idx);
        }
    }

    pub(super) fn save(&mut self) {
        let agents: Vec<String> = if self.selected_agent_indices.is_empty() {
            Vec::new()
        } else {
            self.selected_agent_indices
                .iter()
                .filter_map(|i| self.available_agents.get(*i).cloned())
                .collect()
        };
        let cfg = code_core::config_types::SubagentCommandConfig {
            name: self.name_field.text().to_string(),
            read_only: self.read_only,
            agents,
            orchestrator_instructions: {
                let t = self.orch_field.text().trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            },
            agent_instructions: None,
        };
        // Persist to disk asynchronously to avoid blocking the TUI runtime
        if let Ok(home) = code_core::config::find_code_home() {
            let cfg_clone = cfg.clone();
            tokio::spawn(async move {
                let _ = code_core::config_edit::upsert_subagent_command(&home, &cfg_clone).await;
            });
        }
        // Update in-memory config
        self.app_event_tx.send(AppEvent::UpdateSubagentCommand(cfg));
    }

    fn show_delete(&self) -> bool {
        if self.is_new {
            return false;
        }
        let name = self.name_field.text();
        !name.trim().is_empty()
            && !["plan", "solve", "code"]
                .iter()
                .any(|reserved| name.eq_ignore_ascii_case(reserved))
    }

    fn focus_chain(&self) -> Vec<Focus> {
        let mut chain = vec![Focus::Name, Focus::Mode, Focus::Agents, Focus::Instructions];
        chain.extend(self.action_items().into_iter().map(|(id, _)| id));
        chain
    }

    pub(super) fn focus_prev(&mut self) {
        let chain = self.focus_chain();
        let Some(idx) = chain.iter().position(|f| *f == self.focus) else {
            self.focus = Focus::Name;
            return;
        };
        if idx > 0 {
            self.focus = chain[idx - 1];
        }
    }

    pub(super) fn focus_next(&mut self) {
        let chain = self.focus_chain();
        let Some(idx) = chain.iter().position(|f| *f == self.focus) else {
            self.focus = Focus::Name;
            return;
        };
        if idx + 1 < chain.len() {
            self.focus = chain[idx + 1];
        }
    }

    fn action_items(&self) -> Vec<(Focus, SettingsButtonKind)> {
        if self.confirm_delete {
            vec![
                (Focus::Delete, SettingsButtonKind::Delete),
                (Focus::Cancel, SettingsButtonKind::Cancel),
            ]
        } else if self.show_delete() {
            vec![
                (Focus::Save, SettingsButtonKind::Save),
                (Focus::Delete, SettingsButtonKind::Delete),
                (Focus::Cancel, SettingsButtonKind::Cancel),
            ]
        } else {
            vec![
                (Focus::Save, SettingsButtonKind::Save),
                (Focus::Cancel, SettingsButtonKind::Cancel),
            ]
        }
    }

    pub(super) fn action_button_specs(&self) -> Vec<StandardButtonSpec<Focus>> {
        let focused = matches!(self.focus, Focus::Save | Focus::Delete | Focus::Cancel)
            .then_some(self.focus);
        standard_button_specs(&self.action_items(), focused, None)
    }

    pub(super) fn move_action_left(&mut self) {
        let actions = self.action_items();
        let Some(idx) = actions.iter().position(|(id, _)| *id == self.focus) else {
            return;
        };
        if idx > 0 {
            self.focus = actions[idx - 1].0;
        }
    }

    pub(super) fn move_action_right(&mut self) {
        let actions = self.action_items();
        let Some(idx) = actions.iter().position(|(id, _)| *id == self.focus) else {
            return;
        };
        if idx + 1 < actions.len() {
            self.focus = actions[idx + 1].0;
        }
    }

    pub(super) fn enter_confirm_delete(&mut self) {
        self.confirm_delete = true;
        self.focus = Focus::Delete;
    }

    pub(super) fn exit_confirm_delete(&mut self) {
        self.confirm_delete = false;
        if self.show_delete() {
            self.focus = Focus::Delete;
        } else {
            self.focus = Focus::Save;
        }
    }

    pub(super) fn delete_current(&mut self) {
        let id = self.name_field.text().to_string();
        if id.trim().is_empty() {
            self.exit_confirm_delete();
            return;
        }

        if let Ok(home) = code_core::config::find_code_home() {
            let idc = id.clone();
            tokio::spawn(async move {
                let _ = code_core::config_edit::delete_subagent_command(&home, &idc).await;
            });
        }
        self.app_event_tx.send(AppEvent::DeleteSubagentCommand(id));
        self.is_complete = true;
        self.app_event_tx.send(AppEvent::ShowAgentsOverview);
    }
}

