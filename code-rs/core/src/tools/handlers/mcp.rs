use crate::codex::Session;
use crate::mcp_tool_call::handle_mcp_tool_call;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::tool_error;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::ResponseInputItem;

pub(crate) struct McpToolHandler;

#[async_trait]
impl ToolHandler for McpToolHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Mcp {
            server,
            tool,
            raw_arguments,
        } = inv.payload
        else {
            return tool_error(inv.ctx.call_id, "MCP handler received unsupported payload");
        };

        let Some(server_id) = crate::mcp::ids::McpServerId::parse(server.as_str()) else {
            return tool_error(inv.ctx.call_id, format!("unsupported MCP server name `{server}`"));
        };

        let tool_label = format!("MCP tool `{server}/{tool}`");
        if let Err(message) = crate::codex::mcp_access::ensure_mcp_server_access_for_turn(
            sess,
            &inv.ctx,
            &server_id,
            server.as_str(),
            tool_label.as_str(),
        )
        .await
        {
            return tool_error(inv.ctx.call_id, message);
        }

        sess.maybe_mark_memories_polluted("mcp_tool_call");
        handle_mcp_tool_call(sess, &inv.ctx, server, tool, raw_arguments).await
    }
}
