use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl PluginsSettingsView {
    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match self.mode.clone() {
            Mode::List => self.handle_key_list(key_event),
            Mode::Detail { key } => self.handle_key_detail(key_event, key),
            Mode::ConfirmUninstall { plugin_id_key, key } => {
                self.handle_key_confirm_uninstall(key_event, plugin_id_key, key)
            }
            Mode::Sources(mode) => self.handle_key_sources(key_event, mode),
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
            KeyCode::Char('s') => {
                self.sources_list_state.set({
                    let mut state = self.sources_list_state.get();
                    state.selected_idx = Some(0);
                    state.scroll_top = 0;
                    state
                });
                self.sources_list_viewport_rows.set(DEFAULT_LIST_VIEWPORT_ROWS);
                self.mode = Mode::Sources(SourcesMode::List);
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

    fn handle_key_sources(&mut self, key_event: KeyEvent, mode: SourcesMode) -> bool {
        match mode {
            SourcesMode::List => self.handle_key_sources_list(key_event),
            SourcesMode::EditCurated => self.handle_key_sources_editor(key_event, SourcesMode::EditCurated),
            SourcesMode::EditMarketplaceRepo { index } => {
                self.handle_key_sources_editor(key_event, SourcesMode::EditMarketplaceRepo { index })
            }
            SourcesMode::ConfirmRemoveRepo { index } => {
                self.handle_key_sources_confirm_remove(key_event, index)
            }
        }
    }

    fn sources_row_count(snapshot: &PluginsSharedState) -> usize {
        // Curated row + add row + repo rows
        2usize.saturating_add(snapshot.sources.marketplace_repos.len())
    }

    pub(super) fn selected_sources_row_index(&self, row_count: usize) -> usize {
        self.sources_list_state
            .get()
            .selected_idx
            .unwrap_or(0)
            .min(row_count.saturating_sub(1))
    }

    pub(super) fn enter_sources_editor_curated(&mut self) {
        let snapshot = self.shared_snapshot();
        self.sources_editor
            .url_field
            .set_text(snapshot.sources.curated_repo_url.as_deref().unwrap_or(""));
        self.sources_editor
            .ref_field
            .set_text(snapshot.sources.curated_repo_ref.as_deref().unwrap_or(""));
        self.sources_editor.selected_row = 0;
        self.sources_editor.hovered_button = None;
        self.sources_editor.focused_button = SourcesEditorAction::Save;
        self.sources_editor.error = None;
        self.mode = Mode::Sources(SourcesMode::EditCurated);
    }

    pub(super) fn enter_sources_editor_repo(&mut self, index: Option<usize>) {
        let snapshot = self.shared_snapshot();
        let (url, git_ref) = match index {
            Some(idx) => snapshot
                .sources
                .marketplace_repos
                .get(idx)
                .map(|repo| (repo.url.as_str(), repo.git_ref.as_deref().unwrap_or("")))
                .unwrap_or(("", "")),
            None => ("", ""),
        };
        self.sources_editor.url_field.set_text(url);
        self.sources_editor.ref_field.set_text(git_ref);
        self.sources_editor.selected_row = 0;
        self.sources_editor.hovered_button = None;
        self.sources_editor.focused_button = SourcesEditorAction::Save;
        self.sources_editor.error = None;
        self.mode = Mode::Sources(SourcesMode::EditMarketplaceRepo { index });
    }

    fn handle_key_sources_list(&mut self, key_event: KeyEvent) -> bool {
        let snapshot = self.shared_snapshot();
        let row_count = Self::sources_row_count(&snapshot);

        match key_event.code {
            KeyCode::Esc => {
                self.mode = Mode::List;
                true
            }
            KeyCode::Up => {
                let mut state = self.sources_list_state.get();
                state.move_up_wrap_visible(row_count, self.sources_list_viewport_rows.get().max(1));
                self.sources_list_state.set(state);
                true
            }
            KeyCode::Down => {
                let mut state = self.sources_list_state.get();
                state.move_down_wrap_visible(row_count, self.sources_list_viewport_rows.get().max(1));
                self.sources_list_state.set(state);
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let idx = self.selected_sources_row_index(row_count);
                match idx {
                    0 => {
                        self.enter_sources_editor_curated();
                        true
                    }
                    1 => {
                        self.enter_sources_editor_repo(None);
                        true
                    }
                    _ => {
                        let repo_idx = idx.saturating_sub(2);
                        self.enter_sources_editor_repo(Some(repo_idx));
                        true
                    }
                }
            }
            KeyCode::Char('a') => {
                self.enter_sources_editor_repo(None);
                true
            }
            KeyCode::Backspace | KeyCode::Delete => {
                let idx = self.selected_sources_row_index(row_count);
                match idx {
                    0 => {
                        let mut sources = snapshot.sources.clone();
                        sources.curated_repo_url = None;
                        sources.curated_repo_ref = None;
                        self.request_set_plugin_marketplace_sources(sources);
                        true
                    }
                    2.. => {
                        let repo_idx = idx.saturating_sub(2);
                        self.focused_sources_confirm_button = SourcesConfirmRemoveAction::Cancel;
                        self.hovered_sources_confirm_button = None;
                        self.mode = Mode::Sources(SourcesMode::ConfirmRemoveRepo { index: repo_idx });
                        true
                    }
                    _ => false,
                }
            }
            KeyCode::Char('r') => {
                self.request_plugin_list(/*force_remote_sync*/ false);
                true
            }
            KeyCode::Char('R') => {
                self.request_sync_plugin_marketplaces(/*refresh_list_after*/ true);
                true
            }
            _ => false,
        }
    }

    fn handle_key_sources_editor(&mut self, key_event: KeyEvent, mode: SourcesMode) -> bool {
        let is_ctrl_s = key_event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('s') | KeyCode::Char('S'));
        if is_ctrl_s {
            return self.save_sources_editor(mode);
        }

        const ROW_COUNT: usize = 4;
        match key_event.code {
            KeyCode::Esc => {
                self.mode = Mode::Sources(SourcesMode::List);
                self.sources_editor.error = None;
                true
            }
            KeyCode::Tab => {
                self.sources_editor.selected_row = (self.sources_editor.selected_row + 1) % ROW_COUNT;
                self.sync_sources_editor_button_focus();
                true
            }
            KeyCode::BackTab => {
                if self.sources_editor.selected_row == 0 {
                    self.sources_editor.selected_row = ROW_COUNT - 1;
                } else {
                    self.sources_editor.selected_row = self.sources_editor.selected_row.saturating_sub(1);
                }
                self.sync_sources_editor_button_focus();
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.sources_editor.selected_row {
                0 | 1 => {
                    self.sources_editor.selected_row = (self.sources_editor.selected_row + 1) % ROW_COUNT;
                    self.sync_sources_editor_button_focus();
                    true
                }
                2 => self.save_sources_editor(mode),
                3 => {
                    self.mode = Mode::Sources(SourcesMode::List);
                    self.sources_editor.error = None;
                    true
                }
                _ => false,
            },
            _ => match self.sources_editor.selected_row {
                0 => self.sources_editor.url_field.handle_key(key_event),
                1 => self.sources_editor.ref_field.handle_key(key_event),
                _ => false,
            },
        }
    }

    fn sync_sources_editor_button_focus(&mut self) {
        self.sources_editor.focused_button = match self.sources_editor.selected_row {
            3 => SourcesEditorAction::Cancel,
            _ => SourcesEditorAction::Save,
        };
    }

    pub(super) fn save_sources_editor(&mut self, mode: SourcesMode) -> bool {
        let snapshot = self.shared_snapshot();
        let mut sources = snapshot.sources.clone();
        self.sources_editor.error = None;

        match mode {
            SourcesMode::EditCurated => {
                let url = self.sources_editor.url_field.text().trim();
                if url.is_empty() {
                    sources.curated_repo_url = None;
                    sources.curated_repo_ref = None;
                } else {
                    sources.curated_repo_url = Some(url.to_string());
                    let git_ref = self.sources_editor.ref_field.text().trim();
                    if git_ref.is_empty() {
                        sources.curated_repo_ref = None;
                    } else {
                        sources.curated_repo_ref = Some(git_ref.to_string());
                    }
                }
                self.request_set_plugin_marketplace_sources(sources);
                self.mode = Mode::Sources(SourcesMode::List);
                true
            }
            SourcesMode::EditMarketplaceRepo { index } => {
                let url = self.sources_editor.url_field.text().trim();
                if url.is_empty() {
                    self.sources_editor.error = Some("Marketplace repo URL is required.".to_string());
                    return true;
                }
                let git_ref = self.sources_editor.ref_field.text().trim();
                let repo = PluginMarketplaceRepoToml {
                    url: url.to_string(),
                    git_ref: (!git_ref.is_empty()).then(|| git_ref.to_string()),
                };
                match index {
                    Some(idx) if idx < sources.marketplace_repos.len() => {
                        sources.marketplace_repos[idx] = repo;
                    }
                    Some(_) | None => {
                        sources.marketplace_repos.push(repo);
                    }
                }
                self.request_set_plugin_marketplace_sources(sources);
                self.mode = Mode::Sources(SourcesMode::List);
                true
            }
            _ => false,
        }
    }

    fn handle_key_sources_confirm_remove(&mut self, key_event: KeyEvent, index: usize) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.mode = Mode::Sources(SourcesMode::List);
                true
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
                self.focused_sources_confirm_button = match self.focused_sources_confirm_button {
                    SourcesConfirmRemoveAction::Delete => SourcesConfirmRemoveAction::Cancel,
                    SourcesConfirmRemoveAction::Cancel => SourcesConfirmRemoveAction::Delete,
                };
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.focused_sources_confirm_button {
                SourcesConfirmRemoveAction::Cancel => {
                    self.mode = Mode::Sources(SourcesMode::List);
                    true
                }
                SourcesConfirmRemoveAction::Delete => {
                    let snapshot = self.shared_snapshot();
                    let mut sources = snapshot.sources.clone();
                    if index < sources.marketplace_repos.len() {
                        sources.marketplace_repos.remove(index);
                        self.request_set_plugin_marketplace_sources(sources);
                    }
                    self.mode = Mode::Sources(SourcesMode::List);
                    true
                }
            },
            _ => false,
        }
    }
}
