            match event {
                AppEvent::StartFileSearch(query) => {
                    if !query.is_empty() {
                        self.file_search.on_user_query(query);
                    }
                }
                AppEvent::FileSearchResult { query, matches } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_file_search_result(query, matches);
                    }
                }
                #[cfg(feature = "browser-automation")]
                AppEvent::ShowChromeOptions(port) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_chrome_options(port);
                    }
                }
                #[cfg(feature = "browser-automation")]
                AppEvent::ChromeLaunchOptionSelected(option, port) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_chrome_launch_option(option, port);
                    }
                }
                AppEvent::JumpBack {
                    nth,
                    prefill,
                    history_snapshot,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        let ghost_state = widget.snapshot_ghost_state();
                        // Build response items from current UI history
                        let items = widget.export_response_items();
                        let cfg = widget.config_ref().clone();

                        // Compute prefix up to selected user message now
                        let prefix_items = {
                            let mut user_seen = 0usize;
                            let mut cut = items.len();
                            for (idx, it) in items.iter().enumerate().rev() {
                                if let code_protocol::models::ResponseItem::Message { role, .. } = it
                                    && role == "user" {
                                        user_seen += 1;
                                        if user_seen == nth { cut = idx; break; }
                                    }
                            }
                            items.iter().take(cut).cloned().collect::<Vec<_>>()
                        };

                        self.pending_jump_back_ghost_state = Some(ghost_state);
                        self.pending_jump_back_history_snapshot = history_snapshot;

                        // Perform the fork off the UI thread to avoid nested runtimes
                        let server = self._server.clone();
                        let tx = self.app_event_tx.clone();
                        let prefill_clone = prefill.clone();
                        if let Err(err) = std::thread::Builder::new()
                            .name("jump-back-fork".to_string())
                            .spawn(move || {
                                let rt = tokio::runtime::Builder::new_multi_thread()
                                    .enable_all()
                                    .build()
                                    .unwrap_or_else(|err| {
                                        panic!("build tokio runtime: {err}")
                                    });
                                // Clone cfg for the async block to keep original for the event
                                let cfg_for_rt = cfg.clone();
                                let result = rt.block_on(async move {
                                    // Fallback: start a new conversation instead of forking
                                    server.new_conversation(cfg_for_rt).await
                                });
                                if let Ok(new_conv) = result {
                                    tx.send(AppEvent::JumpBackForked { cfg, new_conv: crate::app_event::Redacted(new_conv), prefix_items, prefill: prefill_clone });
                                } else if let Err(e) = result {
                                    tracing::error!("error forking conversation: {e:#}");
                                }
                            })
                        {
                            tracing::error!("jump-back fork spawn failed: {err}");
                        }
                    }
                }
                AppEvent::JumpBackForked { cfg, new_conv, prefix_items, prefill } => {
                    // Replace widget with a new one bound to the forked conversation
                    let session_conf = new_conv.0.session_configured.clone();
                    let conv = new_conv.0.conversation.clone();

                    let mut ghost_state = self.pending_jump_back_ghost_state.take();
                    let history_snapshot = self.pending_jump_back_history_snapshot.take();
                    let emit_prefix = history_snapshot.is_none();

                    if let AppState::Chat { widget } = &mut self.app_state {
                        let auth_manager = widget.auth_manager();
                        let mut new_widget =
                            ChatWidget::new_from_existing(crate::chatwidget::ForkedChatWidgetInit {
                                config: cfg,
                                conversation: conv,
                                session_configured: session_conf,
                                app_event_tx: self.app_event_tx.clone(),
                                terminal_info: self.terminal_info.clone(),
                                show_order_overlay: self.show_order_overlay,
                                latest_upgrade_version: self.latest_upgrade_version.clone(),
                                auth_manager,
                                show_welcome: false,
                            });
                        if let Some(state) = ghost_state.take() {
                            new_widget.adopt_ghost_state(state);
                        } else {
                            tracing::warn!("jump-back fork missing ghost snapshot state; redo may be unavailable");
                        }
                        if let Some(snapshot) = history_snapshot.as_ref() {
                            new_widget.restore_history_snapshot(snapshot);
                        }
                        new_widget.enable_perf(self.timing_enabled);
                        new_widget.check_for_initial_animations();
                        **widget = new_widget;
                    } else {
                        let auth_manager = AuthManager::shared_with_mode_and_originator(
                            cfg.code_home.clone(),
                            AuthMode::ApiKey,
                            cfg.responses_originator_header.clone(),
                            cfg.cli_auth_credentials_store_mode,
                        );
                        let mut new_widget =
                            ChatWidget::new_from_existing(crate::chatwidget::ForkedChatWidgetInit {
                                config: cfg,
                                conversation: conv,
                                session_configured: session_conf,
                                app_event_tx: self.app_event_tx.clone(),
                                terminal_info: self.terminal_info.clone(),
                                show_order_overlay: self.show_order_overlay,
                                latest_upgrade_version: self.latest_upgrade_version.clone(),
                                auth_manager,
                                show_welcome: false,
                            });
                        if let Some(state) = ghost_state.take() {
                            new_widget.adopt_ghost_state(state);
                        }
                        if let Some(snapshot) = history_snapshot.as_ref() {
                            new_widget.restore_history_snapshot(snapshot);
                        }
                        new_widget.enable_perf(self.timing_enabled);
                        new_widget.check_for_initial_animations();
                        self.app_state = AppState::Chat { widget: Box::new(new_widget) };
                    }
                    self.terminal_runs.clear();
                    // Reset any transient state from the previous widget/session
                    self.commit_anim_running.store(false, Ordering::Release);
                    self.last_esc_time = None;
                    // Force a clean repaint of the new UI state
                    self.clear_on_first_frame = true;

                    // Replay prefix to the UI
                    if emit_prefix {
                        let ev = code_core::protocol::Event {
                            id: "fork".to_string(),
                            event_seq: 0,
                            msg: code_core::protocol::EventMsg::ReplayHistory(
                                code_core::protocol::ReplayHistoryEvent {
                                    items: prefix_items,
                                    history_snapshot: None,
                                }
                            ),
                            order: None,
                        };
                        self.app_event_tx.send(AppEvent::codex_event(ev));
                    }

                    // Prefill composer with the edited text
                    if let AppState::Chat { widget } = &mut self.app_state
                        && !prefill.is_empty() { widget.insert_str(&prefill); }
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                }
                AppEvent::ScheduleFrameIn(duration) => {
                    // Schedule the next redraw with the requested duration
                    self.schedule_redraw_in(duration);
                }
                AppEvent::GhostSnapshotFinished { job_id, result, elapsed } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_ghost_snapshot_finished(job_id, result, elapsed);
                    }
                }
                AppEvent::AutoReviewBaselineCaptured { turn_sequence, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_auto_review_baseline_captured(turn_sequence, result);
                    }
                }
                event => {
                    unreachable!("unhandled AppEvent in App::run match split: {event:?}");
                }
            }
