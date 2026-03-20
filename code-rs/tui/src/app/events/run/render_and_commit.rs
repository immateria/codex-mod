            match event {
                AppEvent::AutoUpgradeCompleted { version } => match &mut self.app_state {
                    AppState::Chat { widget } => widget.on_auto_upgrade_completed(version),
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::RateLimitFetchFailed { message } => match &mut self.app_state {
                    AppState::Chat { widget } => widget.on_rate_limit_refresh_failed(message),
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::RateLimitSnapshotStored { account_id } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        widget.on_rate_limit_snapshot_stored(account_id)
                    }
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::RequestRedraw => {
                    self.schedule_redraw();
                }
                AppEvent::BottomPaneViewChanged => {
                    // Notify the height manager that the bottom pane view changed
                    // so it can bypass hysteresis and apply the new height immediately.
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_bottom_pane_view_changed();
                    }
                    self.schedule_redraw();
                }
                AppEvent::ModelPresetsUpdated { presets, default_model } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.update_model_presets(presets, default_model);
                    }
                    self.schedule_redraw();
                }
                AppEvent::UpdatePlanningUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_planning_use_chat_model(use_chat);
                    }
                    self.schedule_redraw();
                }
                AppEvent::FlushPendingExecEnds => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.flush_pending_exec_ends();
                    }
                    self.schedule_redraw();
                }
                AppEvent::SyncHistoryVirtualization => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.sync_history_virtualization();
                    }
                    self.schedule_redraw();
                }
                AppEvent::FlushInterruptsIfIdle => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.flush_interrupts_if_stream_idle();
                    }
                }
                AppEvent::Redraw => {
                    if self.timing_enabled { self.timing.on_redraw_begin(); }
                    let t0 = Instant::now();
                    let mut used_nonblocking = false;
                    let draw_result = if !tui::stdout_ready_for_writes() {
                        self.stdout_backpressure_skips = self.stdout_backpressure_skips.saturating_add(1);
                        if self.stdout_backpressure_skips == 1
                            || self.stdout_backpressure_skips.is_multiple_of(25)
                        {
                            tracing::warn!(
                                skips = self.stdout_backpressure_skips,
                                "stdout not writable; deferring redraw to avoid blocking"
                            );
                        }

                        if self.stdout_backpressure_skips < BACKPRESSURE_FORCED_DRAW_SKIPS {
                            self.redraw_inflight.store(false, Ordering::Release);
                            self.app_event_tx
                                .send(AppEvent::ScheduleFrameIn(Duration::from_millis(120)));
                            continue;
                        }

                        used_nonblocking = true;
                        tracing::warn!(
                            skips = self.stdout_backpressure_skips,
                            "stdout still blocked; forcing nonblocking redraw"
                        );
                        self.draw_frame_with_nonblocking_stdout(terminal)
                    } else {
                        self.stdout_backpressure_skips = 0;
                        std::io::stdout().sync_update(|_| self.draw_next_frame(terminal))
                    };

                    self.redraw_inflight.store(false, Ordering::Release);
                    let needs_follow_up = self.post_frame_redraw.swap(false, Ordering::AcqRel);
                    if needs_follow_up {
                        self.schedule_redraw();
                    }

                    match flatten_draw_result(draw_result) {
                        Ok(()) => {
                            self.stdout_backpressure_skips = 0;
                            if self.timing_enabled { self.timing.on_redraw_end(t0); }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // A draw can fail after partially writing to the terminal. In that case,
                            // the terminal contents may no longer match ratatui's back buffer, and
                            // subsequent diff-based draws may not fully repair stale tail lines.
                            // Force a clear on the next successful frame to resynchronize.
                            self.clear_on_first_frame = true;

                            // Also force the next successful draw to repaint the entire screen by
                            // invalidating ratatui's notion of the "current" buffer. This avoids
                            // cases where a partially-applied frame leaves stale glyphs visible but
                            // the back buffer thinks the terminal is already up to date.
                            terminal.swap_buffers();
                            // Non‑blocking draw hit backpressure; try again shortly.
                            if used_nonblocking {
                                tracing::debug!("nonblocking redraw hit WouldBlock; rescheduling");
                            }
                            self.app_event_tx
                                .send(AppEvent::ScheduleFrameIn(Duration::from_millis(120)));
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                AppEvent::StartCommitAnimation => {
                    if self
                        .commit_anim_running
                        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                        .is_ok()
                    {
                        let tx = self.app_event_tx.clone();
                        let running = self.commit_anim_running.clone();
                        let running_for_thread = running.clone();
                        let tick_ms: u64 = self
                            .config
                            .tui
                            .stream
                            .commit_tick_ms
                            .or(if self.config.tui.stream.responsive { Some(30) } else { None })
                            .unwrap_or(50);
                        if thread_spawner::spawn_lightweight("commit-anim", move || {
                            while running_for_thread.load(Ordering::Relaxed) {
                                thread::sleep(Duration::from_millis(tick_ms));
                                tx.send(AppEvent::CommitTick);
                            }
                        })
                        .is_none()
                        {
                            running.store(false, Ordering::Release);
                        }
                    }
                }
                AppEvent::StopCommitAnimation => {
                    self.commit_anim_running.store(false, Ordering::Release);
                }
                AppEvent::CommitTick => {
                    // Advance streaming animation: commit at most one queued line.
                    //
                    // Do not skip commit ticks when a redraw is already pending.
                    // Commit ticks are the *driver* for streaming output: skipping
                    // them can leave the UI appearing frozen even though input is
                    // still responsive.
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_commit_tick();
                    }
                }
                event => {
                    include!("input_and_exit.rs");
                }
            }
