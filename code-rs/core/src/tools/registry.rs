use crate::codex::Session;
use crate::tools::context::ToolCall;
use crate::tools::context::ToolInvocation;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use std::collections::HashMap;
use std::sync::Arc;

#[async_trait]
pub(crate) trait ToolHandler: Send + Sync {
    async fn handle(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem;
}

pub(crate) struct ToolRegistry {
    handlers: HashMap<&'static str, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub(crate) fn new(handlers: HashMap<&'static str, Arc<dyn ToolHandler>>) -> Self {
        Self { handlers }
    }

    pub(crate) fn handler(&self, name: &str) -> Option<Arc<dyn ToolHandler>> {
        self.handlers.get(name).map(Arc::clone)
    }

    pub(crate) async fn dispatch(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        call: ToolCall,
        ctx: crate::codex::ToolCallCtx,
        attempt_req: u64,
    ) -> ResponseInputItem {
        let tool_name = call.tool_name.clone();
        let outputs_custom = call.payload.outputs_custom();

        let handler = match self.handler(tool_name.as_str()) {
            Some(handler) => handler,
            None => {
                return unsupported_tool_call_output(
                    &ctx.call_id,
                    outputs_custom,
                    format!("unsupported call: {}", tool_name),
                );
            }
        };

        let inv = ToolInvocation {
            ctx,
            tool_name,
            payload: call.payload,
            attempt_req,
        };
        handler.handle(sess, turn_diff_tracker, inv).await
    }
}

pub(crate) fn unsupported_tool_call_output(
    call_id: &str,
    outputs_custom: bool,
    message: String,
) -> ResponseInputItem {
    if outputs_custom {
        return ResponseInputItem::CustomToolCallOutput {
            call_id: call_id.to_string(),
            output: message,
        };
    }

    ResponseInputItem::FunctionCallOutput {
        call_id: call_id.to_string(),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(message),
            success: Some(false),
        },
    }
}
