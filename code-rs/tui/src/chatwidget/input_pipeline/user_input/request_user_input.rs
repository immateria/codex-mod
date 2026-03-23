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
            let value = answer.first().map(String::as_str).unwrap_or("");
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
