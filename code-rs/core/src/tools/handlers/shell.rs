use crate::codex::Session;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;

pub(crate) struct ShellHandler;

#[async_trait]
impl ToolHandler for ShellHandler {
    async fn handle(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        match inv.payload {
            ToolPayload::Function { arguments } => {
                let params = match crate::codex::exec_tool::parse_container_exec_arguments(
                    arguments,
                    sess,
                    inv.ctx.call_id.as_str(),
                ) {
                    Ok(params) => params,
                    Err(output) => return *output,
                };

                crate::codex::exec_tool::handle_container_exec_with_params(
                    params,
                    sess,
                    turn_diff_tracker,
                    &inv.ctx,
                    inv.attempt_req,
                )
                .await
            }
            ToolPayload::LocalShell { params } => {
                let exec_params = crate::codex::exec_tool::to_exec_params(params, sess);
                crate::codex::exec_tool::handle_container_exec_with_params(
                    exec_params,
                    sess,
                    turn_diff_tracker,
                    &inv.ctx,
                    inv.attempt_req,
                )
                .await
            }
            _ => ResponseInputItem::FunctionCallOutput {
                call_id: inv.ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!(
                        "unsupported shell payload for tool `{}`",
                        inv.tool_name
                    )),
                    success: Some(false),
                },
            },
        }
    }
}
