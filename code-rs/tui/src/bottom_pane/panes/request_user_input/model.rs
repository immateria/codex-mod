use std::collections::HashMap;

use ratatui::layout::Rect;

use code_protocol::request_user_input::{
    RequestUserInputAnswer,
    RequestUserInputQuestion,
    RequestUserInputResponse,
};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::components::scroll_state::ScrollState;

use super::{AnswerState, RequestUserInputView};

impl RequestUserInputView {
    pub(crate) fn new(
        turn_id: String,
        call_id: String,
        questions: Vec<RequestUserInputQuestion>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let answers = questions
            .iter()
            .map(|q| {
                let mut option_state = ScrollState::new();
                if q
                    .options
                    .as_ref()
                    .is_some_and(|options| !options.is_empty())
                {
                    option_state.selected_idx = Some(0);
                }
                AnswerState {
                    option_state,
                    hover_option_idx: None,
                    freeform: String::new(),
                }
            })
            .collect();

        Self {
            app_event_tx,
            turn_id,
            call_id,
            questions,
            answers,
            current_idx: 0,
            submitting: false,
            complete: false,
        }
    }

    pub(super) fn is_mcp_access_prompt(&self) -> bool {
        self.call_id.starts_with("mcp_access:") || self.call_id.starts_with("mcp_elicitation:")
    }

    pub(super) fn question_count(&self) -> usize {
        self.questions.len()
    }

    pub(super) fn current_question(&self) -> Option<&RequestUserInputQuestion> {
        self.questions.get(self.current_idx)
    }

    pub(super) fn current_answer_mut(&mut self) -> Option<&mut AnswerState> {
        self.answers.get_mut(self.current_idx)
    }

    pub(super) fn current_answer(&self) -> Option<&AnswerState> {
        self.answers.get(self.current_idx)
    }

    pub(super) fn current_options_len(&self) -> usize {
        self.current_question()
            .and_then(|q| q.options.as_ref())
            .map(std::vec::Vec::len)
            .unwrap_or(0)
    }

    pub(super) fn current_is_other(&self) -> bool {
        self.current_question().is_some_and(|q| q.is_other)
    }

    pub(super) fn current_other_index(&self) -> Option<usize> {
        let options_len = self.current_options_len();
        if options_len > 0 && self.current_is_other() {
            Some(options_len)
        } else {
            None
        }
    }

    pub(super) fn current_total_options_len(&self) -> usize {
        let options_len = self.current_options_len();
        if options_len > 0 && self.current_is_other() {
            options_len + 1
        } else {
            options_len
        }
    }

    pub(super) fn current_has_options(&self) -> bool {
        self.current_options_len() > 0
    }

    pub(super) fn current_accepts_freeform(&self) -> bool {
        if self.current_options_len() == 0 {
            return true;
        }
        let Some(other_idx) = self.current_other_index() else {
            return false;
        };
        let selected_idx = self
            .current_answer()
            .and_then(|answer| answer.option_state.selected_idx);
        selected_idx == Some(other_idx)
    }

    pub(super) fn move_selection(&mut self, up: bool) {
        let options_len = self.current_total_options_len();
        if options_len == 0 {
            return;
        }
        let Some(answer) = self.current_answer_mut() else {
            return;
        };
        if up {
            answer.option_state.move_up_wrap(options_len);
        } else {
            answer.option_state.move_down_wrap(options_len);
        }
        answer
            .option_state
            .ensure_visible(options_len, options_len.clamp(1, 6));
    }

    pub(super) fn push_freeform_char(&mut self, ch: char) {
        let Some(answer) = self.current_answer_mut() else {
            return;
        };
        answer.freeform.push(ch);
    }

    pub(super) fn pop_freeform_char(&mut self) {
        let Some(answer) = self.current_answer_mut() else {
            return;
        };
        let _ = answer.freeform.pop();
    }

    pub(super) fn go_next_or_submit(&mut self) {
        if self.question_count() == 0 {
            self.complete = true;
            return;
        }

        if self.current_idx + 1 >= self.question_count() {
            self.submit();
        } else {
            self.current_idx = self.current_idx.saturating_add(1);
        }
    }

