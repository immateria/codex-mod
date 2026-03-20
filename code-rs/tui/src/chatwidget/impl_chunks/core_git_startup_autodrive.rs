impl ChatWidget<'_> {
    const MAX_UNDO_CONVERSATION_MESSAGES: usize = 8;
    const MAX_UNDO_PREVIEW_CHARS: usize = 160;
    const MAX_UNDO_FILE_LINES: usize = 24;

    fn is_branch_worktree_path(path: &std::path::Path) -> bool {
        for ancestor in path.ancestors() {
            if ancestor
                .file_name()
                .map(|name| name == std::ffi::OsStr::new("branches"))
                .unwrap_or(false)
            {
                let mut higher = ancestor.parent();
                while let Some(dir) = higher {
                    if dir
                        .file_name()
                        .map(|name| name == std::ffi::OsStr::new(".code"))
                        .unwrap_or(false)
                    {
                        return true;
                    }
                    higher = dir.parent();
                }
            }
        }
        false
    }

    fn merge_lock_for_repo(path: &std::path::Path) -> Arc<tokio::sync::Mutex<()>> {
        let key = path.to_path_buf();
        let mut locks = MERGE_LOCKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match locks.entry(key) {
            Entry::Occupied(existing) => existing.get().clone(),
            Entry::Vacant(slot) => slot.insert(Arc::new(tokio::sync::Mutex::new(()))).clone(),
        }
    }

    async fn git_short_status(path: &std::path::Path) -> Result<String, String> {
        use tokio::process::Command;
        match Command::new("git")
            .current_dir(path)
            .args(["status", "--short"])
            .output()
            .await
        {
            Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Ok(out) => {
                let stderr_s = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let stdout_s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !stderr_s.is_empty() {
                    Err(stderr_s)
                } else if !stdout_s.is_empty() {
                    Err(stdout_s)
                } else {
                    let code = out
                        .status
                        .code()
                        .map(|c| format!("exit status {c}"))
                        .unwrap_or_else(|| "terminated by signal".to_string());
                    Err(format!("git status failed: {code}"))
                }
            }
            Err(err) => Err(err.to_string()),
        }
    }

    async fn git_diff_stat(path: &std::path::Path) -> Result<String, String> {
        use tokio::process::Command;
        match Command::new("git")
            .current_dir(path)
            .args(["diff", "--stat"])
            .output()
            .await
        {
            Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Ok(out) => {
                let stderr_s = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let stdout_s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !stderr_s.is_empty() {
                    Err(stderr_s)
                } else if !stdout_s.is_empty() {
                    Err(stdout_s)
                } else {
                    let code = out
                        .status
                        .code()
                        .map(|c| format!("exit status {c}"))
                        .unwrap_or_else(|| "terminated by signal".to_string());
                    Err(format!("git diff --stat failed: {code}"))
                }
            }
            Err(err) => Err(err.to_string()),
        }
    }

    pub(super) fn is_startup_mcp_error(&self, message: &str) -> bool {
        if self.last_seen_request_index != 0 || self.pending_user_prompts_for_next_turn > 0 {
            return false;
        }

        let lower = message.to_ascii_lowercase();
        lower.contains("mcp server")
            && (lower.contains("failed to start") || lower.contains("failed to list tools"))
    }

    fn extract_mcp_server_name(message: &str) -> Option<&str> {
        for (marker, terminator) in [("MCP server `", '`'), ("MCP server '", '\'')] {
            let start = message.find(marker).map(|idx| idx + marker.len());
            if let Some(start) = start {
                let rest = &message[start..];
                if let Some(end) = rest.find(terminator) {
                    let name = &rest[..end];
                    if !name.is_empty() {
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    pub(super) fn summarize_startup_mcp_error(message: &str) -> String {
        if let Some(name) = Self::extract_mcp_server_name(message) {
            return format!(
                "MCP server '{name}' failed to initialize. Run /mcp status for diagnostics."
            );
        }
        "MCP server failed to initialize. Run /mcp status for diagnostics.".to_string()
    }

    fn background_tail_order_ticket_internal(&mut self) -> BackgroundOrderTicket {
        let req = self.background_tail_request_ordinal();
        self.background_order_ticket_for_req(req)
    }

    fn background_before_next_output_request_ordinal(&mut self) -> u64 {
        if self.last_seen_request_index > 0 {
            self.last_seen_request_index
        } else {
            *self.synthetic_system_req.get_or_insert(1)
        }
    }

    fn background_before_next_output_ticket_internal(&mut self) -> BackgroundOrderTicket {
        let req = self.background_before_next_output_request_ordinal();
        self.background_order_ticket_for_req(req)
    }

    pub(crate) fn make_background_tail_ticket(&mut self) -> BackgroundOrderTicket {
        self.background_tail_order_ticket_internal()
    }

    pub(crate) fn make_background_before_next_output_ticket(&mut self) -> BackgroundOrderTicket {
        self.background_before_next_output_ticket_internal()
    }

    fn auto_card_next_order_key(&mut self) -> OrderKey {
        let ticket = self.make_background_tail_ticket();
        let meta = ticket.next_order();
        self.provider_order_key_from_order_meta(&meta)
    }

    fn auto_card_start(&mut self, goal: Option<String>) {
        let order_key = self.auto_card_next_order_key();
        auto_drive_cards::start_session(self, order_key, goal);
    }

    fn auto_card_add_action(&mut self, message: String, kind: AutoDriveActionKind) {
        let order_key = self.auto_card_next_order_key();
        let had_tracker = self.tools_state.auto_drive_tracker.is_some();
        auto_drive_cards::record_action(self, order_key, message.clone(), kind);
        if !had_tracker {
            self.push_background_tail(message);
        }
    }

    fn auto_card_set_status(&mut self, status: AutoDriveStatus) {
        if self.tools_state.auto_drive_tracker.is_some() {
            let order_key = self.auto_card_next_order_key();
            auto_drive_cards::set_status(self, order_key, status);
        }
    }

    fn auto_card_set_goal(&mut self, goal: Option<String>) {
        if self.tools_state.auto_drive_tracker.is_none() {
            return;
        }
        let order_key = self.auto_card_next_order_key();
        auto_drive_cards::update_goal(self, order_key, goal);
    }

    fn auto_card_finalize(
        &mut self,
        message: Option<String>,
        status: AutoDriveStatus,
        kind: AutoDriveActionKind,
    ) {
        let had_tracker = self.tools_state.auto_drive_tracker.is_some();
        let order_key = self.auto_card_next_order_key();
        let completion_message = if matches!(status, AutoDriveStatus::Stopped) {
            self.auto_state.last_completion_explanation.clone()
        } else {
            None
        };
        auto_drive_cards::finalize(
            self,
            order_key,
            message.clone(),
            status,
            kind,
            completion_message,
        );
        if !had_tracker
            && let Some(msg) = message {
                self.push_background_tail(msg);
            }
        if matches!(status, AutoDriveStatus::Stopped) {
            self.auto_state.last_completion_explanation = None;
        }
        auto_drive_cards::clear(self);
    }

    fn auto_request_session_summary(&mut self) {
        let prompt = AUTO_DRIVE_SESSION_SUMMARY_PROMPT.trim();
        if prompt.is_empty() {
            tracing::warn!("Auto Drive session summary prompt is empty");
            return;
        }

        self.push_background_tail(AUTO_DRIVE_SESSION_SUMMARY_NOTICE.to_string());
        self.request_redraw();
        self.submit_hidden_text_message_with_preface(prompt.to_string(), String::new());
    }

    fn spawn_conversation_runtime(
        &mut self,
        config: Config,
        auth_manager: Arc<AuthManager>,
        code_op_rx: UnboundedReceiver<Op>,
    ) {
        let ticket = self.make_background_tail_ticket();
        agent::spawn_new_conversation_runtime(
            config,
            self.app_event_tx.clone(),
            auth_manager,
            code_op_rx,
            ticket,
        );
    }

    fn consume_pending_prompt_for_ui_only_turn(&mut self) {
        if self.pending_user_prompts_for_next_turn > 0 {
            self.pending_user_prompts_for_next_turn -= 1;
        }
        if !self.pending_dispatched_user_messages.is_empty() {
            self.pending_dispatched_user_messages.pop_front();
        }
    }

}
