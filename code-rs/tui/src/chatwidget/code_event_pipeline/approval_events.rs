use super::*;
use code_core::protocol::OrderMeta;
use code_core::protocol::RequestUserInputEvent;
use code_protocol::dynamic_tools::DynamicToolCallRequest;
use code_protocol::request_user_input::RequestUserInputAnswer;
use code_protocol::request_user_input::RequestUserInputQuestion;
use code_protocol::request_user_input::RequestUserInputResponse;

impl ChatWidget<'_> {
    pub(super) fn handle_exec_approval_request_event(
        &mut self,
        id: String,
        ev: ExecApprovalRequestEvent,
        seq: u64,
    ) {
        let id2 = id.clone();
        let ev2 = ev.clone();
        self.defer_or_handle(
            move |interrupts| interrupts.push_exec_approval(seq, id, ev),
            |this| {
                this.finalize_active_stream();
                this.flush_interrupt_queue();
                this.handle_exec_approval_now(id2, ev2);
                this.request_redraw();
            },
        );
    }

    pub(super) fn handle_request_user_input_event(
        &mut self,
        order: Option<&OrderMeta>,
        ev: RequestUserInputEvent,
    ) {
        let key = self.near_time_key_current_req(order);
        let mut lines: Vec<String> = Vec::new();
        lines.push("Model requested user input".to_string());

        for question in &ev.questions {
            let header = &question.header;
            let id = &question.id;
            let question_text = &question.question;
            lines.push(format!("\n{header} ({id})\n{question_text}"));
            if let Some(options) = &question.options {
                lines.push("Options:".to_string());
                for option in options {
                    let label = &option.label;
                    let description = &option.description;
                    lines.push(format!("- {label}: {description}"));
                }
            }
        }
        let auto_answer = self.auto_state.is_active() && !self.auto_state.is_paused_manual();
        if auto_answer {
            lines.push("\nAuto Drive is active; continuing automatically.".to_string());
        } else {
            lines.push("\nUse the picker below to continue (Esc to type in the composer).".to_string());
        }

        let role = history_cell::plain_role_for_kind(PlainMessageKind::Notice);
        let state =
            history_cell::plain_message_state_from_paragraphs(PlainMessageKind::Notice, role, lines);
        let _ = self.history_insert_plain_state_with_key(state, key, "request_user_input");
        self.restore_reasoning_in_progress_if_streaming();

        if auto_answer {
            let response = Self::build_auto_request_user_input_response(&ev.questions);
            let summary = Self::build_auto_request_user_input_summary(&ev.questions, &response);

            if !summary.trim().is_empty() {
                let key = Self::order_key_successor(key);
                let role = history_cell::plain_role_for_kind(PlainMessageKind::Notice);
                let state = history_cell::plain_message_state_from_paragraphs(
                    PlainMessageKind::Notice,
                    role,
                    vec![format!("Auto Drive answered user input:\n{summary}")],
                );
                let _ = self
                    .history_insert_plain_state_with_key(state, key, "request_user_input_auto_answer");
                self.restore_reasoning_in_progress_if_streaming();
            }

            if let Err(e) = self.code_op_tx.send(Op::UserInputAnswer {
                id: ev.turn_id,
                response,
            }) {
                tracing::error!("failed to send Op::UserInputAnswer: {e}");
            }

            self.bottom_pane
                .update_status_text("waiting for model".to_string());
            self.bottom_pane.set_task_running(true);
        } else {
            self.pending_request_user_input = Some(PendingRequestUserInput {
                turn_id: ev.turn_id.clone(),
                call_id: ev.call_id.clone(),
                anchor_key: key,
                questions: ev.questions.clone(),
            });
            self.bottom_pane
                .update_status_text("waiting for user input".to_string());
            self.bottom_pane.set_task_running(true);
            self.bottom_pane.ensure_input_focus();
            self.bottom_pane
                .show_request_user_input(crate::bottom_pane::RequestUserInputView::new(
                    ev.turn_id.clone(),
                    ev.questions,
                    self.app_event_tx.clone(),
                ));
        }
        self.request_redraw();
    }

    pub(super) fn handle_dynamic_tool_call_request_event(
        &mut self,
        order: Option<&OrderMeta>,
        ev: DynamicToolCallRequest,
    ) {
        let key = self.near_time_key_current_req(order);
        let tool = &ev.tool;
        let call_id = &ev.call_id;
        let lines = vec![
            format!("Dynamic tool call requested: {tool}"),
            format!("call_id: {call_id}"),
            "Dynamic tools are not supported in this UI; returning a failure response.".to_string(),
        ];
        let role = history_cell::plain_role_for_kind(PlainMessageKind::Notice);
        let state =
            history_cell::plain_message_state_from_paragraphs(PlainMessageKind::Notice, role, lines);
        let _ = self.history_insert_plain_state_with_key(state, key, "dynamic_tool_call");
        self.restore_reasoning_in_progress_if_streaming();

        let response = DynamicToolResponse {
            content_items: vec![
                code_protocol::dynamic_tools::DynamicToolCallOutputContentItem::InputText {
                    text: "dynamic tools are not supported in this UI".to_string(),
                },
            ],
            success: false,
        };
        if let Err(e) = self.code_op_tx.send(Op::DynamicToolResponse {
            id: ev.call_id.clone(),
            response,
        }) {
            tracing::error!("failed to send Op::DynamicToolResponse: {e}");
        }

        self.bottom_pane
            .update_status_text("waiting for model".to_string());
        self.bottom_pane.set_task_running(true);
        self.request_redraw();
    }

    pub(super) fn handle_apply_patch_approval_request_event(
        &mut self,
        id: String,
        ev: ApplyPatchApprovalRequestEvent,
        seq: u64,
    ) {
        let id2 = id.clone();
        let ev2 = ev.clone();
        self.defer_or_handle(
            move |interrupts| interrupts.push_apply_patch_approval(seq, id, ev),
            |this| {
                this.finalize_active_stream();
                this.flush_interrupt_queue();
                // Push approval UI state to bottom pane and surface the patch summary there.
                // (Avoid inserting a duplicate summary here; handle_apply_patch_approval_now
                // is responsible for rendering the proposed patch once.)
                this.handle_apply_patch_approval_now(id2, ev2);
                this.request_redraw();
            },
        );
    }

    fn choose_option_label(question: &RequestUserInputQuestion) -> Option<String> {
        let options = question.options.as_ref()?;
        if options.is_empty() {
            return None;
        }

        let recommended = options.iter().position(|opt| {
            opt.label.contains("(Recommended)")
                || opt.label.contains("Recommended")
                || opt.label.contains("recommended")
        });
        let idx = recommended.unwrap_or(0);
        options.get(idx).map(|opt| opt.label.clone())
    }

    fn choose_freeform_value(question: &RequestUserInputQuestion) -> String {
        let key = format!("{} {}", question.id, question.header).to_ascii_lowercase();
        if key.contains("confirm") || key.contains("proceed") {
            "yes".to_string()
        } else if key.contains("name") {
            "Auto Drive".to_string()
        } else {
            "auto".to_string()
        }
    }

    fn build_auto_request_user_input_response(
        questions: &[RequestUserInputQuestion],
    ) -> RequestUserInputResponse {
        let mut answers = std::collections::HashMap::new();
        for question in questions {
            let answer_value = if let Some(label) = Self::choose_option_label(question) {
                vec![label]
            } else {
                vec![Self::choose_freeform_value(question)]
            };
            answers.insert(
                question.id.clone(),
                RequestUserInputAnswer {
                    answers: answer_value,
                },
            );
        }
        RequestUserInputResponse { answers }
    }

    fn build_auto_request_user_input_summary(
        questions: &[RequestUserInputQuestion],
        response: &RequestUserInputResponse,
    ) -> String {
        let mut parts = Vec::new();
        for question in questions {
            let label = response
                .answers
                .get(&question.id)
                .and_then(|a| a.answers.first())
                .map(String::as_str)
                .unwrap_or("(skipped)");
            if questions.len() == 1 {
                parts.push(label.to_string());
            } else {
                let header = question.header.trim();
                if header.is_empty() {
                    parts.push(label.to_string());
                } else {
                    parts.push(format!("{header}: {label}"));
                }
            }
        }
        parts.join("\n")
    }
}
