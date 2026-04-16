pub(crate) mod agent;
pub(crate) mod apply_patch;
pub(crate) mod bridge;
pub(crate) mod browser;
pub(crate) mod dynamic;
pub(crate) mod exec_command;
pub(crate) mod gh_run_wait;
pub(crate) mod grep_files;
pub(crate) mod image_view;
pub(crate) mod repl;
pub(crate) mod list_dir;
pub(crate) mod kill;
pub(crate) mod mcp;
pub(crate) mod mcp_resource;
pub(crate) mod plan;
pub(crate) mod read_file;
pub(crate) mod request_user_input;
pub(crate) mod request_permissions;
pub(crate) mod search_tool_bm25;
pub(crate) mod shell;
pub(crate) mod wait;
pub(crate) mod web_fetch;

use code_protocol::models::{
    FunctionCallOutputBody, FunctionCallOutputPayload, ResponseInputItem,
};

/// Build a `FunctionCallOutput` error response (success=false).
pub(crate) fn tool_error(call_id: String, message: impl Into<String>) -> ResponseInputItem {
    ResponseInputItem::FunctionCallOutput {
        call_id,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(message.into()),
            success: Some(false),
        },
    }
}

/// Build a `FunctionCallOutput` success response (success=true).
pub(crate) fn tool_output(call_id: String, text: impl Into<String>) -> ResponseInputItem {
    ResponseInputItem::FunctionCallOutput {
        call_id,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(text.into()),
            success: Some(true),
        },
    }
}