    pub(super) fn select_current_option_by_label(&mut self, desired_label: &str) -> bool {
        let idx = self
            .current_question()
            .and_then(|question| question.options.as_ref())
            .and_then(|options| options.iter().position(|opt| opt.label.trim() == desired_label));
        let Some(answer) = self.current_answer_mut() else {
            return false;
        };
        if let Some(idx) = idx {
            answer.option_state.selected_idx = Some(idx);
            return true;
        }
        false
    }

    pub(super) fn option_hit_test(&self, area: Rect, x: u16, y: u16) -> Option<usize> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        if !crate::ui_interaction::contains_point(area, x, y) {
            return None;
        }
        if !self.current_has_options() {
            return None;
        }

        let options_len = self.current_total_options_len();
        if options_len == 0 {
            return None;
        }

        let inner = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        if inner.width == 0 || inner.height == 0 {
            return None;
        }

        let question = self.current_question();
        let prompt = question.map_or("", |q| q.question.as_str());

        let mut content_y = inner.y.saturating_add(2);
        let footer_height = 1u16;
        let available = inner
            .height
            .saturating_sub((content_y - inner.y).saturating_add(footer_height));
        if available == 0 {
            return None;
        }

        let desired_prompt_height = if prompt.trim().is_empty() {
            0u16
        } else {
            let wrapped = textwrap::wrap(prompt, inner.width.max(1) as usize);
            u16::try_from(wrapped.len()).unwrap_or(u16::MAX).clamp(1, 3)
        };

        let (prompt_height, content_height) = if available <= 2 {
            (0, available)
        } else {
            let prompt_budget = available.saturating_sub(2);
            let prompt_height = desired_prompt_height.min(prompt_budget);
            let content_height = available.saturating_sub(prompt_height);
            (prompt_height, content_height)
        };

        content_y = content_y.saturating_add(prompt_height);
        if content_height == 0 {
            return None;
        }

        let content_rect = Rect {
            x: inner.x,
            y: content_y,
            width: inner.width,
            height: content_height,
        };
        if !crate::ui_interaction::contains_point(content_rect, x, y) {
            return None;
        }

        let state = self.current_answer().map(|answer| answer.option_state)?;
        let visible_rows = (content_rect.height as usize).min(options_len).max(1);
        let mut start_idx = state.scroll_top.min(options_len.saturating_sub(1));
        if let Some(sel) = state.selected_idx {
            if sel < start_idx {
                start_idx = sel;
            } else {
                let bottom = start_idx.saturating_add(visible_rows).saturating_sub(1);
                if sel > bottom {
                    start_idx = sel.saturating_add(1).saturating_sub(visible_rows);
                }
            }
        }

        let rel_y = y.saturating_sub(content_rect.y) as usize;
        if rel_y >= visible_rows {
            return None;
        }

        let idx = start_idx.saturating_add(rel_y);
        (idx < options_len).then_some(idx)
    }

    pub(super) fn submit(&mut self) {
        if self.submitting {
            return;
        }

        let mut answers = HashMap::new();
        for (idx, question) in self.questions.iter().enumerate() {
            let Some(answer_state) = self.answers.get(idx) else {
                continue;
            };
            let options = question.options.as_ref().filter(|opts| !opts.is_empty());
            let mut answer_list = Vec::new();

            if let Some(options) = options {
                let selected_idx = answer_state.option_state.selected_idx;
                let other_idx = (question.is_other && !options.is_empty()).then_some(options.len());
                if other_idx.is_some_and(|idx| selected_idx == Some(idx)) {
                    let value = answer_state.freeform.trim_end();
                    answer_list.push(value.to_string());
                } else if let Some(label) = selected_idx
                    .and_then(|i| options.get(i))
                    .map(|opt| opt.label.clone())
                {
                    answer_list.push(label);
                }
            } else {
                let value = answer_state.freeform.trim_end();
                // Preserve the legacy behavior of `request_user_input` composer replies:
                // always provide an answer slot, even when empty.
                answer_list.push(value.to_string());
            }

            answers.insert(
                question.id.clone(),
                RequestUserInputAnswer {
                    answers: answer_list,
                },
            );
        }

        self.app_event_tx.send(AppEvent::RequestUserInputAnswer {
            turn_id: self.turn_id.clone(),
            response: RequestUserInputResponse { answers },
        });
        // Keep the picker visible until the ChatWidget consumes the answer.
        // This prevents a race where the composer becomes active while
        // `pending_request_user_input` is still set.
        self.submitting = true;
    }
}
