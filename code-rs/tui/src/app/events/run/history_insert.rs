            match event {
                AppEvent::InsertHistory(mut lines) => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        // Coalesce consecutive InsertHistory events to reduce redraw churn.
                        while let Ok(AppEvent::InsertHistory(mut more)) = self.app_event_rx_bulk.try_recv() {
                            lines.append(&mut more);
                        }
                        tracing::debug!("app: InsertHistory lines={}", lines.len());
                        if self.alt_screen_active {
                            widget.insert_history_lines(lines)
                        } else {
                            use std::io::stdout;
                            // Compute desired bottom height now, so growing/shrinking input
                            // adjusts the reserved region immediately even before the next frame.
                            let width = terminal.size().map(|s| s.width).unwrap_or(80);
                            let reserve = widget.desired_bottom_height(width).max(1);
                            let _ = execute!(stdout(), crossterm::terminal::BeginSynchronizedUpdate);
                            crate::insert_history::insert_history_lines_above(terminal, reserve, lines);
                            let _ = execute!(stdout(), crossterm::terminal::EndSynchronizedUpdate);
                            self.schedule_redraw();
                        }
                    },
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::InsertHistoryWithKind { id, kind, lines } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!("app: InsertHistoryWithKind kind={:?} id={:?} lines={}", kind, id, lines.len());
                        // Always update widget history, even in terminal mode.
                        // In terminal mode, the widget will emit an InsertHistory event
                        // which we will mirror to scrollback in the handler above.
                        let to_mirror = lines.clone();
                        widget.insert_history_lines_with_kind(kind, id, lines);
                        if !self.alt_screen_active {
                            use std::io::stdout;
                            let width = terminal.size().map(|s| s.width).unwrap_or(80);
                            let reserve = widget.desired_bottom_height(width).max(1);
                            let _ = execute!(stdout(), crossterm::terminal::BeginSynchronizedUpdate);
                            crate::insert_history::insert_history_lines_above(terminal, reserve, to_mirror);
                            let _ = execute!(stdout(), crossterm::terminal::EndSynchronizedUpdate);
                            self.schedule_redraw();
                        }
                    },
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::InsertFinalAnswer { id, lines, source } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!("app: InsertFinalAnswer id={:?} lines={} source_len={}", id, lines.len(), source.len());
                        let to_mirror = lines.clone();
                        widget.insert_final_answer_with_id(id, lines, source);
                        if !self.alt_screen_active {
                            use std::io::stdout;
                            let width = terminal.size().map(|s| s.width).unwrap_or(80);
                            let reserve = widget.desired_bottom_height(width).max(1);
                            let _ = execute!(stdout(), crossterm::terminal::BeginSynchronizedUpdate);
                            crate::insert_history::insert_history_lines_above(terminal, reserve, to_mirror);
                            let _ = execute!(stdout(), crossterm::terminal::EndSynchronizedUpdate);
                            self.schedule_redraw();
                        }
                    },
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::InsertBackgroundEvent { message, placement, order } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!(
                            "app: InsertBackgroundEvent placement={:?} len={}",
                            placement,
                            message.len()
                        );
                        widget.insert_background_event_with_placement(message, placement, order);
                    }
                    AppState::Onboarding { .. } => {}
                },
                event => {
                    include!("render_and_commit.rs");
                }
            }
