use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::protocol::EventMsg;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;

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
            return ResponseInputItem::FunctionCallOutput {
                call_id: inv.ctx.call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "request_user_input expects function-call arguments".to_string(),
                    ),
                    success: Some(false),
                },
            };
        };

        handle_request_user_input(sess, &inv.ctx, arguments).await
    }
}

async fn handle_request_user_input(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    use code_protocol::request_user_input::RequestUserInputArgs;
    use code_protocol::request_user_input::RequestUserInputEvent;

    let mut args: RequestUserInputArgs = match serde_json::from_str(&arguments) {
        Ok(args) => args,
        Err(err) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!(
                        "invalid request_user_input arguments: {err}"
                    )),
                    success: Some(false),
                },
            };
        }
    };

    if args.questions.is_empty() {
        return ResponseInputItem::FunctionCallOutput {
            call_id: ctx.call_id.clone(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(
                    "request_user_input requires at least one question".to_string(),
                ),
                success: Some(false),
            },
        };
    }

    let missing_options = args
        .questions
        .iter()
        .any(|question| question.options.as_ref().is_none_or(Vec::is_empty));
    if missing_options {
        return ResponseInputItem::FunctionCallOutput {
            call_id: ctx.call_id.clone(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(
                    "request_user_input requires non-empty options for every question".to_string(),
                ),
                success: Some(false),
            },
        };
    }

    for question in &mut args.questions {
        question.is_other = true;
    }

    let rx_response = match sess.register_pending_user_input(ctx.sub_id.clone()) {
        Ok(rx) => rx,
        Err(err) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(err),
                    success: Some(false),
                },
            };
        }
    };

    sess.send_ordered_from_ctx(
        ctx,
        EventMsg::RequestUserInput(RequestUserInputEvent {
            call_id: ctx.call_id.clone(),
            turn_id: ctx.sub_id.clone(),
            questions: args.questions,
        }),
    )
    .await;

    let response = match rx_response.await {
        Ok(response) => response,
        Err(_) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "request_user_input was cancelled before receiving a response".to_string(),
                    ),
                    success: Some(false),
                },
            };
        }
    };

    let content = match serde_json::to_string(&response) {
        Ok(content) => content,
        Err(err) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!(
                        "failed to serialize request_user_input response: {err}"
                    )),
                    success: Some(false),
                },
            };
        }
    };

    ResponseInputItem::FunctionCallOutput {
        call_id: ctx.call_id.clone(),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(content),
            success: Some(true),
        },
    }
}
