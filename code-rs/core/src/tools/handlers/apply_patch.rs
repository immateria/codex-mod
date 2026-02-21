use crate::codex::Session;
use crate::exec::ExecParams;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::unsupported_tool_call_output;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::ResponseInputItem;
use serde::Deserialize;
use std::collections::HashMap;

pub(crate) struct ApplyPatchToolHandler;

#[derive(Debug, Deserialize)]
struct ApplyPatchArgs {
    input: String,
}

#[async_trait]
impl ToolHandler for ApplyPatchToolHandler {
    async fn handle(
        &self,
        sess: &Session,
        turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let outputs_custom = inv.payload.outputs_custom();

        let patch_input = match inv.payload {
            ToolPayload::Function { arguments } => match serde_json::from_str::<ApplyPatchArgs>(&arguments) {
                Ok(args) => args.input,
                Err(err) => {
                    return unsupported_tool_call_output(
                        &inv.ctx.call_id,
                        outputs_custom,
                        format!("invalid apply_patch arguments: {err}"),
                    );
                }
            },
            ToolPayload::Custom { input } => input,
            other => {
                return unsupported_tool_call_output(
                    &inv.ctx.call_id,
                    outputs_custom,
                    format!("apply_patch received unsupported payload: {other:?}"),
                );
            }
        };

        if patch_input.trim().is_empty() {
            return unsupported_tool_call_output(
                &inv.ctx.call_id,
                outputs_custom,
                "apply_patch input must not be empty".to_string(),
            );
        }

        let command = vec!["apply_patch".to_string(), patch_input];
        match sess
            .maybe_parse_apply_patch_verified(&command, sess.get_cwd())
            .await
        {
            code_apply_patch::MaybeApplyPatchVerified::Body(action) => {
                let params = ExecParams {
                    command,
                    cwd: sess.get_cwd().to_path_buf(),
                    timeout_ms: None,
                    env: HashMap::new(),
                    with_escalated_permissions: None,
                    justification: None,
                };

                crate::codex::exec_tool::handle_apply_patch_action(
                    sess,
                    turn_diff_tracker,
                    &inv.ctx,
                    &params,
                    action,
                    inv.attempt_req,
                    outputs_custom,
                )
                .await
            }
            code_apply_patch::MaybeApplyPatchVerified::CorrectnessError(err) => {
                unsupported_tool_call_output(
                    &inv.ctx.call_id,
                    outputs_custom,
                    format!("error: {err:?}"),
                )
            }
            code_apply_patch::MaybeApplyPatchVerified::ShellParseError(err) => {
                unsupported_tool_call_output(
                    &inv.ctx.call_id,
                    outputs_custom,
                    format!("error: {err:?}"),
                )
            }
            code_apply_patch::MaybeApplyPatchVerified::NotApplyPatch => unsupported_tool_call_output(
                &inv.ctx.call_id,
                outputs_custom,
                "not a valid apply_patch payload".to_string(),
            ),
        }
    }
}
