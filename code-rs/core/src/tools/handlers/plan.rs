use crate::codex::Session;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::tools::handlers::tool_error;
use async_trait::async_trait;
use code_protocol::models::ResponseInputItem;

pub(crate) struct PlanHandler;

#[async_trait]
impl ToolHandler for PlanHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = inv.payload else {
            return tool_error(inv.ctx.call_id, "update_plan expects function-call arguments");
        };

        crate::plan_tool::handle_update_plan(sess, &inv.ctx, arguments).await
    }
}

