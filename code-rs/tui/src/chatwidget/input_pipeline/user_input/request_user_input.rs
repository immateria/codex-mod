impl ChatWidget<'_> {
    pub(in super::super) fn submit_request_user_input_answer(&mut self, pending: PendingRequestUserInput, raw: String) {
        use code_protocol::request_user_input::RequestUserInputAnswer;
        use code_protocol::request_user_input::RequestUserInputResponse;

        tracing::info!(
            "[request_user_input] answer turn_id={} call_id={}",
            pending.turn_id,
            pending.call_id
        );

        let response = serde_json::from_str::<RequestUserInputResponse>(&raw).unwrap_or_else(|_| {
            let question_count = pending.questions.len();
            let mut lines: Vec<String> = raw
                .lines()
                .map(|line| line.trim_end().to_string())
                .collect();

            if question_count <= 1 {
                lines = vec![raw.trim().to_string()];
            } else if lines.len() > question_count {
                let tail = lines.split_off(question_count - 1);
                lines.push(tail.join("\n"));
            }

            while lines.len() < question_count {
                lines.push(String::new());
            }

            let mut answers = std::collections::HashMap::new();
            for (idx, question) in pending.questions.iter().enumerate() {
                let value = lines.get(idx).cloned().unwrap_or_default();
                answers.insert(
                    question.id.clone(),
                    RequestUserInputAnswer {
                        answers: vec![value],
                    },
                );
            }
            RequestUserInputResponse { answers }
        });

        let display_text =
            Self::format_request_user_input_display(&pending.questions, &response);
        if !display_text.trim().is_empty() {
            let key = Self::order_key_successor(pending.anchor_key);
            let state = history_cell::new_user_prompt(display_text);
            let _ =
                self.history_insert_plain_state_with_key(state, key, "request_user_input_answer");
            self.restore_reasoning_in_progress_if_streaming();
        }

        if let Err(e) = self.code_op_tx.send(Op::UserInputAnswer {
            id: pending.turn_id,
            response,
        }) {
            tracing::error!("failed to send Op::UserInputAnswer: {e}");
        }

        self.clear_composer();
        self.bottom_pane
            .update_status_text("waiting for model".to_string());
        self.request_redraw();
    }

    pub(in super::super) fn format_request_user_input_display(
        questions: &[code_protocol::request_user_input::RequestUserInputQuestion],
        response: &code_protocol::request_user_input::RequestUserInputResponse,
    ) -> String {
        let mut lines = Vec::new();
        for question in questions {
            let answer: &[String] = response
                .answers
                .get(&question.id)
                .map(|a| a.answers.as_slice())
                .unwrap_or(&[]);
            let value = answer.first().map_or("", String::as_str);
            let value = if value.trim().is_empty() {
                "(skipped)"
            } else if question.is_secret {
                "[hidden]"
            } else {
                value
            };

            if questions.len() == 1 {
                lines.push(value.to_string());
            } else {
                let header = question.header.trim();
                if header.is_empty() {
                    lines.push(value.to_string());
                } else {
                    lines.push(format!("{header}: {value}"));
                }
            }
        }
        lines.join("\n")
    }

    pub(crate) fn on_request_user_input_answer(
        &mut self,
        turn_id: String,
        response: code_protocol::request_user_input::RequestUserInputResponse,
    ) {
        if turn_id.starts_with("mcp_elicitation:") {
            let Some(pending) = self.pending_mcp_elicitation.take() else {
                tracing::warn!(
                    "[mcp_elicitation] received UI answer but no request is pending (turn_id={turn_id})"
                );
                return;
            };

            if pending.turn_id != turn_id {
                tracing::warn!(
                    "[mcp_elicitation] received UI answer for unexpected turn_id (expected={}, got={turn_id})",
                    pending.turn_id,
                );
            }

            self.bottom_pane.close_request_user_input_view();

            let selected = response
                .answers
                .get("mcp_elicitation")
                .and_then(|answer| answer.answers.first())
                .map(|value| value.trim().to_string())
                .unwrap_or_default();

            let action = match selected.to_ascii_lowercase().as_str() {
                "accept" => code_protocol::approvals::ElicitationAction::Accept,
                "cancel" => code_protocol::approvals::ElicitationAction::Cancel,
                _ => code_protocol::approvals::ElicitationAction::Decline,
            };

            let display_text = match action {
                code_protocol::approvals::ElicitationAction::Accept => "Accept".to_string(),
                code_protocol::approvals::ElicitationAction::Decline => "Decline".to_string(),
                code_protocol::approvals::ElicitationAction::Cancel => "Cancel".to_string(),
            };
            let key = Self::order_key_successor(pending.anchor_key);
            let state = history_cell::new_user_prompt(display_text);
            let _ = self
                .history_insert_plain_state_with_key(state, key, "mcp_elicitation_answer");
            self.restore_reasoning_in_progress_if_streaming();

            if let Err(e) = self.code_op_tx.send(Op::ResolveMcpElicitation {
                server_name: pending.server_name,
                id: pending.id,
                action,
                // The picker currently doesn't surface typed form input, so accept returns `{}`.
                content: matches!(action, code_protocol::approvals::ElicitationAction::Accept)
                    .then(|| serde_json::json!({})),
                meta: None,
            }) {
                tracing::error!("failed to send Op::ResolveMcpElicitation: {e}");
            }

            self.clear_composer();
            self.bottom_pane
                .update_status_text("waiting for model".to_string());
            self.request_redraw();
            return;
        }

        let Some(pending) = self.pending_request_user_input.take() else {
            tracing::warn!(
                "[request_user_input] received UI answer but no request is pending (turn_id={turn_id})"
            );
            return;
        };

        if pending.turn_id != turn_id {
            tracing::warn!(
                "[request_user_input] received UI answer for unexpected turn_id (expected={}, got={turn_id})",
                pending.turn_id,
            );
        }

        self.bottom_pane.close_request_user_input_view();

        let display_text =
            Self::format_request_user_input_display(&pending.questions, &response);

        if !display_text.trim().is_empty() {
            let key = Self::order_key_successor(pending.anchor_key);
            let state = history_cell::new_user_prompt(display_text);
            let _ =
                self.history_insert_plain_state_with_key(state, key, "request_user_input_answer");
            self.restore_reasoning_in_progress_if_streaming();
        }

        if let Err(e) = self.code_op_tx.send(Op::UserInputAnswer {
            id: pending.turn_id,
            response,
        }) {
            tracing::error!("failed to send Op::UserInputAnswer: {e}");
        }

        self.clear_composer();
        self.bottom_pane
            .update_status_text("waiting for model".to_string());
        self.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::chatwidget::smoke_helpers::ChatWidgetHarness;
    use code_core::protocol::{Event, EventMsg};
    use code_protocol::approvals::{ElicitationRequest, ElicitationRequestEvent};
    use code_protocol::mcp::RequestId;
    use code_protocol::request_user_input::RequestUserInputAnswer;
    use code_protocol::request_user_input::RequestUserInputResponse;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn mcp_elicitation_accept_sends_empty_object_content() {
        let mut harness = ChatWidgetHarness::new();

        let (op_tx, mut op_rx) = unbounded_channel::<Op>();
        harness.with_chat(|chat| {
            chat.code_op_tx = op_tx;
        });

        let server_name = "demo_server".to_string();
        let request_id = RequestId::String("req_123".to_string());
        let request = ElicitationRequest::Form {
            meta: None,
            message: "please confirm".to_string(),
            requested_schema: serde_json::json!({"type":"object"}),
        };
        harness.handle_event(Event {
            id: "evt".into(),
            event_seq: 0,
            msg: EventMsg::ElicitationRequest(ElicitationRequestEvent {
                turn_id: None,
                server_name: server_name.clone(),
                id: request_id.clone(),
                request,
            }),
            order: None,
        });

        let mut answers = std::collections::HashMap::new();
        answers.insert(
            "mcp_elicitation".to_string(),
            RequestUserInputAnswer {
                answers: vec!["Accept".to_string()],
            },
        );
        let response = RequestUserInputResponse { answers };
        let turn_id = "mcp_elicitation:demo_server:req_123".to_string();

        harness.with_chat(|chat| chat.on_request_user_input_answer(turn_id, response));

        match op_rx.try_recv().expect("expected resolve op") {
            Op::ResolveMcpElicitation {
                server_name: got_server,
                id: got_id,
                action,
                content,
                meta,
            } => {
                assert_eq!(got_server, server_name);
                assert_eq!(got_id, request_id);
                assert_eq!(action, code_protocol::approvals::ElicitationAction::Accept);
                assert_eq!(content, Some(serde_json::json!({})));
                assert_eq!(meta, None);
            }
            other => panic!("unexpected op: {other:?}"),
        }
    }
}
