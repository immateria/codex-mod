            match event {
                #[cfg(feature = "managed-network-proxy")]
                AppEvent::SetNetworkProxySettings(settings) => {
                    match code_core::config::set_network_proxy_settings(
                        &self.config.code_home,
                        &settings,
                    ) {
                        Ok(()) => {
                            self.config.network = Some(settings.clone());
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let status = if settings.enabled { "Enabled" } else { "Disabled" };
                                widget.apply_network_proxy_settings(Some(settings));
                                widget.flash_footer_notice(format!("Network mediation: {status}"));
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to persist network settings: {err}",
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::SetExecLimitsSettings(settings) => {
                    match code_core::config::set_exec_limits_settings(&self.config.code_home, &settings) {
                        Ok(()) => {
                            self.config.exec_limits = settings.clone();
                            if let AppState::Chat { widget } = &mut self.app_state {
                                match code_core::config::apply_exec_limits_settings(&settings) {
                                    Ok(()) => {
                                        widget.apply_exec_limits_settings(settings);
                                        widget.flash_footer_notice("Exec limits: updated".to_string());
                                    }
                                    Err(err) => {
                                        widget.flash_footer_notice(format!(
                                            "Failed to apply exec limits: {err}",
                                        ));
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to persist exec limits: {err}",
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::SetJsReplSettings(settings) => {
                    match code_core::config::set_js_repl_settings(&self.config.code_home, &settings) {
                        Ok(()) => {
                            self.config.tools_js_repl = settings.enabled;
                            self.config.js_repl_runtime = settings.runtime;
                            self.config.js_repl_runtime_path = settings.runtime_path.clone();
                            self.config.js_repl_runtime_args = settings.runtime_args.clone();
                            self.config.js_repl_node_module_dirs = settings.node_module_dirs.clone();
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.apply_js_repl_settings(settings);
                                let status = if self.config.tools_js_repl { "Enabled" } else { "Disabled" };
                                widget.flash_footer_notice(format!("JS REPL: {status}"));
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to persist JS REPL settings: {err}",
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::SetTuiSettingsMenuConfig(settings) => {
                    match code_core::config::set_tui_settings_menu(&self.config.code_home, &settings) {
                        Ok(()) => {
                            self.config.tui.settings_menu = settings.clone();
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.apply_tui_settings_menu(settings.clone());
                                let mode = match settings.open_mode {
                                    code_core::config_types::SettingsMenuOpenMode::Auto => {
                                        format!("auto (>= {})", settings.overlay_min_width)
                                    }
                                    code_core::config_types::SettingsMenuOpenMode::Overlay => {
                                        "overlay".to_string()
                                    }
                                    code_core::config_types::SettingsMenuOpenMode::Bottom => {
                                        "bottom".to_string()
                                    }
                                };
                                widget.flash_footer_notice(format!("Settings UI: {mode}"));
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to persist settings UI preferences: {err}",
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::SetTuiHotkeysConfig(hotkeys) => {
                    match code_core::config::set_tui_hotkeys(&self.config.code_home, &hotkeys) {
                        Ok(()) => {
                            self.config.tui.hotkeys = hotkeys.clone();
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.apply_tui_hotkeys(hotkeys.clone());
                                widget.flash_footer_notice("Hotkeys saved".to_string());
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to persist hotkeys: {err}",
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::StatusLineSetup {
                    top_items,
                    bottom_items,
                    primary,
                } => {
                    let top_ids = top_items.iter().map(ToString::to_string).collect::<Vec<_>>();
                    let bottom_ids = bottom_items
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    match code_core::config::set_tui_status_line_layout(
                        &self.config.code_home,
                        &top_ids,
                        &bottom_ids,
                        primary,
                    ) {
                        Ok(()) => {
                            self.config.tui.status_line_top = if top_ids.is_empty() {
                                None
                            } else {
                                Some(top_ids)
                            };
                            self.config.tui.status_line_bottom = if bottom_ids.is_empty() {
                                None
                            } else {
                                Some(bottom_ids)
                            };
                            self.config.tui.status_line_primary = primary;
                            self.config.tui.status_line = self.config.tui.status_line_top.clone();
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.setup_status_line(top_items, bottom_items, primary);
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to persist status line config: {err}",
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::StatusLineSetupCancelled => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.cancel_status_line_setup();
                    }
                    self.schedule_redraw();
                }
                AppEvent::FetchPluginsList { roots, force_remote_sync } => {
                    let auth = if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_mark_list_loading(roots.clone(), force_remote_sync);
                        widget.auth_manager().auth()
                    } else {
                        None
                    };
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let config = self.config.clone();
                    tokio::spawn(async move {
                        let manager = code_core::plugins::PluginsManager::new(code_home);

                        let mut remote_sync_error = None;
                        let mut remote_sync_needs_auth = false;
                        if force_remote_sync {
                            if let Err(err) = manager.sync_marketplace_sources(&config).await {
                                remote_sync_error = Some(err);
                            }
                            if let Err(err) = manager
                                .sync_plugins_from_remote(&config, auth.as_ref(), /*additive_only*/ false)
                                .await
                            {
                                remote_sync_needs_auth |= match &err {
                                    code_core::plugins::PluginRemoteSyncError::AuthRequired
                                    | code_core::plugins::PluginRemoteSyncError::UnsupportedAuthMode
                                    | code_core::plugins::PluginRemoteSyncError::AuthToken(_) => true,
                                    code_core::plugins::PluginRemoteSyncError::UnexpectedStatus { status, .. } => {
                                        let code = status.as_u16();
                                        code == 401 || code == 403
                                    }
                                    _ => false,
                                };
                                match remote_sync_error.as_mut() {
                                    Some(existing) => {
                                        existing.push_str("; ");
                                        existing.push_str(&err.to_string());
                                    }
                                    None => remote_sync_error = Some(err.to_string()),
                                }
                            }
                        }

                        let marketplaces_outcome = manager
                            .list_marketplaces_for_roots(&config, &roots)
                            .map_err(|err| err.to_string());

                        let featured_plugin_ids = manager
                            .featured_plugin_ids_for_config(&config, auth.as_ref())
                            .await
                            .unwrap_or_else(|err| {
                                tracing::debug!("failed to fetch featured plugin ids: {err}");
                                Vec::new()
                            });

                        let result = marketplaces_outcome.map(|outcome| crate::app_event::PluginListSnapshot {
                            marketplaces: outcome.marketplaces,
                            marketplace_load_errors: outcome.errors,
                            remote_sync_error,
                            remote_sync_needs_auth,
                            featured_plugin_ids,
                        });

                        tx.send(AppEvent::PluginsListLoaded { roots, result });
                    });
                }
                AppEvent::PluginsListLoaded { roots, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_apply_list_loaded(roots, result);
                    }
                    self.schedule_redraw();
                }
                AppEvent::FetchPluginDetail { request } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        let key = crate::chatwidget::PluginDetailKey::new(
                            request.marketplace_path.clone(),
                            request.plugin_name.clone(),
                        );
                        widget.plugins_mark_detail_loading(key);
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    tokio::spawn(async move {
                        let request_for_event = request.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            let manager = code_core::plugins::PluginsManager::new(code_home);
                            manager.read_plugin_for_config(&request)
                        })
                        .await
                        .map_err(|err| err.to_string())
                        .and_then(|result| result.map_err(|err| err.to_string()));

                        tx.send(AppEvent::PluginDetailLoaded {
                            request: request_for_event,
                            result,
                        });
                    });
                }
                AppEvent::PluginDetailLoaded { request, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        let key = crate::chatwidget::PluginDetailKey::new(
                            request.marketplace_path.clone(),
                            request.plugin_name.clone(),
                        );
                        widget.plugins_apply_detail_loaded(key, result);
                    }
                    self.schedule_redraw();
                }
                AppEvent::InstallPlugin { request, force_remote_sync } => {
                    let auth = if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_set_action_in_progress(crate::chatwidget::PluginsActionInProgress::Install {
                            marketplace_path: request.marketplace_path.clone(),
                            plugin_name: request.plugin_name.clone(),
                            force_remote_sync,
                        });
                        widget.auth_manager().auth()
                    } else {
                        None
                    };
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let config = self.config.clone();
                    tokio::spawn(async move {
                        let manager = code_core::plugins::PluginsManager::new(code_home);
                        let result = if force_remote_sync {
                            manager
                                .install_plugin_with_remote_sync(&config, auth.as_ref(), request.clone())
                                .await
                                .map(|_| ())
                        } else {
                            manager.install_plugin(request.clone()).await.map(|_| ())
                        };

                        tx.send(AppEvent::PluginInstallFinished {
                            request,
                            force_remote_sync,
                            result: result.map_err(|err| err.to_string()),
                        });
                    });
                }
                AppEvent::UninstallPlugin { plugin_id_key, force_remote_sync } => {
                    let auth = if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_set_action_in_progress(crate::chatwidget::PluginsActionInProgress::Uninstall {
                            plugin_id_key: plugin_id_key.clone(),
                            force_remote_sync,
                        });
                        widget.auth_manager().auth()
                    } else {
                        None
                    };
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let config = self.config.clone();
                    tokio::spawn(async move {
                        let manager = code_core::plugins::PluginsManager::new(code_home);
                        let result = if force_remote_sync {
                            manager
                                .uninstall_plugin_with_remote_sync(&config, auth.as_ref(), plugin_id_key.clone())
                                .await
                        } else {
                            manager.uninstall_plugin(plugin_id_key.clone()).await
                        };

                        tx.send(AppEvent::PluginUninstallFinished {
                            plugin_id_key,
                            force_remote_sync,
                            result: result.map_err(|err| err.to_string()),
                        });
                    });
                }
                AppEvent::SetPluginEnabled { plugin_id_key, enabled } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_set_action_in_progress(crate::chatwidget::PluginsActionInProgress::SetEnabled {
                            plugin_id_key: plugin_id_key.clone(),
                            enabled,
                        });
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    tokio::spawn(async move {
                        let result = code_core::config_edit::set_plugin_enabled(
                            code_home.as_path(),
                            &plugin_id_key,
                            enabled,
                        )
                        .await
                        .map(|_| ());

                        tx.send(AppEvent::PluginEnabledSetFinished {
                            plugin_id_key,
                            enabled,
                            result: result.map_err(|err| err.to_string()),
                        });
                    });
                }
                AppEvent::SetPluginMarketplaceSources { roots, sources } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_set_sources_sync_status(/*in_progress*/ true, None);
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    tokio::spawn(async move {
                        let result = code_core::config_edit::set_plugin_marketplace_sources(
                            code_home.as_path(),
                            &sources,
                        )
                        .await
                        .map_err(|err| err.to_string());

                        tx.send(AppEvent::PluginMarketplaceSourcesSetFinished {
                            roots,
                            sources,
                            result,
                        });
                    });
                }
                AppEvent::PluginMarketplaceSourcesSetFinished { roots, sources, result } => {
                    match result {
                        Ok(mutated) => {
                            if mutated {
                                let reload = self.reload_config_with_startup_overrides();
                                match reload {
                                    Ok(config) => {
                                        self.config = config.clone();
                                        if let AppState::Chat { widget } = &mut self.app_state {
                                            widget.apply_reloaded_config_keep_settings_state(config);
                                        }
                                        self.app_event_tx.send(AppEvent::SyncPluginMarketplaces {
                                            roots,
                                            refresh_list_after: true,
                                        });
                                    }
                                    Err(err) => {
                                        if let AppState::Chat { widget } = &mut self.app_state {
                                            widget.plugins_set_sources_sync_status(
                                                /*in_progress*/ false,
                                                Some(format!(
                                                    "Saved plugin sources, but failed to reload config: {err}",
                                                )),
                                            );
                                            let mut config = self.config.clone();
                                            config.plugins = sources;
                                            self.config = config.clone();
                                            widget.apply_reloaded_config_keep_settings_state(config);
                                        } else {
                                            self.config.plugins = sources;
                                        }
                                    }
                                }
                            } else if let AppState::Chat { widget } = &mut self.app_state {
                                widget.plugins_set_sources_sync_status(/*in_progress*/ false, None);
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.plugins_set_sources_snapshot(self.config.plugins.clone());
                                widget.plugins_set_sources_sync_status(
                                    /*in_progress*/ false,
                                    Some(format!("Failed to save plugin sources: {err}")),
                                );
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::SyncPluginMarketplaces { roots, refresh_list_after } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_set_sources_sync_status(/*in_progress*/ true, None);
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let config = self.config.clone();
                    tokio::spawn(async move {
                        let manager = code_core::plugins::PluginsManager::new(code_home);
                        let result = manager
                            .sync_marketplace_sources(&config)
                            .await;
                        tx.send(AppEvent::PluginMarketplacesSynced {
                            roots,
                            refresh_list_after,
                            result,
                        });
                    });
                }
                AppEvent::PluginMarketplacesSynced { roots, refresh_list_after, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        let error = match result.as_ref() {
                            Ok(()) => None,
                            Err(err) => Some(err.clone()),
                        };
                        widget.plugins_set_sources_sync_status(/*in_progress*/ false, error);
                    }
                    if refresh_list_after {
                        self.app_event_tx.send(AppEvent::FetchPluginsList {
                            roots,
                            force_remote_sync: false,
                        });
                    }
                    self.schedule_redraw();
                }
                AppEvent::PluginInstallFinished { request, force_remote_sync: _force_remote_sync, result } => {
                    let reload = if result.is_ok() {
                        Some(self.reload_config_with_startup_overrides())
                    } else {
                        None
                    };
                    let mut should_refresh_list = false;
                    let mut refresh_roots: Option<Vec<code_utils_absolute_path::AbsolutePathBuf>> = None;

                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_clear_action_in_progress();
                        match result {
                            Ok(()) => {
                                widget.plugins_set_action_error(None);
                                widget.flash_footer_notice(format!(
                                    "Installed plugin: {}",
                                    request.plugin_name,
                                ));

                                match reload {
                                    Some(Ok(config)) => {
                                        self.config = config.clone();
                                        widget.apply_reloaded_config(config);
                                        widget.submit_op(widget.current_configure_session_op());
                                        widget.submit_op(Op::ListSkills);
                                        widget.submit_op(Op::RefreshMcpTools);

                                        should_refresh_list = true;
                                        refresh_roots = widget
                                            .plugins_shared_state()
                                            .lock()
                                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                                            .list
                                            .roots()
                                            .map(<[_]>::to_vec);
                                    }
                                    Some(Err(err)) => {
                                        widget.flash_footer_notice(format!(
                                            "Failed to reload config after install: {err}",
                                        ));
                                    }
                                    None => {}
                                }
                            }
                            Err(err) => {
                                widget.plugins_set_action_error(Some(err.clone()));
                                widget.flash_footer_notice(format!("Plugin install failed: {err}"));
                            }
                        }
                    }

                    if should_refresh_list {
                        let roots = refresh_roots.unwrap_or_else(|| {
                            code_utils_absolute_path::AbsolutePathBuf::try_from(self.config.cwd.clone())
                                .ok()
                                .into_iter()
                                .collect::<Vec<_>>()
                        });
                        self.app_event_tx.send(AppEvent::FetchPluginsList {
                            roots,
                            force_remote_sync: false,
                        });
                    }
                    self.schedule_redraw();
                }
                AppEvent::PluginUninstallFinished {
                    plugin_id_key: _plugin_id_key,
                    force_remote_sync: _force_remote_sync,
                    result,
                } => {
                    let reload = if result.is_ok() {
                        Some(self.reload_config_with_startup_overrides())
                    } else {
                        None
                    };
                    let mut should_refresh_list = false;
                    let mut refresh_roots: Option<Vec<code_utils_absolute_path::AbsolutePathBuf>> = None;

                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_clear_action_in_progress();
                        match result {
                            Ok(()) => {
                                widget.plugins_set_action_error(None);
                                widget.flash_footer_notice("Plugin uninstalled".to_string());

                                match reload {
                                    Some(Ok(config)) => {
                                        self.config = config.clone();
                                        widget.apply_reloaded_config(config);
                                        widget.submit_op(widget.current_configure_session_op());
                                        widget.submit_op(Op::ListSkills);
                                        widget.submit_op(Op::RefreshMcpTools);

                                        should_refresh_list = true;
                                        refresh_roots = widget
                                            .plugins_shared_state()
                                            .lock()
                                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                                            .list
                                            .roots()
                                            .map(<[_]>::to_vec);
                                    }
                                    Some(Err(err)) => {
                                        widget.flash_footer_notice(format!(
                                            "Failed to reload config after uninstall: {err}",
                                        ));
                                    }
                                    None => {}
                                }
                            }
                            Err(err) => {
                                widget.plugins_set_action_error(Some(err.clone()));
                                widget.flash_footer_notice(format!("Plugin uninstall failed: {err}"));
                            }
                        }
                    }

                    if should_refresh_list {
                        let roots = refresh_roots.unwrap_or_else(|| {
                            code_utils_absolute_path::AbsolutePathBuf::try_from(self.config.cwd.clone())
                                .ok()
                                .into_iter()
                                .collect::<Vec<_>>()
                        });
                        self.app_event_tx.send(AppEvent::FetchPluginsList {
                            roots,
                            force_remote_sync: false,
                        });
                    }
                    self.schedule_redraw();
                }
                AppEvent::PluginEnabledSetFinished {
                    plugin_id_key: _plugin_id_key,
                    enabled: _enabled,
                    result,
                } => {
                    let reload = if result.is_ok() {
                        Some(self.reload_config_with_startup_overrides())
                    } else {
                        None
                    };
                    let mut should_refresh_list = false;
                    let mut refresh_roots: Option<Vec<code_utils_absolute_path::AbsolutePathBuf>> = None;

                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.plugins_clear_action_in_progress();
                        match result {
                            Ok(()) => {
                                widget.plugins_set_action_error(None);
                                widget.flash_footer_notice("Plugin setting updated".to_string());

                                match reload {
                                    Some(Ok(config)) => {
                                        self.config = config.clone();
                                        widget.apply_reloaded_config(config);
                                        widget.submit_op(widget.current_configure_session_op());
                                        widget.submit_op(Op::ListSkills);
                                        widget.submit_op(Op::RefreshMcpTools);

                                        should_refresh_list = true;
                                        refresh_roots = widget
                                            .plugins_shared_state()
                                            .lock()
                                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                                            .list
                                            .roots()
                                            .map(<[_]>::to_vec);
                                    }
                                    Some(Err(err)) => {
                                        widget.flash_footer_notice(format!(
                                            "Failed to reload config after change: {err}",
                                        ));
                                    }
                                    None => {}
                                }
                            }
                            Err(err) => {
                                widget.plugins_set_action_error(Some(err.clone()));
                                widget.flash_footer_notice(format!("Plugin update failed: {err}"));
                            }
                        }
                    }

                    if should_refresh_list {
                        let roots = refresh_roots.unwrap_or_else(|| {
                            code_utils_absolute_path::AbsolutePathBuf::try_from(self.config.cwd.clone())
                                .ok()
                                .into_iter()
                                .collect::<Vec<_>>()
                        });
                        self.app_event_tx.send(AppEvent::FetchPluginsList {
                            roots,
                            force_remote_sync: false,
                        });
                    }
                    self.schedule_redraw();
                }
                AppEvent::SetAppsSources { sources } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apps_set_action_in_progress(crate::chatwidget::AppsActionInProgress::SaveSources);
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let profile = self.config.active_profile.clone();
                    tokio::spawn(async move {
                        let result = code_core::config_edit::set_apps_sources(
                            code_home.as_path(),
                            profile.as_deref(),
                            &sources,
                        )
                        .await
                        .map_err(|err| err.to_string());

                        tx.send(AppEvent::AppsSourcesSetFinished { sources, result });
                    });
                }
                AppEvent::AppsSourcesSetFinished { sources, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apps_clear_action_in_progress();
                    }

                    match result {
                        Ok(mutated) => {
                            if mutated {
                                match self.reload_config_with_startup_overrides() {
                                    Ok(config) => {
                                        self.config = config.clone();
                                        if let AppState::Chat { widget } = &mut self.app_state {
                                            widget.apply_reloaded_config_keep_settings_state(config);
                                            widget.submit_op(widget.current_configure_session_op());
                                            widget.submit_op(Op::RefreshMcpTools);
                                        }

                                        let active_account_id = code_core::apps_sources::active_chatgpt_account_id(&self.config.code_home)
                                            .unwrap_or_default();
                                        let effective_source_ids = code_core::apps_sources::effective_source_account_ids(
                                            &self.config.apps_sources,
                                            active_account_id.as_deref(),
                                        );
                                        if !effective_source_ids.is_empty() {
                                            self.app_event_tx.send(AppEvent::FetchAppsStatus {
                                                account_ids: effective_source_ids,
                                                force_refresh_tools: true,
                                            });
                                        }
                                    }
                                    Err(err) => {
                                        if let AppState::Chat { widget } = &mut self.app_state {
                                            widget.apps_set_action_error(Some(format!(
                                                "Saved Apps sources, but failed to reload config: {err}",
                                            )));
                                            widget.apps_set_sources_snapshot(
                                                self.config.active_profile.clone(),
                                                sources.clone(),
                                            );
                                        }
                                        self.config.apps_sources = sources;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.apps_set_action_error(Some(format!(
                                    "Failed to save Apps sources: {err}",
                                )));
                                widget.apps_set_sources_snapshot(
                                    self.config.active_profile.clone(),
                                    self.config.apps_sources.clone(),
                                );
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::UpdateFeatureFlags { updates } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.flash_footer_notice("Saving experimental features...".to_string());
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let profile = self.config.active_profile.clone();
                    tokio::spawn(async move {
                        let result = code_core::config_edit::set_feature_flags(
                            code_home.as_path(),
                            profile.as_deref(),
                            &updates,
                        )
                        .await
                        .map_err(|err| err.to_string());

                        tx.send(AppEvent::UpdateFeatureFlagsFinished { result });
                    });
                }
                AppEvent::UpdateFeatureFlagsFinished { result } => {
                    match result {
                        Ok(mutated) => {
                            if mutated {
                                match self.reload_config_with_startup_overrides() {
                                    Ok(config) => {
                                        self.config = config.clone();
                                        if let AppState::Chat { widget } = &mut self.app_state {
                                            widget.apply_reloaded_config_keep_settings_state(config);
                                            widget.submit_op(widget.current_configure_session_op());
                                        }
                                    }
                                    Err(err) => {
                                        if let AppState::Chat { widget } = &mut self.app_state {
                                            widget.flash_footer_notice(format!(
                                                "Saved features, but failed to reload config: {err}",
                                            ));
                                        }
                                    }
                                }
                            } else if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice("Experimental features unchanged".to_string());
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to save experimental features: {err}",
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::UpdateShellEscalationSettings { enabled, zsh_path, wrapper } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.flash_footer_notice("Saving shell escalation settings...".to_string());
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    let profile = self.config.active_profile.clone();
                    tokio::spawn(async move {
                        let result = code_core::config_edit::set_shell_escalation_settings(
                            code_home.as_path(),
                            profile.as_deref(),
                            enabled,
                            zsh_path.as_deref(),
                            wrapper.as_deref(),
                        )
                        .await
                        .map_err(|err| err.to_string());

                        tx.send(AppEvent::UpdateShellEscalationSettingsFinished { result });
                    });
                }
                AppEvent::UpdateShellEscalationSettingsFinished { result } => {
                    match result {
                        Ok(mutated) => {
                            if mutated {
                                match self.reload_config_with_startup_overrides() {
                                    Ok(config) => {
                                        self.config = config.clone();
                                        if let AppState::Chat { widget } = &mut self.app_state {
                                            widget.apply_reloaded_config_keep_settings_state(config);
                                            widget.submit_op(widget.current_configure_session_op());
                                        }
                                    }
                                    Err(err) => {
                                        if let AppState::Chat { widget } = &mut self.app_state {
                                            widget.flash_footer_notice(format!(
                                                "Saved shell escalation settings, but failed to reload config: {err}",
                                            ));
                                        }
                                    }
                                }
                            } else if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(
                                    "Shell escalation settings unchanged".to_string(),
                                );
                            }
                        }
                        Err(err) => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.flash_footer_notice(format!(
                                    "Failed to save shell escalation settings: {err}",
                                ));
                            }
                        }
                    }
                    self.schedule_redraw();
                }
                AppEvent::FetchSecretsList { env_id } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.secrets_mark_list_loading(env_id.clone());
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    tokio::spawn(async move {
                        let env_id_for_list = env_id.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            let manager = code_secrets::SecretsManager::new(
                                code_home,
                                code_secrets::SecretsBackendKind::Local,
                            );

                            let mut entries = Vec::new();
                            entries.extend(manager.list(Some(&code_secrets::SecretScope::Global))?);
                            entries.extend(manager.list(Some(&code_secrets::SecretScope::Environment(
                                env_id_for_list,
                            )))?);

                            fn scope_rank(scope: &code_secrets::SecretScope) -> u8 {
                                match scope {
                                    code_secrets::SecretScope::Environment(_) => 0,
                                    code_secrets::SecretScope::Global => 1,
                                }
                            }

                            entries.sort_by(|left, right| {
                                scope_rank(&left.scope)
                                    .cmp(&scope_rank(&right.scope))
                                    .then_with(|| left.name.as_str().cmp(right.name.as_str()))
                            });

                            Ok::<_, anyhow::Error>(crate::app_event::SecretsListSnapshot { entries })
                        })
                        .await
                        .map_err(|err| err.to_string())
                        .and_then(|result| result.map_err(|err| err.to_string()));

                        tx.send(AppEvent::SecretsListLoaded { env_id, result });
                    });
                }
                AppEvent::SecretsListLoaded { env_id, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.secrets_apply_list_loaded(env_id, result);
                    }
                    self.schedule_redraw();
                }
                AppEvent::DeleteSecret { env_id, entry } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.secrets_mark_delete_in_progress(env_id.clone(), entry.clone());
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let code_home = self.config.code_home.clone();
                    tokio::spawn(async move {
                        let entry_for_delete = entry.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            let manager = code_secrets::SecretsManager::new(
                                code_home,
                                code_secrets::SecretsBackendKind::Local,
                            );
                            manager.delete(&entry_for_delete.scope, &entry_for_delete.name)
                        })
                        .await
                        .map_err(|err| err.to_string())
                        .and_then(|result| result.map_err(|err| err.to_string()));

                        tx.send(AppEvent::DeleteSecretFinished { env_id, entry, result });
                    });
                }
                AppEvent::DeleteSecretFinished { env_id, entry, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.secrets_apply_delete_finished(env_id.clone(), entry.clone(), result.clone());

                        match &result {
                            Ok(true) => {
                                widget.flash_footer_notice(format!(
                                    "Deleted secret: {}",
                                    entry.name.as_str(),
                                ));
                            }
                            Ok(false) => {
                                widget.flash_footer_notice(format!(
                                    "Secret not found: {}",
                                    entry.name.as_str(),
                                ));
                            }
                            Err(err) => {
                                widget.flash_footer_notice(format!(
                                    "Failed to delete secret {}: {err}",
                                    entry.name.as_str(),
                                ));
                            }
                        }
                    }

                    if result.is_ok() {
                        self.app_event_tx.send(AppEvent::FetchSecretsList { env_id });
                    }

                    self.schedule_redraw();
                }
                AppEvent::FetchAppsStatus {
                    account_ids,
                    force_refresh_tools,
                } => {
                    let should_compute = if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apps_mark_status_loading(&account_ids, force_refresh_tools);
                        if force_refresh_tools {
                            widget.submit_op(Op::RefreshMcpTools);
                            false
                        } else {
                            true
                        }
                    } else {
                        false
                    };
                    self.schedule_redraw();

                    if should_compute {
                        let results = if let AppState::Chat { widget } = &mut self.app_state {
                            account_ids
                                .iter()
                                .map(|id| {
                                    let (result, needs_login) =
                                        widget.apps_status_snapshot_for_account_id(id);
                                    (id.clone(), result, needs_login)
                                })
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };

                        for (account_id, result, needs_login) in results {
                            self.app_event_tx.send(AppEvent::AppsStatusLoaded {
                                account_id,
                                result,
                                needs_login,
                            });
                        }
                    }
                }
                AppEvent::AppsStatusLoaded {
                    account_id,
                    result,
                    needs_login,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apps_apply_status_loaded(account_id, result, needs_login);
                    }
                    self.schedule_redraw();
                }
                AppEvent::FetchAppsDirectory { force_refetch } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apps_directory_mark_loading(force_refetch);
                    }
                    self.schedule_redraw();

                    let tx = self.app_event_tx.clone();
                    let config = self.config.clone();
                    tokio::spawn(async move {
                        let result = code_chatgpt::connectors::list_all_connectors_with_options(
                            &config,
                            force_refetch,
                        )
                        .await
                        .map_err(|err| err.to_string());
                        tx.send(AppEvent::AppsDirectoryLoaded {
                            force_refetch,
                            result,
                        });
                    });
                }
                AppEvent::AppsDirectoryLoaded {
                    force_refetch,
                    result,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apps_directory_apply_loaded(force_refetch, result);
                    }
                    self.schedule_redraw();
                }
                AppEvent::ShowAppLinkView { params } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.open_app_link_view(params);
                    }
                    self.schedule_redraw();
                }
                AppEvent::OpenUrlInBrowser { url } => {
                    if let Err(err) = crate::open_url::open_url(&url) {
                        if let AppState::Chat { widget } = &mut self.app_state {
                            widget.debug_notice(format!(
                                "Failed to open browser: {err}. URL: {url}"
                            ));
                        }
                    }
                }
                AppEvent::InsertText { text } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.insert_str(&text);
                    }
                    self.schedule_redraw();
                }
                event => {
                    include!("accounts_and_auth_store.rs");
                }
            }
