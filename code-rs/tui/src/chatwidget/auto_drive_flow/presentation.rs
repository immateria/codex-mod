use super::*;

impl ChatWidget<'_> {
    pub(crate) fn auto_rebuild_live_ring(&mut self) {
        if !self.auto_state.is_active() {
            if self.auto_state.should_show_goal_entry() {
                self.auto_show_goal_entry_panel();
                return;
            }
            if let Some(summary) = self.auto_state.last_run_summary.clone() {
                self.bottom_pane.clear_live_ring();
                self.auto_reset_intro_timing();
                self.auto_ensure_intro_timing();
                let mut status_lines: Vec<String> = Vec::new();
                if let Some(msg) = summary.message.as_ref() {
                    let trimmed = msg.trim();
                    if !trimmed.is_empty() {
                        status_lines.push(trimmed.to_string());
                    }
                }
                if status_lines.is_empty() {
                    if let Some(goal) = summary.goal.as_ref() {
                        status_lines.push(format!("Auto Drive completed: {goal}"));
                    } else {
                        status_lines.push("Auto Drive completed.".to_string());
                    }
                }
                let model = AutoCoordinatorViewModel::Active(AutoActiveViewModel {
                    goal: summary.goal.clone(),
                    status_lines,
                    cli_prompt: None,
                    cli_context: None,
                    show_composer: true,
            awaiting_submission: false,
            waiting_for_response: false,
            coordinator_waiting: false,
            waiting_for_review: false,
                    countdown: None,
                    button: None,
                    manual_hint: None,
                    ctrl_switch_hint: "Esc to exit Auto Drive".to_string(),
                    cli_running: false,
                    turns_completed: summary.turns_completed,
                    started_at: None,
                    elapsed: Some(summary.duration),
                    status_sent_to_user: None,
                    status_title: None,
                    session_tokens: self.auto_session_tokens(),
                    editing_prompt: false,
                    intro_started_at: self.auto_state.intro_started_at,
                    intro_reduced_motion: self.auto_state.intro_reduced_motion,
                });
            self
                .bottom_pane
                .show_auto_coordinator_view(model);
            self.bottom_pane.release_auto_drive_style();
            self.bottom_pane.set_standard_terminal_hint(None);
            return;
        }

        self.bottom_pane.clear_auto_coordinator_view(true);
        self.bottom_pane.clear_live_ring();
        self.bottom_pane.set_standard_terminal_hint(None);
        self.auto_reset_intro_timing();
        return;
    }

    // AutoDrive is active: if intro animation was mid-flight, force reduced motion
    // so a rebuild cannot leave the header half-rendered (issue #431).
    if self.auto_state.intro_started_at.is_some() && !self.auto_state.intro_reduced_motion {
        self.auto_state.intro_reduced_motion = true;
    }

    if self.auto_state.is_paused_manual() {
        self.bottom_pane.clear_auto_coordinator_view(false);
        self.bottom_pane.clear_live_ring();
        self.bottom_pane.set_standard_terminal_hint(None);
        return;
    }

        self.bottom_pane.clear_live_ring();

        let status_text = if self.auto_state.awaiting_review() {
            "waiting for code review...".to_string()
        } else if let Some(line) = self
            .auto_state
            .current_display_line
            .as_ref()
            .filter(|line| !line.trim().is_empty())
        {
            line.clone()
        } else {
            self
                .auto_state
                .placeholder_phrase
                .get_or_insert_with(|| auto_drive_strings::next_auto_drive_phrase().to_string())
                .clone()
        };

        let headline = self.auto_format_status_headline(&status_text);
        let mut status_lines = vec![headline];
        if !self.auto_state.awaiting_review() {
            self.auto_append_status_lines(
                &mut status_lines,
                self.auto_state.current_status_title.as_ref(),
                self.auto_state.current_status_sent_to_user.as_ref(),
            );
            if self.auto_state.is_waiting_for_response() && !self.auto_state.is_coordinator_waiting() {
                let appended = self.auto_append_status_lines(
                    &mut status_lines,
                    self.auto_state.last_decision_status_title.as_ref(),
                    self.auto_state.last_decision_status_sent_to_user.as_ref(),
                );
                if !appended
                    && let Some(summary) = self.auto_state.last_decision_summary.as_ref() {
                        let trimmed = summary.trim();
                        if !trimmed.is_empty() {
                            let collapsed = trimmed
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ");
                            if !collapsed.is_empty() {
                                let current_line = status_lines
                                    .first()
                                    .map(|line| line.trim_end_matches('…').trim())
                                    .unwrap_or("");
                                if collapsed != current_line {
                                    let display = Self::truncate_with_ellipsis(&collapsed, 160);
                                    status_lines.push(display);
                                }
                            }
                        }
                    }
            }
        }
        let cli_running = self.is_cli_running();
        let progress_hint_active = self.auto_state.awaiting_coordinator_submit()
            || (self.auto_state.is_waiting_for_response() && !self.auto_state.is_coordinator_waiting())
            || cli_running;

        // Keep the most recent coordinator status visible across approval and
        // CLI execution. The coordinator clears the current status fields once it
        // starts streaming the next turn, so fall back to the last decision while
        // we are still acting on it.
        let status_title_for_view = if progress_hint_active {
            self.auto_state
                .current_status_title
                .clone()
                .or_else(|| self.auto_state.last_decision_status_title.clone())
        } else {
            None
        };
        let status_sent_to_user_for_view = if progress_hint_active {
            self.auto_state
                .current_status_sent_to_user
                .clone()
                .or_else(|| self.auto_state.last_decision_status_sent_to_user.clone())
        } else {
            None
        };

        let cli_prompt = self
            .auto_state
            .current_cli_prompt
            .clone()
            .filter(|p| !p.trim().is_empty());
        let cli_context = if self.auto_state.hide_cli_context_in_ui {
            None
        } else {
            self.auto_state
                .current_cli_context
                .clone()
                .filter(|value| !value.trim().is_empty())
        };
        let has_cli_prompt = cli_prompt.is_some();

        let bootstrap_pending = self.auto_pending_goal_request;
        let continue_cta_active = self.auto_should_show_continue_cta();

        let countdown_limit = self.auto_state.countdown_seconds();
        let countdown_active = self.auto_state.countdown_active();
        let countdown = if self.auto_state.awaiting_coordinator_submit() {
            match countdown_limit {
                Some(limit) if limit > 0 => Some(CountdownState {
                    remaining: self.auto_state.seconds_remaining.min(limit),
                }),
                _ => None,
            }
        } else {
            None
        };

        let button = if self.auto_state.awaiting_coordinator_submit() {
            let base_label = if bootstrap_pending {
                "Complete Current Task"
            } else if has_cli_prompt {
                "Send prompt"
            } else if continue_cta_active {
                "Continue current task"
            } else {
                "Send prompt"
            };
            let label = if countdown_active {
                format!("{base_label} ({}s)", self.auto_state.seconds_remaining)
            } else {
                base_label.to_string()
            };
            Some(AutoCoordinatorButton {
                label,
                enabled: true,
            })
        } else {
            None
        };

        let manual_hint = if self.auto_state.awaiting_coordinator_submit() {
            if self.auto_state.is_paused_manual() {
                Some("Edit the prompt, then press Enter to continue.".to_string())
            } else if bootstrap_pending {
                None
            } else if has_cli_prompt {
                if countdown_active {
                    Some("Enter to send now • Esc to edit".to_string())
                } else {
                    Some("Enter to send • Esc to edit".to_string())
                }
            } else if continue_cta_active {
                if countdown_active {
                    Some("Enter to continue now • Esc to stop".to_string())
                } else {
                    Some("Enter to continue • Esc to stop".to_string())
                }
            } else if countdown_active {
                Some("Enter to send now • Esc to stop".to_string())
            } else {
                Some("Enter to send • Esc to stop".to_string())
            }
        } else {
            None
        };

        let ctrl_switch_hint = if self.auto_state.awaiting_coordinator_submit() {
            if self.auto_state.is_paused_manual() {
                "Esc to cancel".to_string()
            } else if bootstrap_pending {
                "Esc enter new goal".to_string()
            } else if has_cli_prompt {
                "Esc to edit".to_string()
            } else {
                "Esc to stop".to_string()
            }
        } else {
            String::new()
        };

        let show_composer =
            !self.auto_state.awaiting_coordinator_submit() || self.auto_state.is_paused_manual();

        let model = AutoCoordinatorViewModel::Active(AutoActiveViewModel {
            goal: self.auto_state.goal.clone(),
            status_lines,
            cli_prompt,
            awaiting_submission: self.auto_state.awaiting_coordinator_submit(),
            waiting_for_response: self.auto_state.is_waiting_for_response(),
            coordinator_waiting: self.auto_state.is_coordinator_waiting(),
            waiting_for_review: self.auto_state.awaiting_review(),
            countdown,
            button,
            manual_hint,
            ctrl_switch_hint,
            cli_running,
            turns_completed: self.auto_state.turns_completed,
            started_at: self.auto_state.started_at,
            elapsed: self.auto_state.elapsed_override,
            status_sent_to_user: status_sent_to_user_for_view,
            status_title: status_title_for_view,
            session_tokens: self.auto_session_tokens(),
            cli_context,
            show_composer,
            editing_prompt: self.auto_state.is_paused_manual(),
            intro_started_at: self.auto_state.intro_started_at,
            intro_reduced_motion: self.auto_state.intro_reduced_motion,
        });

        self
            .bottom_pane
            .show_auto_coordinator_view(model);

        self.auto_update_terminal_hint();

        if self.auto_state.started_at.is_some() {
            self.app_event_tx
                .send(AppEvent::ScheduleFrameIn(Duration::from_secs(1)));
        }
    }

    pub(crate) fn auto_should_show_continue_cta(&self) -> bool {
        self.auto_state.is_active()
            && self.auto_state.awaiting_coordinator_submit()
            && !self.auto_state.is_paused_manual()
            && self.config.auto_drive.coordinator_routing
            && self.auto_state.continue_mode != AutoContinueMode::Manual
    }

    pub(crate) fn auto_format_status_headline(&self, text: &str) -> String {
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            return String::new();
        }

        if self.auto_state.current_display_is_summary {
            return trimmed.to_string();
        }

        let show_summary_without_ellipsis = self.auto_state.awaiting_coordinator_submit()
            && self.auto_state.current_reasoning_title.is_none()
            && self
                .auto_state
                .current_summary
                .as_ref()
                .map(|summary| !summary.trim().is_empty())
                .unwrap_or(false);

        if show_summary_without_ellipsis {
            trimmed.to_string()
        } else {
            append_thought_ellipsis(trimmed)
        }
    }

    pub(crate) fn auto_update_terminal_hint(&mut self) {
        if !self.auto_state.is_active() && !self.auto_state.should_show_goal_entry() {
            self.bottom_pane.set_standard_terminal_hint(None);
            return;
        }

        let agents_label = if self.auto_state.subagents_enabled {
            "Agents Enabled"
        } else {
            "Agents Disabled"
        };
        let diagnostics_enabled = self.auto_state.qa_automation_enabled
            && (self.auto_state.review_enabled || self.auto_state.cross_check_enabled);
        let diagnostics_label = if diagnostics_enabled {
            "Diagnostics Enabled"
        } else {
            "Diagnostics Disabled"
        };

        let left = format!("• {agents_label}  • {diagnostics_label}");

        let hint = left;
        self.bottom_pane
            .set_standard_terminal_hint(Some(hint));
    }

    pub(crate) fn auto_update_display_title(&mut self) {
        if !self.auto_state.is_active() {
            return;
        }

        let Some(summary) = self.auto_state.current_summary.as_ref() else {
            return;
        };

        let display = summary.lines().find_map(|line| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then(|| Self::truncate_with_ellipsis(trimmed, 160))
        });

        let Some(display) = display else {
            return;
        };

        let needs_update = self
            .auto_state
            .current_display_line
            .as_ref()
            .map(|current| current != &display)
            .unwrap_or(true);

        if needs_update {
            self.auto_state.current_display_line = Some(display);
            self.auto_state.current_display_is_summary = true;
            self.auto_state.placeholder_phrase = None;
            self.auto_state.current_reasoning_title = None;
        }
    }

    pub(crate) fn auto_broadcast_summary(&mut self, raw: &str) {
        if !self.auto_state.is_active() {
            return;
        }

        let display_text = extract_latest_bold_title(raw).or_else(|| {
            raw.lines().find_map(|line| {
                let trimmed = line.trim();
                (!trimmed.is_empty()).then_some(trimmed.to_string())
            })
        });

        let Some(display_text) = display_text else {
            return;
        };

        if self
            .auto_state
            .last_broadcast_summary
            .as_ref()
            .map(|prev| prev == &display_text)
            .unwrap_or(false)
        {
            return;
        }

        self.auto_state.last_broadcast_summary = Some(display_text);
    }

    pub(crate) fn auto_on_reasoning_delta(&mut self, delta: &str, summary_index: Option<u32>) {
        if !self.auto_state.is_active() || delta.trim().is_empty() {
            return;
        }

        let mut needs_refresh = false;

        if let Some(idx) = summary_index
            && self.auto_state.current_summary_index != Some(idx) {
                self.auto_state.current_summary_index = Some(idx);
                self.auto_state.current_summary = Some(String::new());
                self.auto_state.thinking_prefix_stripped = false;
                self.auto_state.current_reasoning_title = None;
                self.auto_state.current_display_line = None;
                self.auto_state.current_display_is_summary = false;
                self.auto_state.placeholder_phrase =
                    Some(auto_drive_strings::next_auto_drive_phrase().to_string());
                needs_refresh = true;
            }

        let cleaned_delta = if !self.auto_state.thinking_prefix_stripped {
            let (without_prefix, stripped) = strip_role_prefix_if_present(delta);
            if stripped {
                self.auto_state.thinking_prefix_stripped = true;
            }
            without_prefix.to_string()
        } else {
            delta.to_string()
        };

        if !self.auto_state.thinking_prefix_stripped && !cleaned_delta.trim().is_empty() {
            self.auto_state.thinking_prefix_stripped = true;
        }

        {
            let entry = self
                .auto_state
                .current_summary
                .get_or_insert_with(String::new);

            if auto_drive_strings::is_auto_drive_phrase(entry) {
                entry.clear();
            }

            entry.push_str(&cleaned_delta);

            let mut display_updated = false;

            if let Some(title) = extract_latest_bold_title(entry) {
                let needs_update = self
                    .auto_state
                    .current_reasoning_title
                    .as_ref()
                    .map(|existing| existing != &title)
                    .unwrap_or(true);
                if needs_update {
                    self.auto_state.current_reasoning_title = Some(title.clone());
                    self.auto_state.current_display_line = Some(title);
                    self.auto_state.current_display_is_summary = false;
                    self.auto_state.placeholder_phrase = None;
                    display_updated = true;
                }
            } else if self.auto_state.current_reasoning_title.is_none() {
                let previous_line = self.auto_state.current_display_line.clone();
                let previous_is_summary = self.auto_state.current_display_is_summary;
                self.auto_update_display_title();
                let updated_line = self.auto_state.current_display_line.clone();
                let updated_is_summary = self.auto_state.current_display_is_summary;
                if updated_is_summary
                    && (updated_line != previous_line || !previous_is_summary)
                {
                    display_updated = true;
                }
            }

            if display_updated {
                needs_refresh = true;
            }
        }

        if needs_refresh {
            self.auto_rebuild_live_ring();
            self.request_redraw();
        }
    }

    pub(crate) fn auto_on_reasoning_final(&mut self, text: &str) {
        if !self.auto_state.is_active() {
            return;
        }

        self.auto_state.current_reasoning_title = None;
        self.auto_state.current_summary = Some(text.to_string());
        self.auto_state.thinking_prefix_stripped = true;
        self.auto_state.current_summary_index = None;
        self.auto_update_display_title();
        self.auto_broadcast_summary(text);

        if self.auto_state.is_waiting_for_response() {
            self.auto_rebuild_live_ring();
            self.request_redraw();
        }
    }

    pub(crate) fn truncate_with_ellipsis(text: &str, max_chars: usize) -> String {
        crate::text_formatting::truncate_chars_with_ellipsis(text, max_chars)
    }

    pub(crate) fn normalize_status_field(field: Option<String>) -> Option<String> {
        field.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    pub(crate) fn compose_status_summary(
        status_title: &Option<String>,
        status_sent_to_user: &Option<String>,
    ) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(title) = status_title
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            parts.push(title.to_string());
        }
        if let Some(sent) = status_sent_to_user
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            && !parts.iter().any(|existing| existing.eq_ignore_ascii_case(sent)) {
                parts.push(sent.to_string());
            }

        match parts.len() {
            0 => String::new(),
            1 => parts.into_iter().next().unwrap_or_default(),
            _ => parts.join(" · "),
        }
    }

    pub(crate) fn auto_append_status_lines(
        &self,
        lines: &mut Vec<String>,
        status_title: Option<&String>,
        status_sent_to_user: Option<&String>,
    ) -> bool {
        let initial_len = lines.len();
        Self::append_status_line(lines, status_title);
        Self::append_status_line(lines, status_sent_to_user);
        lines.len() > initial_len
    }

    pub(crate) fn append_status_line(lines: &mut Vec<String>, status: Option<&String>) {
        if let Some(status) = status {
            let trimmed = status.trim();
            if trimmed.is_empty() {
                return;
            }
            let display = Self::truncate_with_ellipsis(trimmed, 160);
            if !lines.iter().any(|existing| existing.trim() == display) {
                lines.push(display);
            }
        }
    }

}
