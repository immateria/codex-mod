use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::protocol::EventMsg;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::tools::handlers::{tool_error, tool_output};
use async_trait::async_trait;
use code_protocol::models::ResponseInputItem;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RequestUserInputToolOption {
    label: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct RequestUserInputToolQuestion {
    id: String,
    header: String,
    question: String,
    #[serde(default = "default_allow_freeform")]
    allow_freeform: bool,
    #[serde(default)]
    allow_multiple: bool,
    #[serde(default)]
    is_secret: bool,
    #[serde(default)]
    options: Option<Vec<RequestUserInputToolOption>>,
}

#[derive(Debug, Deserialize)]
struct RequestUserInputToolArgs {
    questions: Vec<RequestUserInputToolQuestion>,
}

const fn default_allow_freeform() -> bool {
    true
}

pub(crate) struct RequestUserInputHandler;

#[async_trait]
impl ToolHandler for RequestUserInputHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = inv.payload else {
            return tool_error(inv.ctx.call_id, "request_user_input expects function-call arguments");
        };

        handle_request_user_input(sess, &inv.ctx, arguments).await
    }
}

async fn handle_request_user_input(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    use code_protocol::request_user_input::RequestUserInputEvent;

    let args: RequestUserInputToolArgs = match serde_json::from_str(&arguments) {
        Ok(args) => args,
        Err(err) => {
            return tool_error(
                ctx.call_id.clone(),
                format!("invalid request_user_input arguments: {err}"),
            );
        }
    };

    if args.questions.is_empty() {
        return tool_error(
            ctx.call_id.clone(),
            "request_user_input requires at least one question",
        );
    }

    let questions = match normalize_request_user_input_questions(args) {
        Ok(questions) => questions,
        Err(err) => {
            return tool_error(ctx.call_id.clone(), err);
        }
    };

    let rx_response = match sess.register_pending_user_input(ctx.sub_id.clone()) {
        Ok(rx) => rx,
        Err(err) => {
            return tool_error(ctx.call_id.clone(), err);
        }
    };

    sess.send_ordered_from_ctx(
        ctx,
        EventMsg::RequestUserInput(RequestUserInputEvent {
            call_id: ctx.call_id.clone(),
            turn_id: ctx.sub_id.clone(),
            questions,
        }),
    )
    .await;

    let Ok(response) = rx_response.await else {
        return tool_error(
            ctx.call_id.clone(),
            "request_user_input was cancelled before receiving a response",
        );
    };

    let content = match serde_json::to_string(&response) {
        Ok(content) => content,
        Err(err) => {
            return tool_error(
                ctx.call_id.clone(),
                format!("failed to serialize request_user_input response: {err}"),
            );
        }
    };

    tool_output(ctx.call_id.clone(), content)
}

fn normalize_request_user_input_questions(
    args: RequestUserInputToolArgs,
) -> Result<Vec<code_protocol::request_user_input::RequestUserInputQuestion>, String> {
    args.questions
        .into_iter()
        .map(|question| {
            let options = question
                .options
                .filter(|options| !options.is_empty())
                .map(|options| {
                    options
                        .into_iter()
                        .map(|option| code_protocol::request_user_input::RequestUserInputQuestionOption {
                            label: option.label,
                            description: option.description,
                        })
                        .collect::<Vec<_>>()
                });
            let has_options = options.as_ref().is_some_and(|options| !options.is_empty());
            if question.allow_multiple && !has_options {
                return Err(format!(
                    "request_user_input question `{}` enables allow_multiple but has no options",
                    question.id
                ));
            }
            if !has_options && !question.allow_freeform {
                return Err(format!(
                    "request_user_input question `{}` must provide options or set allow_freeform",
                    question.id
                ));
            }
            Ok(code_protocol::request_user_input::RequestUserInputQuestion {
                id: question.id,
                header: question.header,
                question: question.question,
                is_other: has_options && question.allow_freeform,
                is_secret: question.is_secret,
                allow_multiple: question.allow_multiple,
                options,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        RequestUserInputToolArgs,
        normalize_request_user_input_questions,
    };

    #[test]
    fn normalize_request_user_input_questions_supports_freeform_only_questions() {
        let args: RequestUserInputToolArgs = serde_json::from_value(serde_json::json!({
            "questions": [{
                "id": "display_name",
                "header": "Name",
                "question": "Type a display name"
            }]
        }))
        .expect("valid args");

        let questions = normalize_request_user_input_questions(args).expect("normalized questions");
        assert_eq!(questions.len(), 1);
        assert!(questions[0].options.is_none());
        assert!(!questions[0].allow_multiple);
        assert!(!questions[0].is_other);
    }

    #[test]
    fn normalize_request_user_input_questions_maps_multiselect_and_disables_other() {
        let args: RequestUserInputToolArgs = serde_json::from_value(serde_json::json!({
            "questions": [{
                "id": "terminals",
                "header": "Terminals",
                "question": "Select supported terminals",
                "allow_freeform": false,
                "allow_multiple": true,
                "options": [
                    { "label": "Termux", "description": "Android shell" },
                    { "label": "WezTerm", "description": "Desktop terminal" }
                ]
            }]
        }))
        .expect("valid args");

        let questions = normalize_request_user_input_questions(args).expect("normalized questions");
        assert_eq!(questions.len(), 1);
        assert!(questions[0].allow_multiple);
        assert!(!questions[0].is_other);
        assert_eq!(questions[0].options.as_ref().map_or(0, Vec::len), 2);
    }
}
