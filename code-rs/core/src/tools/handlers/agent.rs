use crate::codex::Session;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::tools::handlers::tool_error;
use async_trait::async_trait;
use code_protocol::models::ResponseInputItem;

pub(crate) struct AgentToolHandler;

#[async_trait]
impl ToolHandler for AgentToolHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = inv.payload else {
            return tool_error(inv.ctx.call_id, "agent expects function-call arguments");
        };

        crate::codex::agent_tool_call::handle_agent_tool(sess, &inv.ctx, arguments).await
    }
}
