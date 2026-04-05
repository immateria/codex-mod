            match event {
                AppEvent::SetThemeSplitPreview { current, preview } => {
                    let next = ThemeSplitPreview { current, preview };
                    let unchanged = self
                        .theme_split_preview
                        .map(|existing| {
                            existing.current == next.current && existing.preview == next.preview
                        })
                        .unwrap_or(false);
                    if !unchanged {
                        self.theme_split_preview = Some(next);
                        self.schedule_redraw();
                    }
                }
                AppEvent::ClearThemeSplitPreview => {
                    if self.theme_split_preview.take().is_some() {
                        self.schedule_redraw();
                    }
                }
                AppEvent::UpdateTheme(new_theme) => {
                    // Switch the theme immediately
                    if matches!(new_theme, code_core::config_types::ThemeName::Custom) {
                        // Prefer runtime custom colors; fall back to config on disk
                        if let Some(colors) = crate::theme::custom_theme_colors() {
                            crate::theme::init_theme(&code_core::config_types::ThemeConfig { name: new_theme, colors, label: crate::theme::custom_theme_label(), is_dark: crate::theme::custom_theme_is_dark() });
                        } else if let Ok(cfg) = code_core::config::Config::load_with_cli_overrides(vec![], code_core::config::ConfigOverrides::default()) {
                            crate::theme::init_theme(&cfg.tui.theme);
                        } else {
                            crate::theme::switch_theme(new_theme);
                        }
                    } else {
                        crate::theme::switch_theme(new_theme);
                    }

                    // Clear terminal with new theme colors
                    let theme_bg = crate::colors::background();
                    let theme_fg = crate::colors::text();
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::style::SetColors(crossterm::style::Colors::new(
                            theme_fg.into(),
                            theme_bg.into()
                        )),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        crossterm::cursor::MoveTo(0, 0),
                        crossterm::terminal::EnableLineWrap
                    );
                    self.apply_terminal_title();

                    // Update config and save to file
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_theme(new_theme);
                    }

                    // Force a full redraw on the next frame so the entire
                    // ratatui back buffer is cleared and repainted with the
                    // new theme. This avoids any stale cells lingering on
                    // terminals that preserve previous cell attributes.
                    self.clear_on_first_frame = true;
                    self.schedule_redraw();
                }
                AppEvent::PreviewTheme(new_theme) => {
                    // Switch the theme immediately for preview (no history event)
                    if matches!(new_theme, code_core::config_types::ThemeName::Custom) {
                        if let Some(colors) = crate::theme::custom_theme_colors() {
                            crate::theme::init_theme(&code_core::config_types::ThemeConfig { name: new_theme, colors, label: crate::theme::custom_theme_label(), is_dark: crate::theme::custom_theme_is_dark() });
                        } else if let Ok(cfg) = code_core::config::Config::load_with_cli_overrides(vec![], code_core::config::ConfigOverrides::default()) {
                            crate::theme::init_theme(&cfg.tui.theme);
                        } else {
                            crate::theme::switch_theme(new_theme);
                        }
                    } else {
                        crate::theme::switch_theme(new_theme);
                    }

                    // Clear terminal with new theme colors
                    let theme_bg = crate::colors::background();
                    let theme_fg = crate::colors::text();
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::style::SetColors(crossterm::style::Colors::new(
                            theme_fg.into(),
                            theme_bg.into()
                        )),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        crossterm::cursor::MoveTo(0, 0),
                        crossterm::terminal::EnableLineWrap
                    );
                    self.apply_terminal_title();

                    // Retint pre-rendered history cells so the preview reflects immediately
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.retint_history_for_preview();
                    }

                    // Don't update config or add to history for previews
                    // Force a full redraw so previews repaint cleanly as you cycle
                    self.clear_on_first_frame = true;
                    self.schedule_redraw();
                }
                AppEvent::UpdateSpinner(name) => {
                    // Switch spinner immediately
                    crate::spinner::switch_spinner(&name);
                    // Update config and save to file
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_spinner(name.clone());
                    }
                    self.schedule_redraw();
                }
                AppEvent::PreviewSpinner(name) => {
                    // Switch spinner immediately for preview (no history event)
                    crate::spinner::switch_spinner(&name);
                    // No config change on preview
                    self.schedule_redraw();
                }
                AppEvent::ComposerExpanded => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_composer_expanded();
                    }
                    self.schedule_redraw();
                }
                AppEvent::ShowLoginAccounts => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_login_accounts_view();
                    }
                }
                AppEvent::ShowLoginAddAccount => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_login_add_account_view();
                    }
                }
                AppEvent::CycleAccessMode => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.cycle_access_mode();
                    }
                    self.schedule_redraw();
                }
                AppEvent::CycleAutoDriveVariant => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.cycle_auto_drive_variant();
                    }
                    self.schedule_redraw();
                }
                AppEvent::LoginStartChatGpt => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if !widget.login_add_view_active() {
                            continue 'main;
                        }

                        if let Some(flow) = self.login_flow.take() {
                            if let Some(shutdown) = flow.shutdown {
                                shutdown.shutdown();
                            }
                            flow.join_handle.abort();
                        }

                        let opts = ServerOptions::new(
                            self.config.code_home.clone(),
                            code_login::CLIENT_ID.to_string(),
                            self.config.responses_originator_header.clone(),
                            self.config.cli_auth_credentials_store_mode,
                        );

                        match code_login::run_login_server(opts) {
                            Ok(server) => {
                                widget.notify_login_chatgpt_started(server.auth_url.clone());
                                let shutdown = server.cancel_handle();
                                let tx = self.app_event_tx.clone();
                                let join_handle = tokio::spawn(async move {
                                    let result = server
                                        .block_until_done()
                                        .await
                                        .map_err(|e| e.to_string());
                                    tx.send(AppEvent::LoginChatGptComplete { result });
                                });
                                self.login_flow = Some(LoginFlowState {
                                    shutdown: Some(shutdown),
                                    join_handle,
                                });
                            }
                            Err(err) => {
                                widget.notify_login_chatgpt_failed(format!(
                                    "Failed to start ChatGPT login: {err}"
                                ));
                            }
                        }
                    }
                }
                AppEvent::LoginStartDeviceCode => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if !widget.login_add_view_active() {
                            continue 'main;
                        }

                        if let Some(flow) = self.login_flow.take() {
                            if let Some(shutdown) = flow.shutdown {
                                shutdown.shutdown();
                            }
                            flow.join_handle.abort();
                        }
                        widget.notify_login_device_code_pending();

                        let opts = ServerOptions::new(
                            self.config.code_home.clone(),
                            code_login::CLIENT_ID.to_string(),
                            self.config.responses_originator_header.clone(),
                            self.config.cli_auth_credentials_store_mode,
                        );
                        let tx = self.app_event_tx.clone();
                        let join_handle = tokio::spawn(async move {
                            match code_login::DeviceCodeSession::start(opts).await {
                                Ok(session) => {
                                    let authorize_url = session.authorize_url();
                                    let user_code = session.user_code().to_string();
                                    tx.send(AppEvent::LoginDeviceCodeReady { authorize_url, user_code });
                                    let result = session.wait_for_tokens().await.map_err(|e| e.to_string());
                                    tx.send(AppEvent::LoginDeviceCodeComplete { result });
                                }
                                Err(err) => {
                                    tx.send(AppEvent::LoginDeviceCodeFailed { message: err.to_string() });
                                }
                            }
                        });
                        self.login_flow = Some(LoginFlowState { shutdown: None, join_handle });
                    }
                }
                AppEvent::LoginCancelChatGpt => {
                    if let Some(flow) = self.login_flow.take() {
                        if let Some(shutdown) = flow.shutdown {
                            shutdown.shutdown();
                        }
                        flow.join_handle.abort();
                    }
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_flow_cancelled();
                    }
                }
                AppEvent::LoginChatGptComplete { result } => {
                    if let Some(flow) = self.login_flow.take() {
                        if let Some(shutdown) = flow.shutdown {
                            shutdown.shutdown();
                        }
                        // Allow the task to finish naturally; if still running, abort.
                        if !flow.join_handle.is_finished() {
                            flow.join_handle.abort();
                        }
                    }

                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_chatgpt_complete(result);
                    }
                }
                AppEvent::LoginDeviceCodeReady { authorize_url, user_code } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_device_code_ready(authorize_url, user_code);
                    }
                }
                AppEvent::LoginDeviceCodeFailed { message } => {
                    if let Some(flow) = self.login_flow.take() {
                        if let Some(shutdown) = flow.shutdown {
                            shutdown.shutdown();
                        }
                        if !flow.join_handle.is_finished() {
                            flow.join_handle.abort();
                        }
                    }
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_device_code_failed(message);
                    }
                }
                AppEvent::LoginDeviceCodeComplete { result } => {
                    if let Some(flow) = self.login_flow.take() {
                        if let Some(shutdown) = flow.shutdown {
                            shutdown.shutdown();
                        }
                        if !flow.join_handle.is_finished() {
                            flow.join_handle.abort();
                        }
                    }

                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_device_code_complete(result);
                    }
                }
                AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth } => {
                    self.handle_login_mode_change(using_chatgpt_auth);
                }
                AppEvent::OnboardingAuthComplete(result) => {
                    if let AppState::Onboarding { screen } = &mut self.app_state {
                        screen.on_auth_complete(result);
                    }
                }
                AppEvent::OnboardingComplete(ChatWidgetArgs {
                    config,
                    initial_images,
                    initial_prompt,
                    terminal_info,
                    show_order_overlay,
                    enable_perf,
                    resume_picker,
                    fork_picker,
                    fork_source_path,
                    latest_upgrade_version,
                }) => {
                    let mut w = ChatWidget::new(crate::chatwidget::ChatWidgetInit {
                        config,
                        app_event_tx: app_event_tx.clone(),
                        initial_prompt,
                        initial_images,
                        terminal_info,
                        show_order_overlay,
                        latest_upgrade_version,
                    });
                    w.enable_perf(enable_perf);
                    if resume_picker {
                        w.show_resume_picker();
                    }
                    if fork_picker {
                        w.show_fork_picker();
                    }
                    self.app_state = AppState::Chat { widget: Box::new(w) };
                    self.terminal_runs.clear();
                    if let Some(path) = fork_source_path {
                        self.app_event_tx.send(AppEvent::ForkFrom(path));
                    }
                }
                event => {
                    include!("file_search_chrome_jump_back_tail.rs");
                }
            }
