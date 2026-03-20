            match event {
                AppEvent::SetAutoUpgradeEnabled(enabled) => {
                    if crate::updates::upgrade_ui_enabled() {
                        if let AppState::Chat { widget } = &mut self.app_state {
                            widget.set_auto_upgrade_enabled(enabled);
                        }
                        self.config.auto_upgrade_enabled = enabled;
                    }
                }
                AppEvent::SetMemoriesSettings { scope, settings } => {
                    let persist_result = match &scope {
                        crate::app_event::MemoriesSettingsScope::Global => {
                            code_core::config::set_global_memories_settings(
                                &self.config.code_home,
                                (!settings.is_empty()).then_some(&settings),
                            )
                        }
                        crate::app_event::MemoriesSettingsScope::Profile { name } => {
                            code_core::config::set_profile_memories_settings(
                                &self.config.code_home,
                                name,
                                (!settings.is_empty()).then_some(&settings),
                            )
                        }
                        crate::app_event::MemoriesSettingsScope::Project { path } => {
                            code_core::config::set_project_memories_settings(
                                &self.config.code_home,
                                path,
                                (!settings.is_empty()).then_some(&settings),
                            )
                        }
                    };
                    match persist_result {
                        Ok(()) => {
                            let updated_settings = (!settings.is_empty()).then_some(settings.clone());
                            let scope_label = match &scope {
                                crate::app_event::MemoriesSettingsScope::Global => "global",
                                crate::app_event::MemoriesSettingsScope::Profile { .. } => "profile",
                                crate::app_event::MemoriesSettingsScope::Project { .. } => "project",
                            };
                            match scope {
                                crate::app_event::MemoriesSettingsScope::Global => {
                                    self.config.global_memories = updated_settings;
                                }
                                crate::app_event::MemoriesSettingsScope::Profile { .. } => {
                                    self.config.active_profile_memories = updated_settings;
                                }
                                crate::app_event::MemoriesSettingsScope::Project { .. } => {
                                    self.config.project_memories = updated_settings;
                                }
                            }
                            self.config.memories = code_core::config_types::resolve_memories_config(
                                self.config.global_memories.as_ref(),
                                self.config.active_profile_memories.as_ref(),
                                self.config.project_memories.as_ref(),
                            );
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.apply_memories_settings(self.config.memories.clone());
                                widget.flash_footer_notice(format!("Memories: updated ({scope_label})"));
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to persist memories settings: {err}",
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::RunMemoriesStatusLoad { target } => {
                    let code_home = self.config.code_home.clone();
                    let global = self.config.global_memories.clone();
                    let profile = self.config.active_profile_memories.clone();
                    let project = self.config.project_memories.clone();
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let result = code_core::load_memories_status(
                            &code_home,
                            global.as_ref(),
                            profile.as_ref(),
                            project.as_ref(),
                        )
                        .await
                        .map_err(|err| err.to_string());
                        tx.send(AppEvent::MemoriesStatusLoaded { target, result });
                    });
                }
                AppEvent::MemoriesStatusLoaded { target, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_memories_status_loaded(target, result);
                    }
                    self.schedule_redraw();
                }
                AppEvent::RunMemoriesArtifactsAction { action } => {
                    let code_home = self.config.code_home.clone();
                    let settings = self.config.memories.clone();
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let result = match action {
                            crate::app_event::MemoriesArtifactsAction::Refresh => {
                                code_core::refresh_memory_artifacts_now(&code_home, &settings)
                                    .await
                                    .map(|_| "Memories: artifacts refreshed".to_string())
                            }
                            crate::app_event::MemoriesArtifactsAction::Clear => {
                                code_core::clear_generated_memory_artifacts(&code_home)
                                    .await
                                    .map(|_| "Memories: generated artifacts cleared".to_string())
                            }
                        }
                        .map_err(|err| match action {
                            crate::app_event::MemoriesArtifactsAction::Refresh => {
                                format!("Failed to refresh memories artifacts: {err}")
                            }
                            crate::app_event::MemoriesArtifactsAction::Clear => {
                                format!("Failed to clear memories artifacts: {err}")
                            }
                        });
                        tx.send(AppEvent::MemoriesArtifactsActionFinished {
                            _action: action,
                            result,
                        });
                    });
                }
                AppEvent::MemoriesArtifactsActionFinished { _action: _, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        match result {
                            Ok(message) => widget.flash_footer_notice(message),
                            Err(message) => widget.flash_footer_notice(message),
                        }
                    }
                    self.app_event_tx.send(AppEvent::RunMemoriesStatusLoad {
                        target: crate::app_event::MemoriesStatusLoadTarget::RefreshCacheOnly,
                    });
                    self.schedule_redraw();
                }
                event => {
                    include!("tools_and_ui_settings.rs");
                }
            }
