use crate::codex::Session;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use crate::codex::ToolCallCtx;
use crate::protocol::EventMsg;

pub(crate) struct DynamicToolHandler;

#[async_trait]
impl ToolHandler for DynamicToolHandler {
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
                        "dynamic tool expects function-call arguments".to_string(),
                    ),
                    success: Some(false),
                },
            };
        };

        handle_dynamic_tool_call(sess, &inv.ctx, inv.tool_name, arguments).await
    }
}

async fn handle_dynamic_tool_call(
    sess: &Session,
    ctx: &ToolCallCtx,
    tool_name: String,
    arguments: String,
) -> ResponseInputItem {
    let args = if arguments.trim().is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        match serde_json::from_str::<serde_json::Value>(&arguments) {
            Ok(args) => args,
            Err(err) => {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: ctx.call_id.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "invalid dynamic tool arguments: {err}"
                        )),
                        success: Some(false),
                    },
                };
            }
        }
    };

    let rx_response = match sess.register_pending_dynamic_tool(ctx.call_id.clone()) {
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
        EventMsg::DynamicToolCallRequest(code_protocol::dynamic_tools::DynamicToolCallRequest {
            call_id: ctx.call_id.clone(),
            turn_id: ctx.sub_id.clone(),
            tool: tool_name,
            arguments: args,
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
                        "dynamic tool call was cancelled before receiving a response".to_string(),
                    ),
                    success: Some(false),
                },
            };
        }
    };

    ResponseInputItem::FunctionCallOutput {
        call_id: ctx.call_id.clone(),
        output: {
            let content_items = response
                .content_items
                .into_iter()
                .map(code_protocol::models::FunctionCallOutputContentItem::from)
                .collect::<Vec<_>>();
            let mut payload = FunctionCallOutputPayload::from_content_items(content_items);
            payload.success = Some(response.success);
            payload
        },
    }
}
