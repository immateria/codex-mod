mod model;
mod pane_impl;
mod render;

use code_protocol::request_user_input::RequestUserInputQuestion;
use crate::app_event_sender::AppEventSender;

use crate::components::scroll_state::ScrollState;

#[derive(Debug, Clone)]
struct AnswerState {
    option_state: ScrollState,
    hover_option_idx: Option<usize>,
    freeform: String,
}

pub(crate) struct RequestUserInputView {
    app_event_tx: AppEventSender,
    turn_id: String,
    call_id: String,
    questions: Vec<RequestUserInputQuestion>,
    answers: Vec<AnswerState>,
    current_idx: usize,
    submitting: bool,
    complete: bool,
}
