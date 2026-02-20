use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::protocol::CustomToolCallBeginEvent;
use crate::protocol::CustomToolCallEndEvent;
use crate::protocol::EventMsg;
use code_protocol::models::ResponseInputItem;
use std::time::Instant;

pub(crate) async fn execute_custom_tool<F, Fut>(
    sess: &Session,
    ctx: &ToolCallCtx,
    tool_name: String,
    parameters: Option<serde_json::Value>,
    tool_fn: F,
) -> ResponseInputItem
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ResponseInputItem>,
{
    sess.send_ordered_from_ctx(
        ctx,
        EventMsg::CustomToolCallBegin(CustomToolCallBeginEvent {
            call_id: ctx.call_id.clone(),
            tool_name: tool_name.clone(),
            parameters: parameters.clone(),
        }),
    )
    .await;

    let start = Instant::now();
    let result = tool_fn().await;
    let duration = start.elapsed();

    // Extract success/failure from result. Prefer explicit success flag when available.
    let (success, message) = match &result {
        ResponseInputItem::FunctionCallOutput { output, .. } => {
            let success_flag = output.success;
            let message = output
                .body
                .to_text()
                .unwrap_or_else(|| String::from("Tool completed"));
            (success_flag.unwrap_or(true), message)
        }
        _ => (true, String::from("Tool completed")),
    };

    sess.send_ordered_from_ctx(
        ctx,
        EventMsg::CustomToolCallEnd(CustomToolCallEndEvent {
            call_id: ctx.call_id.clone(),
            tool_name,
            parameters,
            duration,
            result: if success { Ok(message) } else { Err(message) },
        }),
    )
    .await;

    result
}

