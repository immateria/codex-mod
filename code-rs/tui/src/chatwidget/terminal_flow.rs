use super::*;

impl ChatWidget<'_> {
    pub(crate) fn launch_update_command(
        &mut self,
        command: Vec<String>,
        display: String,
        latest_version: Option<String>,
    ) -> Option<TerminalLaunch> {
        if !crate::updates::upgrade_ui_enabled() {
            return None;
        }

        self.pending_upgrade_notice = None;
        if command.is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "`/update` — no upgrade command available for this install.".to_string(),
            ));
            self.request_redraw();
            return None;
        }

        let command_text = if display.trim().is_empty() {
            strip_bash_lc_and_escape(&command)
        } else {
            display
        };

        if command_text.trim().is_empty() {
            self.history_push_plain_state(history_cell::new_error_event(
                "`/update` — unable to resolve upgrade command text.".to_string(),
            ));
            self.request_redraw();
            return None;
        }

        let id = self.terminal.alloc_id();
        if let Some(version) = &latest_version {
            self.pending_upgrade_notice = Some((id, version.clone()));
        }

        let (controller_tx, controller_rx) = mpsc::channel();
        let controller = TerminalRunController { tx: controller_tx };
        let display_label = Self::truncate_with_ellipsis(&format!("Guided: {command_text}"), 128);

        let launch = TerminalLaunch {
            id,
            title: "Upgrade Code".to_string(),
            command: Vec::new(),
            command_display: display_label,
            controller: Some(controller.clone()),
            auto_close_on_success: false,
            start_running: true,
        };

        let cwd = self.config.cwd.to_string_lossy().to_string();
        start_upgrade_terminal_session(UpgradeTerminalSessionArgs {
            app_event_tx: self.app_event_tx.clone(),
            terminal_id: id,
            initial_command: command_text,
            latest_version,
            cwd: Some(cwd),
            control: GuidedTerminalControl {
                controller,
                controller_rx,
            },
            config: self.config.clone(),
            debug_enabled: self.config.debug,
        });

        Some(launch)
    }

    pub(crate) fn terminal_open(&mut self, launch: &TerminalLaunch) {
        let mut overlay = TerminalOverlay::new(
            launch.id,
            launch.title.clone(),
            launch.command_display.clone(),
            launch.auto_close_on_success,
        );
        if !launch.start_running {
            overlay.running = false;
        }
        let visible = self.terminal.last_visible_rows.get();
        overlay.visible_rows = visible;
        overlay.clamp_scroll();
        overlay.ensure_pending_command();
        self.terminal.overlay = Some(overlay);
        self.request_redraw();
    }

    pub(crate) fn terminal_append_chunk(&mut self, id: u64, chunk: &[u8], is_stderr: bool) {
        let mut needs_redraw = false;
        let visible = self.terminal.last_visible_rows.get();
        let visible_cols = self.terminal.last_visible_cols.get();
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id {
                if visible > 0 {
                    overlay.pty_rows = visible;
                }
                if visible_cols > 0 {
                    overlay.pty_cols = visible_cols;
                }
                if visible != overlay.visible_rows {
                    overlay.visible_rows = visible;
                    overlay.clamp_scroll();
                }
                overlay.append_chunk(chunk, is_stderr);
                needs_redraw = true;
            }
        if needs_redraw {
            self.request_redraw();
        }
    }

    pub(crate) fn terminal_dimensions_hint(&self) -> Option<(u16, u16)> {
        let rows = self.terminal.last_visible_rows.get();
        let cols = self.terminal.last_visible_cols.get();
        if rows > 0 && cols > 0 {
            Some((rows, cols))
        } else {
            None
        }
    }

    pub(crate) fn terminal_apply_resize(&mut self, id: u64, rows: u16, cols: u16) {
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id && overlay.update_pty_dimensions(rows, cols) {
                self.request_redraw();
            }
    }

    pub(crate) fn request_terminal_cancel(&mut self, id: u64) {
        let mut needs_redraw = false;
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id {
                overlay.push_info_message("Cancel requested…");
                if overlay.running {
                    overlay.running = false;
                    needs_redraw = true;
                }
            }
        if needs_redraw {
            self.request_redraw();
        }
        self.app_event_tx.send(AppEvent::TerminalCancel { id });
    }

    pub(crate) fn terminal_update_message(&mut self, id: u64, message: String) {
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id {
                overlay.push_info_message(&message);
                self.request_redraw();
            }
    }

    pub(crate) fn terminal_set_assistant_message(&mut self, id: u64, message: String) {
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id {
                overlay.push_assistant_message(&message);
                self.request_redraw();
            }
    }

    pub(crate) fn terminal_set_command_display(&mut self, id: u64, command: String) {
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id {
                overlay.command_display = command;
                self.request_redraw();
            }
    }

    pub(crate) fn terminal_prepare_command(
        &mut self,
        id: u64,
        suggestion: String,
        ack: Sender<TerminalCommandGate>,
    ) {
        let mut updated = false;
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id {
                overlay.set_pending_command(suggestion, ack);
                updated = true;
            }
        if updated {
            self.request_redraw();
        }
    }

    pub(crate) fn terminal_accept_pending_command(&mut self) -> Option<PendingCommandAction> {
        if let Some(overlay) = self.terminal.overlay_mut() {
            if overlay.running {
                return None;
            }
            if let Some(action) = overlay.accept_pending_command() {
                match &action {
                    PendingCommandAction::Forwarded(command)
                    | PendingCommandAction::Manual(command) => {
                        overlay.command_display = command.clone();
                    }
                }
                self.request_redraw();
                return Some(action);
            }
        }
        None
    }

    pub(crate) fn terminal_execute_manual_command(&mut self, id: u64, command: String) {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            if let Some(overlay) = self.terminal.overlay_mut() {
                overlay.ensure_pending_command();
            }
            self.request_redraw();
            return;
        }

        if let Some(rest) = trimmed.strip_prefix("$$") {
            let prompt_text = rest.trim();
            if prompt_text.is_empty() {
                if let Some(overlay) = self.terminal.overlay_mut() {
                    overlay.push_info_message("Provide a prompt after '$'.");
                    overlay.ensure_pending_command();
                }
                self.request_redraw();
                return;
            }

            if let Some(overlay) = self.terminal.overlay_mut() {
                overlay.cancel_pending_command();
                overlay.running = true;
                overlay.exit_code = None;
                overlay.duration = None;
                overlay.push_assistant_message("Preparing guided command…");
            }

            let (controller_tx, controller_rx) = mpsc::channel();
            let controller = TerminalRunController { tx: controller_tx };
            let cwd = self.config.cwd.to_string_lossy().to_string();

            start_prompt_terminal_session(
                self.app_event_tx.clone(),
                id,
                prompt_text.to_string(),
                Some(cwd),
                controller,
                controller_rx,
                self.config.debug,
            );

            self.push_background_before_next_output(format!(
                "Terminal prompt: {prompt_text}"
            ));
            return;
        }

        let mut command_body = trimmed;
        let mut run_direct = false;
        if let Some(rest) = trimmed.strip_prefix('$') {
            let candidate = rest.trim();
            if candidate.is_empty() {
                if let Some(overlay) = self.terminal.overlay_mut() {
                    overlay.push_info_message("Provide a command after '$'.");
                    overlay.ensure_pending_command();
                }
                self.request_redraw();
                return;
            }
            command_body = candidate;
            run_direct = true;
        }

        let command_string = command_body.to_string();
        let wrapped_command = wrap_command(&command_string);
        if wrapped_command.is_empty() {
            self.app_event_tx.send(AppEvent::TerminalSetAssistantMessage {
                id,
                message: "Command could not be constructed.".to_string(),
            });
            if let Some(overlay) = self.terminal.overlay_mut() {
                overlay.ensure_pending_command();
            }
            self.request_redraw();
            return;
        }

        if !matches!(self.config.sandbox_policy, SandboxPolicy::DangerFullAccess) {
            if let Some(overlay) = self.terminal.overlay_mut() {
                overlay.cancel_pending_command();
            }
            self.pending_manual_terminal.insert(
                id,
                PendingManualTerminal {
                    command: command_string.clone(),
                    run_direct,
                },
            );
            if let Some(overlay) = self.terminal.overlay_mut() {
                overlay.push_assistant_message("Awaiting approval to run this command…");
                overlay.running = false;
            }
            let ticket = self.make_background_before_next_output_ticket();
            self.bottom_pane.push_approval_request(
                ApprovalRequest::TerminalCommand {
                    id,
                    command: command_string,
                },
                ticket,
            );
            self.request_redraw();
            return;
        }

        if run_direct && self.terminal_dimensions_hint().is_some() {
            self.start_direct_terminal_command(id, command_string, wrapped_command);
        } else {
            self.start_manual_terminal_session(id, command_string);
        }
    }

    pub(crate) fn start_manual_terminal_session(&mut self, id: u64, command: String) {
        if command.is_empty() {
            return;
        }
        if let Some(overlay) = self.terminal.overlay_mut() {
            overlay.cancel_pending_command();
            overlay.running = true;
            overlay.exit_code = None;
            overlay.duration = None;
        }
        let (controller_tx, controller_rx) = mpsc::channel();
        let controller = TerminalRunController { tx: controller_tx };
        let cwd = self.config.cwd.to_string_lossy().to_string();
        start_direct_terminal_session(
            self.app_event_tx.clone(),
            id,
            command,
            Some(cwd),
            controller,
            controller_rx,
            self.config.debug,
        );
    }

    pub(crate) fn start_direct_terminal_command(
        &mut self,
        id: u64,
        display: String,
        command: Vec<String>,
    ) {
        if let Some(overlay) = self.terminal.overlay_mut() {
            overlay.cancel_pending_command();
        }
        self.app_event_tx.send(AppEvent::TerminalRunCommand {
            id,
            command,
            command_display: display,
            controller: None,
        });
    }

    pub(crate) fn terminal_send_input(&mut self, id: u64, data: Vec<u8>) {
        if data.is_empty() {
            return;
        }
        self.app_event_tx
            .send(AppEvent::TerminalSendInput { id, data });
    }

    pub(crate) fn terminal_mark_running(&mut self, id: u64) {
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id {
                overlay.running = true;
                overlay.exit_code = None;
                overlay.duration = None;
                overlay.start_time = Some(Instant::now());
                self.request_redraw();
            }
    }

    pub(crate) fn terminal_finalize(
        &mut self,
        id: u64,
        exit_code: Option<i32>,
        duration: Duration,
    ) -> Option<TerminalAfter> {
        let mut success = false;
        let mut after = None;
        let mut needs_redraw = false;
        let mut should_close = false;
        let mut take_after = false;
        let visible = self.terminal.last_visible_rows.get();
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id {
                overlay.cancel_pending_command();
                if visible != overlay.visible_rows {
                    overlay.visible_rows = visible;
                    overlay.clamp_scroll();
                }
                let was_following = overlay.is_following();
                overlay.finalize(exit_code, duration);
                overlay.auto_follow(was_following);
                needs_redraw = true;
                if exit_code == Some(0) {
                    success = true;
                    take_after = true;
                    if overlay.auto_close_on_success {
                        should_close = true;
                    }
                }
                overlay.ensure_pending_command();
            }
        if take_after {
            after = self.terminal.after.take();
        }
        if should_close {
            self.terminal.overlay = None;
        }
        if needs_redraw {
            self.request_redraw();
        }
        if success {
            if crate::updates::upgrade_ui_enabled()
                && let Some((pending_id, version)) = self.pending_upgrade_notice.take() {
                    if pending_id == id {
                        self.bottom_pane
                            .flash_footer_notice(format!("Upgraded to {version}"));
                        self.latest_upgrade_version = None;
                    } else {
                        self.pending_upgrade_notice = Some((pending_id, version));
                    }
                }
            after
        } else {
            None
        }
    }

    pub(crate) fn terminal_prepare_rerun(&mut self, id: u64) -> bool {
        let mut reset = false;
        let visible = self.terminal.last_visible_rows.get();
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.id == id && !overlay.running {
                overlay.reset_for_rerun();
                overlay.visible_rows = visible;
                overlay.clamp_scroll();
                overlay.ensure_pending_command();
                reset = true;
            }
        if reset {
            self.request_redraw();
        }
        reset
    }

    pub(crate) fn handle_terminal_approval_decision(&mut self, id: u64, approved: bool) {
        let pending = self.pending_manual_terminal.remove(&id);
        if approved {
            if let Some(entry) = pending
                && self
                    .terminal
                    .overlay()
                    .map(|overlay| overlay.id == id)
                    .unwrap_or(false)
                {
                    if let Some(overlay) = self.terminal.overlay_mut() {
                        overlay.push_assistant_message("Approval granted. Running command…");
                    }
                    if entry.run_direct && self.terminal_dimensions_hint().is_some() {
                        let command_vec = wrap_command(&entry.command);
                        self.start_direct_terminal_command(id, entry.command, command_vec);
                    } else {
                        self.start_manual_terminal_session(id, entry.command);
                    }
                    self.request_redraw();
                }
            return;
        }

        if let Some(entry) = pending {
            if let Some(overlay) = self.terminal.overlay_mut() {
                overlay.push_info_message("Command was not approved. You can edit it and try again.");
                overlay.running = false;
                overlay.exit_code = None;
                overlay.duration = None;
                overlay.pending_command = Some(PendingCommand::manual_with_input(entry.command));
            }
            self.request_redraw();
        }
    }

    pub(crate) fn close_terminal_overlay(&mut self) {
        let mut cancel_id = None;
        let mut preserved_visible = None;
        let mut overlay_id = None;
        if let Some(overlay) = self.terminal.overlay_mut() {
            overlay_id = Some(overlay.id);
            if overlay.running {
                cancel_id = Some(overlay.id);
            }
            overlay.cancel_pending_command();
            preserved_visible = Some(overlay.visible_rows);
        }
        if let Some(id) = cancel_id {
            self.app_event_tx.send(AppEvent::TerminalCancel { id });
        }
        if let Some(id) = overlay_id {
            self.pending_manual_terminal.remove(&id);
        }
        if let Some(visible_rows) = preserved_visible {
            self.terminal.last_visible_rows.set(visible_rows);
        }
        self.terminal.clear();
        self.request_redraw();
    }

    pub(crate) fn terminal_overlay_id(&self) -> Option<u64> {
        self.terminal.overlay().map(|o| o.id)
    }

    pub(crate) fn terminal_overlay_active(&self) -> bool {
        self.terminal.overlay().is_some()
    }

    pub(crate) fn terminal_is_running(&self) -> bool {
        self.terminal.overlay().map(|o| o.running).unwrap_or(false)
    }

    pub(crate) fn ctrl_c_requests_exit(&self) -> bool {
        !self.terminal_overlay_active() && self.bottom_pane.ctrl_c_quit_hint_visible()
    }

    pub(crate) fn terminal_has_pending_command(&self) -> bool {
        self.terminal
            .overlay()
            .and_then(|overlay| overlay.pending_command.as_ref())
            .is_some()
    }

    pub(crate) fn terminal_handle_pending_key(&mut self, key_event: KeyEvent) -> bool {
        if self.terminal_is_running() {
            return false;
        }
        if !self.terminal_has_pending_command() {
            return false;
        }
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return true;
        }

        let mut needs_redraw = false;
        let mut handled = false;

        if let Some(overlay) = self.terminal.overlay_mut()
            && let Some(pending) = overlay.pending_command.as_mut() {
                match key_event.code {
                    KeyCode::Char(ch) => {
                        if key_event
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                        {
                            handled = true;
                        } else if pending.insert_char(ch) {
                            needs_redraw = true;
                            handled = true;
                        } else {
                            handled = true;
                        }
                    }
                    KeyCode::Backspace => {
                        handled = true;
                        if pending.backspace() {
                            needs_redraw = true;
                        }
                    }
                    KeyCode::Delete => {
                        handled = true;
                        if pending.delete() {
                            needs_redraw = true;
                        }
                    }
                    KeyCode::Left => {
                        handled = true;
                        if pending.move_left() {
                            needs_redraw = true;
                        }
                    }
                    KeyCode::Right => {
                        handled = true;
                        if pending.move_right() {
                            needs_redraw = true;
                        }
                    }
                    KeyCode::Home => {
                        handled = true;
                        if pending.move_home() {
                            needs_redraw = true;
                        }
                    }
                    KeyCode::End => {
                        handled = true;
                        if pending.move_end() {
                            needs_redraw = true;
                        }
                    }
                    KeyCode::Tab => {
                        handled = true;
                    }
                    _ => {}
                }
            }

        if needs_redraw {
            self.request_redraw();
        }
        handled
    }

    pub(crate) fn terminal_scroll_lines(&mut self, delta: i32) {
        let mut updated = false;
        let visible = self.terminal.last_visible_rows.get();
        if let Some(overlay) = self.terminal.overlay_mut() {
            if visible != overlay.visible_rows {
                overlay.visible_rows = visible;
            }
            let current = overlay.scroll as i32;
            let max_scroll = overlay.max_scroll() as i32;
            let mut next = current + delta;
            if next < 0 {
                next = 0;
            } else if next > max_scroll {
                next = max_scroll;
            }
            if next as u16 != overlay.scroll {
                overlay.scroll = next as u16;
                updated = true;
            }
        }
        if updated {
            self.request_redraw();
        }
    }

    pub(crate) fn terminal_scroll_page(&mut self, direction: i32) {
        let mut delta = None;
        let visible_value = self.terminal.last_visible_rows.get();
        if let Some(overlay) = self.terminal.overlay_mut() {
            let visible = visible_value.max(1);
            if visible != overlay.visible_rows {
                overlay.visible_rows = visible;
            }
            delta = Some((visible.saturating_sub(1)) as i32 * direction);
        }
        if let Some(amount) = delta {
            self.terminal_scroll_lines(amount);
        }
    }

    pub(crate) fn terminal_scroll_to_top(&mut self) {
        let mut updated = false;
        if let Some(overlay) = self.terminal.overlay_mut()
            && overlay.scroll != 0 {
                overlay.scroll = 0;
                updated = true;
            }
        if updated {
            self.request_redraw();
        }
    }

    pub(crate) fn terminal_scroll_to_bottom(&mut self) {
        let mut updated = false;
        let visible = self.terminal.last_visible_rows.get();
        if let Some(overlay) = self.terminal.overlay_mut() {
            if visible != overlay.visible_rows {
                overlay.visible_rows = visible;
            }
            let max_scroll = overlay.max_scroll();
            if overlay.scroll != max_scroll {
                overlay.scroll = max_scroll;
                updated = true;
            }
        }
        if updated {
            self.request_redraw();
        }
    }

    pub(crate) fn handle_terminal_after(&mut self, after: TerminalAfter) {
        match after {
            TerminalAfter::RefreshAgentsAndClose { selected_index } => {
                self.agents_overview_selected_index = selected_index;
                self.show_agents_overview_ui();
            }
        }
    }

    // show_subagent_editor_ui removed; use show_subagent_editor_for_name or show_new_subagent_editor

}
