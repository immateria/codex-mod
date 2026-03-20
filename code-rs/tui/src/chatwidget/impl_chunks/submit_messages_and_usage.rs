impl ChatWidget<'_> {
    /// Programmatically submit a user text message as if typed in the
    /// composer. The text will be added to conversation history and sent to
    /// the agent. This also handles slash command expansion.
    pub(crate) fn submit_text_message(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.submit_user_message(text.into());
    }

    /// Submit a message where the user sees `display` in history, but the
    /// model receives only `prompt`. This is used for prompt-expanding
    /// slash commands selected via the popup where expansion happens before
    /// reaching the normal composer pipeline.
    pub(crate) fn submit_prompt_with_display(&mut self, display: String, prompt: String) {
        if display.is_empty() && prompt.is_empty() {
            return;
        }
        use crate::chatwidget::message::UserMessage;
        use code_core::protocol::InputItem;
        let mut ordered = Vec::new();
        if !prompt.trim().is_empty() {
            ordered.push(InputItem::Text { text: prompt });
        }
        let msg = UserMessage {
            display_text: display,
            ordered_items: ordered,
            suppress_persistence: false,
        };
        self.submit_user_message(msg);
    }

    /// Submit a visible text message, but prepend a hidden instruction that is
    /// sent to the agent in the same turn. The hidden text is not added to the
    /// chat history; only `visible` appears to the user.
    pub(crate) fn submit_text_message_with_preface(&mut self, visible: String, preface: String) {
        if visible.is_empty() {
            return;
        }
        use crate::chatwidget::message::UserMessage;
        use code_core::protocol::InputItem;
        let mut ordered = Vec::new();
        if !preface.trim().is_empty() {
            ordered.push(InputItem::Text { text: preface });
        }
        ordered.push(InputItem::Text {
            text: visible.clone(),
        });
        let msg = UserMessage {
            display_text: visible,
            ordered_items: ordered,
            suppress_persistence: false,
        };
        self.submit_user_message(msg);
    }

    pub(crate) fn submit_hidden_text_message_with_preface(
        &mut self,
        agent_text: String,
        preface: String,
    ) {
        self.submit_hidden_text_message_with_preface_and_notice(agent_text, preface, false);
    }

    /// Submit a hidden message with optional notice surfacing.
    /// When `surface_notice` is true, the injected text is also shown in history
    /// as a developer-style notice; when false, the injection is silent.
    pub(crate) fn submit_hidden_text_message_with_preface_and_notice(
        &mut self,
        agent_text: String,
        preface: String,
        surface_notice: bool,
    ) {
        if agent_text.trim().is_empty() && preface.trim().is_empty() {
            return;
        }
        use crate::chatwidget::message::UserMessage;
        use code_core::protocol::InputItem;

        let mut ordered = Vec::new();
        let preface_cache = preface.clone();
        let agent_cache = agent_text.clone();
        if !preface.trim().is_empty() {
            ordered.push(InputItem::Text { text: preface });
        }
        if !agent_text.trim().is_empty() {
            ordered.push(InputItem::Text { text: agent_text });
        }

        if ordered.is_empty() {
            return;
        }

        if surface_notice {
            // Surface immediately in the TUI as a notice (developer-style message).
            let mut notice_lines = Vec::new();
            if !preface_cache.trim().is_empty() {
                notice_lines.push(preface_cache.trim().to_string());
            }
            if !agent_cache.trim().is_empty() {
                notice_lines.push(agent_cache.trim().to_string());
            }
            if !notice_lines.is_empty() {
                self.history_push_plain_paragraphs(PlainMessageKind::Notice, notice_lines);
            }
        }

        let msg = UserMessage {
            display_text: String::new(),
            ordered_items: ordered,
            suppress_persistence: false,
        };
        let mut cache = String::new();
        if !preface_cache.trim().is_empty() {
            cache.push_str(preface_cache.trim());
        }
        if !agent_cache.trim().is_empty() {
            if !cache.is_empty() {
                cache.push('\n');
            }
            cache.push_str(agent_cache.trim());
        }
        let cleaned = Self::strip_context_sections(&cache);
        self.last_developer_message = (!cleaned.trim().is_empty()).then_some(cleaned);
        self.pending_turn_origin = Some(TurnOrigin::Developer);
        self.submit_user_message_immediate(msg);
    }

    /// Dispatch a user message immediately, bypassing the queued/turn-active
    /// path. Used for developer/system injections that must not be lost if the
    /// current turn ends abruptly.
    fn submit_user_message_immediate(&mut self, message: UserMessage) {
        if message.ordered_items.is_empty() {
            return;
        }

        let items = message.ordered_items.clone();
        if let Err(e) = self.code_op_tx.send(Op::UserInput {
            items,
            final_output_json_schema: None,
        }) {
            tracing::error!("failed to send immediate UserInput: {e}");
        }

        self.finalize_sent_user_message(message);
    }

    /// Queue a note that will be delivered to the agent as a hidden system
    /// message immediately before the next user input is sent. Notes are
    /// drained in FIFO order so multiple updates retain their sequencing.
    pub(crate) fn queue_agent_note<S: Into<String>>(&mut self, note: S) {
        let note = note.into();
        if note.trim().is_empty() {
            return;
        }
        self.pending_agent_notes.push(note);
    }

    pub(crate) fn token_usage(&self) -> &TokenUsage {
        &self.total_token_usage
    }

    pub(crate) fn session_id(&self) -> Option<uuid::Uuid> {
        self.session_id
    }

    fn insert_resume_placeholder(&mut self) {
        if self.resume_placeholder_visible {
            return;
        }
        let key = self.next_req_key_top();
        let cell = history_cell::new_background_event(RESUME_PLACEHOLDER_MESSAGE.to_string());
        let _ = self.history_insert_with_key_global_tagged(Box::new(cell), key, "background", None);
        self.resume_placeholder_visible = true;
    }

    fn clear_resume_placeholder(&mut self) {
        if !self.resume_placeholder_visible {
            return;
        }
        if let Some(idx) = self.history_cells.iter().position(|cell| {
            cell.as_any()
                .downcast_ref::<crate::history_cell::BackgroundEventCell>()
                .map(|c| c.state().description.trim() == RESUME_PLACEHOLDER_MESSAGE)
                .unwrap_or(false)
        }) {
            self.history_remove_at(idx);
        }
        self.resume_placeholder_visible = false;
    }

    fn replace_resume_placeholder_with_notice(&mut self, message: &str) {
        if !self.resume_placeholder_visible {
            return;
        }
        self.clear_resume_placeholder();
        self.push_background_tail(message.to_string());
    }

    pub(crate) fn clear_token_usage(&mut self) {
        self.total_token_usage = TokenUsage::default();
        self.rate_limit_snapshot = None;
        self.rate_limit_warnings.reset();
        self.rate_limit_last_fetch_at = None;
        self.bottom_pane.set_token_usage(
            self.last_token_usage.clone(),
            self.config.model_context_window,
            self.config.context_mode,
        );
    }

    fn log_and_should_display_warning(&self, warning: &RateLimitWarning) -> bool {
        let reset_at = match warning.scope {
            RateLimitWarningScope::Primary => self.rate_limit_primary_next_reset_at,
            RateLimitWarningScope::Secondary => self.rate_limit_secondary_next_reset_at,
        };

        let account_id = auth_accounts::get_active_account_id(&self.config.code_home)
            .ok()
            .flatten()
            .unwrap_or_else(|| "_default".to_string());

        let plan = if account_id == "_default" {
            None
        } else {
            match account_usage::list_rate_limit_snapshots(&self.config.code_home) {
                Ok(records) => records
                    .into_iter()
                    .find(|record| record.account_id == account_id)
                    .and_then(|record| record.plan),
                Err(err) => {
                    tracing::warn!(?err, "failed to load rate limit snapshots while logging warning");
                    None
                }
            }
        };

        match account_usage::record_rate_limit_warning(
            &self.config.code_home,
            &account_id,
            plan.as_deref(),
            account_usage::RateLimitWarningEvent::new(
                warning.scope,
                warning.threshold,
                reset_at,
                Utc::now(),
                &warning.message,
            ),
        ) {
            Ok(result) => result,
            Err(err) => {
                tracing::warn!(?err, "failed to persist rate limit warning log");
                true
            }
        }
    }
}
