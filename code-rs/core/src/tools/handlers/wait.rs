use crate::codex::Session;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::tools::handlers::tool_error;
use async_trait::async_trait;
use code_protocol::models::ResponseInputItem;

pub(crate) struct WaitToolHandler;

#[async_trait]
impl ToolHandler for WaitToolHandler {
    fn scheduling_hints(&self) -> crate::tools::registry::ToolSchedulingHints {
        crate::tools::registry::ToolSchedulingHints::pure_parallel()
    }

    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = inv.payload else {
            return tool_error(inv.ctx.call_id, "wait expects function-call arguments");
        };

        crate::codex::exec_tool::handle_wait(sess, &inv.ctx, arguments).await
    }
}
