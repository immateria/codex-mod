use crate::codex::ToolCallCtx;
use code_protocol::models::ShellToolCallParams;

#[derive(Debug, Clone)]
pub(crate) enum ToolPayload {
    Function { arguments: String },
    Custom {
        #[allow(dead_code)]
        input: String,
    },
    LocalShell { params: ShellToolCallParams },
    Mcp {
        server: String,
        tool: String,
        raw_arguments: String,
    },
}

impl ToolPayload {
    pub(crate) fn outputs_custom(&self) -> bool {
        matches!(self, Self::Custom { .. })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ToolCall {
    pub(crate) tool_name: String,
    pub(crate) payload: ToolPayload,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolInvocation {
    pub(crate) ctx: ToolCallCtx,
    pub(crate) tool_name: String,
    pub(crate) payload: ToolPayload,
    pub(crate) attempt_req: u64,
}
