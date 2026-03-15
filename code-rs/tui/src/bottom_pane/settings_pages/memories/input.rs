use super::*;

use crate::app_event::{AppEvent, MemoriesArtifactsAction};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl MemoriesSettingsView {
    fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = Some((text.into(), is_error));
    }

    fn clear_status(&mut self) {
        self.status = None;
    }

    fn cycle_scope(&mut self, forward: bool) {
        let ordered = [
            MemoriesScopeChoice::Global,
            MemoriesScopeChoice::Profile,
            MemoriesScopeChoice::Project,
        ];
        let current_idx = ordered
            .iter()
            .position(|scope| *scope == self.scope)
            .unwrap_or(0);
        for step in 1..=ordered.len() {
            let idx = if forward {
                (current_idx + step) % ordered.len()
            } else {
                (current_idx + ordered.len() - step) % ordered.len()
            };
            let candidate = ordered[idx];
            if self.supports_scope(candidate) {
                self.scope = candidate;
                self.clear_status();
                return;
            }
        }
        self.set_status("Active profile scope is unavailable.", true);
    }

    fn cycle_scope_from_row(&mut self, forward: bool) {
        if self.active_profile.is_none() {
            self.set_status("Active profile scope is unavailable.", true);
        }
        self.cycle_scope(forward);
    }

    fn toggle_bool_row(&mut self, row: RowKind) {
        let effective = self.effective_settings();
        match self.scope {
            MemoriesScopeChoice::Global => {
                let settings = &mut self.global_settings;
                match row {
                    RowKind::GenerateMemories => {
                        settings.generate_memories = Some(!effective.generate_memories)
                    }
                    RowKind::UseMemories => settings.use_memories = Some(!effective.use_memories),
                    RowKind::SkipMcpOrWebSearch => {
                        settings.no_memories_if_mcp_or_web_search =
                            Some(!effective.no_memories_if_mcp_or_web_search)
                    }
                    _ => {}
                }
            }
            MemoriesScopeChoice::Profile | MemoriesScopeChoice::Project => {
                let inherited = match row {
                    RowKind::GenerateMemories => effective.generate_memories,
                    RowKind::UseMemories => effective.use_memories,
                    RowKind::SkipMcpOrWebSearch => effective.no_memories_if_mcp_or_web_search,
                    _ => false,
                };
                let settings = self.ensure_current_scope_settings_mut();
                let target = match row {
                    RowKind::GenerateMemories => &mut settings.generate_memories,
                    RowKind::UseMemories => &mut settings.use_memories,
                    RowKind::SkipMcpOrWebSearch => &mut settings.no_memories_if_mcp_or_web_search,
                    _ => return,
                };
                *target = match *target {
                    None => Some(!inherited),
                    Some(true) => Some(false),
                    Some(false) => None,
                };
                self.prune_optional_scope();
            }
        }
        self.clear_status();
    }

    fn edit_target_for_row(row: RowKind) -> Option<EditTarget> {
        match row {
            RowKind::MaxRawMemories => Some(EditTarget::MaxRawMemories),
            RowKind::MaxRolloutAgeDays => Some(EditTarget::MaxRolloutAgeDays),
            RowKind::MaxRolloutsPerStartup => Some(EditTarget::MaxRolloutsPerStartup),
            RowKind::MinRolloutIdleHours => Some(EditTarget::MinRolloutIdleHours),
            _ => None,
        }
    }

    fn field_text_for_target(&self, target: EditTarget) -> String {
        let scoped = self.current_scope_settings();
        match (self.scope, target) {
            (MemoriesScopeChoice::Global, EditTarget::MaxRawMemories) => self
                .global_settings
                .max_raw_memories_for_consolidation
                .unwrap_or(self.effective_settings().max_raw_memories_for_consolidation)
                .to_string(),
            (MemoriesScopeChoice::Global, EditTarget::MaxRolloutAgeDays) => self
                .global_settings
                .max_rollout_age_days
                .unwrap_or(self.effective_settings().max_rollout_age_days)
                .to_string(),
            (MemoriesScopeChoice::Global, EditTarget::MaxRolloutsPerStartup) => self
                .global_settings
                .max_rollouts_per_startup
                .unwrap_or(self.effective_settings().max_rollouts_per_startup)
                .to_string(),
            (MemoriesScopeChoice::Global, EditTarget::MinRolloutIdleHours) => self
                .global_settings
                .min_rollout_idle_hours
                .unwrap_or(self.effective_settings().min_rollout_idle_hours)
                .to_string(),
            (_, EditTarget::MaxRawMemories) => scoped
                .and_then(|settings| {
                    settings
                        .max_raw_memories_for_consolidation
                        .or(settings.max_raw_memories_for_global)
                })
                .map(|value| value.to_string())
                .unwrap_or_default(),
            (_, EditTarget::MaxRolloutAgeDays) => scoped
                .and_then(|settings| settings.max_rollout_age_days)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            (_, EditTarget::MaxRolloutsPerStartup) => scoped
                .and_then(|settings| settings.max_rollouts_per_startup)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            (_, EditTarget::MinRolloutIdleHours) => scoped
                .and_then(|settings| settings.min_rollout_idle_hours)
                .map(|value| value.to_string())
                .unwrap_or_default(),
        }
    }

    fn open_edit_for(&mut self, target: EditTarget) {
        let mut field = FormTextField::new_single_line();
        if !matches!(self.scope, MemoriesScopeChoice::Global) {
            field.set_placeholder("inherit");
        }
        field.set_text(&self.field_text_for_target(target));
        self.mode = ViewMode::Edit {
            target,
            field,
            error: None,
        };
    }

    fn apply_numeric_edit(&mut self, target: EditTarget, text: &str) -> Result<(), String> {
        match (self.scope, target) {
            (MemoriesScopeChoice::Global, EditTarget::MaxRawMemories) => {
                let value: usize = text.trim().parse().map_err(|_| {
                    "Max retained memories must be an integer >= 1".to_string()
                })?;
                if value == 0 {
                    return Err("Max retained memories must be >= 1".to_string());
                }
                self.global_settings.max_raw_memories_for_consolidation = Some(value);
                self.global_settings.max_raw_memories_for_global = None;
            }
            (MemoriesScopeChoice::Global, EditTarget::MaxRolloutAgeDays) => {
                let value: i64 = text.trim().parse().map_err(|_| {
                    "Max rollout age must be an integer >= 0".to_string()
                })?;
                if value < 0 {
                    return Err("Max rollout age must be >= 0".to_string());
                }
                self.global_settings.max_rollout_age_days = Some(value);
            }
            (MemoriesScopeChoice::Global, EditTarget::MaxRolloutsPerStartup) => {
                let value: usize = text.trim().parse().map_err(|_| {
                    "Max rollouts per refresh must be an integer >= 1".to_string()
                })?;
                if value == 0 {
                    return Err("Max rollouts per refresh must be >= 1".to_string());
                }
                self.global_settings.max_rollouts_per_startup = Some(value);
            }
            (MemoriesScopeChoice::Global, EditTarget::MinRolloutIdleHours) => {
                let value: i64 = text.trim().parse().map_err(|_| {
                    "Min rollout idle must be an integer >= 0".to_string()
                })?;
                if value < 0 {
                    return Err("Min rollout idle must be >= 0".to_string());
                }
                self.global_settings.min_rollout_idle_hours = Some(value);
            }
            (_, EditTarget::MaxRawMemories) => {
                let settings = self.ensure_current_scope_settings_mut();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    settings.max_raw_memories_for_consolidation = None;
                    settings.max_raw_memories_for_global = None;
                } else {
                    let value: usize = trimmed.parse().map_err(|_| {
                        "Max retained memories must be an integer >= 1".to_string()
                    })?;
                    if value == 0 {
                        return Err("Max retained memories must be >= 1".to_string());
                    }
                    settings.max_raw_memories_for_consolidation = Some(value);
                    settings.max_raw_memories_for_global = None;
                }
                self.prune_optional_scope();
            }
            (_, EditTarget::MaxRolloutAgeDays) => {
                let settings = self.ensure_current_scope_settings_mut();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    settings.max_rollout_age_days = None;
                } else {
                    let value: i64 = trimmed.parse().map_err(|_| {
                        "Max rollout age must be an integer >= 0".to_string()
                    })?;
                    if value < 0 {
                        return Err("Max rollout age must be >= 0".to_string());
                    }
                    settings.max_rollout_age_days = Some(value);
                }
                self.prune_optional_scope();
            }
            (_, EditTarget::MaxRolloutsPerStartup) => {
                let settings = self.ensure_current_scope_settings_mut();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    settings.max_rollouts_per_startup = None;
                } else {
                    let value: usize = trimmed.parse().map_err(|_| {
                        "Max rollouts per refresh must be an integer >= 1".to_string()
                    })?;
                    if value == 0 {
                        return Err("Max rollouts per refresh must be >= 1".to_string());
                    }
                    settings.max_rollouts_per_startup = Some(value);
                }
                self.prune_optional_scope();
            }
            (_, EditTarget::MinRolloutIdleHours) => {
                let settings = self.ensure_current_scope_settings_mut();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    settings.min_rollout_idle_hours = None;
                } else {
                    let value: i64 = trimmed.parse().map_err(|_| {
                        "Min rollout idle must be an integer >= 0".to_string()
                    })?;
                    if value < 0 {
                        return Err("Min rollout idle must be >= 0".to_string());
                    }
                    settings.min_rollout_idle_hours = Some(value);
                }
                self.prune_optional_scope();
            }
        }
        Ok(())
    }

    fn dispatch_apply(&mut self) {
        if matches!(self.scope, MemoriesScopeChoice::Profile) && self.active_profile.is_none() {
            self.set_status("Active profile scope is unavailable.", true);
            return;
        }
        let payload = self.current_scope_payload();
        self.app_event_tx.send(AppEvent::SetMemoriesSettings {
            scope: self.app_scope(),
            settings: payload,
        });
        self.mark_scope_saved();
        self.set_status("Applying memories settings…", false);
    }

    fn trigger_action(&mut self, action: MemoriesArtifactsAction) {
        let message = match action {
            MemoriesArtifactsAction::Refresh => "Refreshing memories artifacts…",
            MemoriesArtifactsAction::Clear => "Clearing generated memories artifacts…",
        };
        self.app_event_tx
            .send(AppEvent::RunMemoriesArtifactsAction { action });
        self.set_status(message, false);
    }

    fn open_memories_directory(&mut self) {
        let path = self.code_home.join("memories");
        match crate::native_file_manager::reveal_path(&path) {
            Ok(()) => self.set_status(format!("Opened {}", path.display()), false),
            Err(err) => self.set_status(
                format!("Failed to open {}: {err}", path.display()),
                true,
            ),
        }
    }

    pub(super) fn activate_selected(&mut self) {
        match self.selected_row() {
            RowKind::Scope => self.cycle_scope_from_row(true),
            RowKind::GenerateMemories | RowKind::UseMemories | RowKind::SkipMcpOrWebSearch => {
                self.toggle_bool_row(self.selected_row())
            }
            RowKind::MaxRawMemories
            | RowKind::MaxRolloutAgeDays
            | RowKind::MaxRolloutsPerStartup
            | RowKind::MinRolloutIdleHours => {
                if let Some(target) = Self::edit_target_for_row(self.selected_row()) {
                    self.open_edit_for(target);
                }
            }
            RowKind::RefreshArtifacts => self.trigger_action(MemoriesArtifactsAction::Refresh),
            RowKind::ClearArtifacts => self.trigger_action(MemoriesArtifactsAction::Clear),
            RowKind::OpenDirectory => self.open_memories_directory(),
            RowKind::Apply => self.dispatch_apply(),
            RowKind::Close => self.is_complete = true,
        }
    }

    fn process_main_key_event(&mut self, key_event: KeyEvent) -> bool {
        let rows = Self::rows();
        let mut state = self.state.get();
        let visible = self.viewport_rows.get().max(1);
        match key_event.code {
            KeyCode::Esc => {
                self.is_complete = true;
                self.state.set(state);
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.move_up_wrap_visible(rows.len(), visible);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.move_down_wrap_visible(rows.len(), visible);
            }
            KeyCode::Left => match self.selected_row() {
                RowKind::Scope => self.cycle_scope_from_row(false),
                RowKind::GenerateMemories | RowKind::UseMemories | RowKind::SkipMcpOrWebSearch => {
                    self.toggle_bool_row(self.selected_row())
                }
                _ => {}
            },
            KeyCode::Right => match self.selected_row() {
                RowKind::Scope => self.cycle_scope_from_row(true),
                RowKind::GenerateMemories | RowKind::UseMemories | RowKind::SkipMcpOrWebSearch => {
                    self.toggle_bool_row(self.selected_row())
                }
                _ => {}
            },
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.activate_selected();
            }
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.dispatch_apply();
            }
            _ => {
                self.state.set(state);
                return false;
            }
        }
        state.ensure_visible(rows.len(), visible);
        self.state.set(state);
        true
    }

    fn process_edit_key_event(&mut self, key_event: KeyEvent) -> bool {
        let mode = std::mem::replace(&mut self.mode, ViewMode::Transition);
        let ViewMode::Edit {
            target,
            mut field,
            mut error,
        } = mode
        else {
            self.mode = mode;
            return false;
        };

        let handled = match key_event.code {
            KeyCode::Esc => {
                self.mode = ViewMode::Main;
                true
            }
            KeyCode::Enter => {
                let text = field.text().to_string();
                match self.apply_numeric_edit(target, &text) {
                    Ok(()) => {
                        self.mode = ViewMode::Main;
                        self.clear_status();
                    }
                    Err(err) => {
                        error = Some(err);
                    }
                }
                true
            }
            KeyCode::Char('s') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                let text = field.text().to_string();
                match self.apply_numeric_edit(target, &text) {
                    Ok(()) => {
                        self.mode = ViewMode::Main;
                        self.clear_status();
                    }
                    Err(err) => {
                        error = Some(err);
                    }
                }
                true
            }
            _ => {
                error = None;
                field.handle_key(key_event)
            }
        };

        if matches!(self.mode, ViewMode::Transition) {
            self.mode = ViewMode::Edit {
                target,
                field,
                error,
            };
        }
        handled
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match self.mode {
            ViewMode::Main => self.process_main_key_event(key_event),
            ViewMode::Edit { .. } => self.process_edit_key_event(key_event),
            ViewMode::Transition => {
                self.mode = ViewMode::Main;
                false
            }
        }
    }
}

