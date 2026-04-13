            match event {
                AppEvent::InsertHistory(mut lines) => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        // Coalesce consecutive InsertHistory events to reduce redraw churn.
                        while let Ok(AppEvent::InsertHistory(mut more)) = self.app_event_rx_bulk.try_recv() {
                            lines.append(&mut more);
                        }
                        tracing::debug!("app: InsertHistory lines={}", lines.len());
                        if self.alt_screen_active {
                            widget.insert_history_lines(lines);
                        } else {
                            mirror_to_scrollback(terminal, widget, lines);
                            self.schedule_redraw();
                        }
                    },
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::InsertHistoryWithKind { id, kind, lines } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!("app: InsertHistoryWithKind kind={:?} id={:?} lines={}", kind, id, lines.len());
                        let to_mirror = lines.clone();
                        widget.insert_history_lines_with_kind(kind, id, lines);
                        if !self.alt_screen_active {
                            mirror_to_scrollback(terminal, widget, to_mirror);
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
                            mirror_to_scrollback(terminal, widget, to_mirror);
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
                    include!("render_and_commit.rs")
                }
            }
