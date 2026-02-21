use crate::codex::Session;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::execute_custom_tool;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::unsupported_tool_call_output;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::ResponseInputItem;
use std::path::PathBuf;

pub(crate) struct ExecCommandToolHandler;

fn shell_supports_lc(shell: &str) -> bool {
    let lower = shell.to_ascii_lowercase();
    !(lower.contains("powershell") || lower.contains("pwsh"))
}

#[async_trait]
impl ToolHandler for ExecCommandToolHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = &inv.payload else {
            return unsupported_tool_call_output(
                &inv.ctx.call_id,
                inv.payload.outputs_custom(),
                format!("{} expects function-call arguments", inv.tool_name),
            );
        };

        let params_for_event = serde_json::from_str::<serde_json::Value>(arguments).ok();
        let arguments = arguments.clone();
        let ctx = inv.ctx.clone();
        let call_id = ctx.call_id.clone();
        let tool_name = inv.tool_name.clone();
        let mgr = sess.exec_command_manager();
        let cwd = sess.get_cwd().to_path_buf();

        execute_custom_tool(sess, &ctx, tool_name.clone(), params_for_event, move || async move {
            match tool_name.as_str() {
                crate::exec_command::EXEC_COMMAND_TOOL_NAME => {
                    let raw_args: serde_json::Value = match serde_json::from_str(&arguments) {
                        Ok(value) => value,
                        Err(err) => {
                            return unsupported_tool_call_output(
                                &call_id,
                                false,
                                format!("invalid exec_command arguments: {err}"),
                            );
                        }
                    };
                    let shell_was_provided = raw_args.get("shell").is_some();
                    let mut params: crate::exec_command::ExecCommandParams =
                        match serde_json::from_value(raw_args) {
                            Ok(params) => params,
                            Err(err) => {
                                return unsupported_tool_call_output(
                                    &call_id,
                                    false,
                                    format!("invalid exec_command arguments: {err}"),
                                );
                            }
                        };

                    // Default the workdir to the turn/session cwd (tool schema contract).
                    let mut effective_workdir = params
                        .workdir
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(PathBuf::from)
                        .unwrap_or_else(|| cwd.clone());
                    if !effective_workdir.is_absolute() {
                        effective_workdir = cwd.join(&effective_workdir);
                    }
                    params.workdir = Some(effective_workdir.to_string_lossy().to_string());

                    // If shell isn't explicitly set, default to the session shell when it is bash/zsh.
                    if !shell_was_provided {
                        match sess.user_shell() {
                            crate::shell::Shell::Zsh(zsh) => params.shell = zsh.shell_path.clone(),
                            crate::shell::Shell::Bash(bash) => params.shell = bash.shell_path.clone(),
                            _ => {}
                        }
                    }

                    if !shell_supports_lc(&params.shell) {
                        return unsupported_tool_call_output(
                            &call_id,
                            false,
                            format!(
                                "exec_command shell `{}` is not supported (requires -lc/-c semantics). Use /bin/bash or /bin/zsh.",
                                params.shell
                            ),
                        );
                    }

                    // Intercept apply_patch-style commands invoked via exec_command.
                    let mode_flag = if params.login { "-lc" } else { "-c" };
                    let wrapper = vec![params.shell.clone(), mode_flag.to_string(), params.cmd.clone()];
                    match code_apply_patch::maybe_parse_apply_patch_verified(&wrapper, &effective_workdir) {
                        code_apply_patch::MaybeApplyPatchVerified::Body(_)
                        | code_apply_patch::MaybeApplyPatchVerified::CorrectnessError(_) => {
                            return unsupported_tool_call_output(
                                &call_id,
                                false,
                                "apply_patch was requested via exec_command. Use the apply_patch tool instead.".to_string(),
                            );
                        }
                        code_apply_patch::MaybeApplyPatchVerified::ShellParseError(_)
                        | code_apply_patch::MaybeApplyPatchVerified::NotApplyPatch => {}
                    }

                    let output = crate::exec_command::result_into_payload(
                        mgr.handle_exec_command_request(params).await,
                    );
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output,
                    }
                }
                crate::exec_command::WRITE_STDIN_TOOL_NAME => {
                    let params: crate::exec_command::WriteStdinParams =
                        match serde_json::from_str(&arguments) {
                            Ok(params) => params,
                            Err(err) => {
                                return unsupported_tool_call_output(
                                    &call_id,
                                    false,
                                    format!("invalid write_stdin arguments: {err}"),
                                );
                            }
                        };
                    let output =
                        crate::exec_command::result_into_payload(mgr.handle_write_stdin_request(params).await);
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output,
                    }
                }
                other => unsupported_tool_call_output(
                    &call_id,
                    false,
                    format!("unexpected tool name: {other}"),
                ),
            }
        })
        .await
    }
}
