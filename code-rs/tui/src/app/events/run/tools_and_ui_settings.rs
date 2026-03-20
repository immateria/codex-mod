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
                event => {
                    include!("accounts_and_auth_store.rs");
                }
            }
