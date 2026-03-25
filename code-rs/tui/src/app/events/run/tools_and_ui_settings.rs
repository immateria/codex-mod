            match event {
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
                        if force_remote_sync {
                            if let Err(err) = manager
                                .sync_plugins_from_remote(&config, auth.as_ref(), /*additive_only*/ false)
                                .await
                            {
                                remote_sync_error = Some(err.to_string());
                            }
                        }

                        let marketplaces = manager
                            .list_marketplaces_for_roots(&roots)
                            .map_err(|err| err.to_string());

                        let featured_plugin_ids = manager
                            .featured_plugin_ids_for_config(&config, auth.as_ref())
                            .await
                            .unwrap_or_else(|err| {
                                tracing::debug!("failed to fetch featured plugin ids: {err}");
                                Vec::new()
                            });

                        let result = marketplaces.map(|marketplaces| crate::app_event::PluginListSnapshot {
                            marketplaces,
                            remote_sync_error,
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
                                            .unwrap_or_else(|err| err.into_inner())
                                            .list
                                            .roots()
                                            .map(|roots| roots.to_vec());
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
                                            .unwrap_or_else(|err| err.into_inner())
                                            .list
                                            .roots()
                                            .map(|roots| roots.to_vec());
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
                                            .unwrap_or_else(|err| err.into_inner())
                                            .list
                                            .roots()
                                            .map(|roots| roots.to_vec());
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
                event => {
                    include!("accounts_and_auth_store.rs");
                }
            }
