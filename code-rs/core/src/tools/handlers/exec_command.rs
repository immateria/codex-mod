use crate::codex::Session;
use crate::codex::{ApprovedCommandPattern, CommandApprovalRequest};
use crate::command_safety::context::CommandSafetyContext;
use crate::protocol::ApprovedCommandMatchKind;
use crate::protocol::AskForApproval;
use crate::protocol::ReviewDecision;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::execute_custom_tool;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::unsupported_tool_call_output;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::ResponseInputItem;
use std::collections::HashMap;
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
        let sub_id = ctx.sub_id.clone();
        let call_id = ctx.call_id.clone();
        let tool_name = inv.tool_name.clone();
        let mgr = sess.exec_command_manager();
        let cwd = sess.get_cwd().to_path_buf();
        let sandbox_policy = sess.get_sandbox_policy().clone();
        let sandbox_policy_cwd = cwd.clone();
        let enforce_managed_network = sess.managed_network_proxy().is_some();

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
                    let wrapper =
                        vec![params.shell.clone(), mode_flag.to_string(), params.cmd.clone()];
                    match code_apply_patch::maybe_parse_apply_patch_verified(&wrapper, &effective_workdir)
                    {
                        code_apply_patch::MaybeApplyPatchVerified::Body(_)
                        | code_apply_patch::MaybeApplyPatchVerified::CorrectnessError(_) => {
                            return unsupported_tool_call_output(
                                &call_id,
                                false,
                                "apply_patch was requested via exec_command. Use the apply_patch tool instead."
                                    .to_string(),
                            );
                        }
                        code_apply_patch::MaybeApplyPatchVerified::ShellParseError(_)
                        | code_apply_patch::MaybeApplyPatchVerified::NotApplyPatch => {}
                    }

                    let sandbox_permissions = params.sandbox_permissions.unwrap_or_default();
                    if sandbox_permissions.requires_escalated_permissions()
                        && params
                            .justification
                            .as_ref()
                            .map(|justification| justification.trim().is_empty())
                            .unwrap_or(true)
                    {
                        return unsupported_tool_call_output(
                            &call_id,
                            false,
                            "sandbox_permissions=require_escalated requires a justification"
                                .to_string(),
                        );
                    }

                    let additional_permissions = sandbox_permissions
                        .uses_additional_permissions()
                        .then(|| params.additional_permissions.clone())
                        .flatten();
                    if sandbox_permissions.uses_additional_permissions()
                        && additional_permissions
                            .as_ref()
                            .map(code_protocol::models::PermissionProfile::is_empty)
                            .unwrap_or(true)
                    {
                        return unsupported_tool_call_output(
                            &call_id,
                            false,
                            "sandbox_permissions=with_additional_permissions requires additional_permissions"
                                .to_string(),
                        );
                    }

                    if sandbox_permissions.requests_sandbox_override()
                        && !matches!(
                            &sandbox_policy,
                            &crate::protocol::SandboxPolicy::DangerFullAccess
                        )
                        && !sess.is_command_approved(&wrapper)
                    {
                        match sess.get_approval_policy() {
                            AskForApproval::Never => {
                                return unsupported_tool_call_output(
                                    &call_id,
                                    false,
                                    "exec_command rejected: sandbox override requires approval but approval policy is set to never"
                                        .to_string(),
                                );
                            }
                            AskForApproval::Reject(config)
                                if config.rejects_sandbox_approval()
                                    || config.rejects_rules_approval() =>
                            {
                                return unsupported_tool_call_output(
                                    &call_id,
                                    false,
                                    "exec_command rejected: approval policy auto-rejected sandbox override"
                                        .to_string(),
                                );
                            }
                            _ => {}
                        }

                        let reason = if sandbox_permissions.requires_escalated_permissions() {
                            "Command requested to run without sandbox restrictions".to_string()
                        } else {
                            "Command requested additional sandbox permissions".to_string()
                        };

                        let rx_approve = sess
                            .request_command_approval(CommandApprovalRequest {
                                sub_id: sub_id.clone(),
                                call_id: call_id.clone(),
                                approval_id: None,
                                command: wrapper.clone(),
                                cwd: effective_workdir.clone(),
                                reason: Some(reason),
                                network_approval_context: None,
                                additional_permissions: additional_permissions.clone(),
                            })
                            .await;
                        let decision = rx_approve.await.unwrap_or_default();
                        match decision {
                            ReviewDecision::Approved => {}
                            ReviewDecision::ApprovedForSession => {
                                sess.add_approved_command(ApprovedCommandPattern::new(
                                    wrapper.clone(),
                                    ApprovedCommandMatchKind::Exact,
                                    None,
                                ));
                            }
                            ReviewDecision::Denied | ReviewDecision::Abort => {
                                return unsupported_tool_call_output(
                                    &call_id,
                                    false,
                                    "exec_command rejected by user".to_string(),
                                );
                            }
                        }
                    }

                    // Dangerous-command gating: exec_command previously bypassed command safety.
                    // Keep behavior minimal and non-regressive by prompting only for commands
                    // classified as dangerous (fork bomb / destructive operations), and honor
                    // session approvals.
                    let command_safety_context = CommandSafetyContext::current().with_command_shell(&wrapper);
                    let wrapper_is_trusted = crate::is_safe_command::is_known_safe_command_with_context_and_rules(
                        &wrapper,
                        command_safety_context,
                        sess.safe_command_rules(),
                    ) || sess.is_command_approved(&wrapper);
                    if !wrapper_is_trusted
                        && sess.dangerous_command_detection_enabled()
                        && crate::is_dangerous_command::command_might_be_dangerous_with_context_and_rules(
                            &wrapper,
                            command_safety_context,
                            sess.dangerous_command_rules(),
                        )
                    {
                        if matches!(sess.get_approval_policy(), AskForApproval::Never) {
                            return unsupported_tool_call_output(
                                &call_id,
                                false,
                                "exec_command rejected: approval policy is set to never, but command is considered dangerous"
                                    .to_string(),
                            );
                        }

                        let rx_approve = sess
                            .request_command_approval(CommandApprovalRequest {
                                sub_id: sub_id.clone(),
                                call_id: call_id.clone(),
                                approval_id: None,
                                command: wrapper.clone(),
                                cwd: effective_workdir.clone(),
                                reason: Some(
                                    "Command flagged as dangerous (possible fork bomb / destructive operation)"
                                        .to_string(),
                                ),
                                network_approval_context: None,
                                additional_permissions,
                            })
                            .await;
                        let decision = rx_approve.await.unwrap_or_default();
                        match decision {
                            ReviewDecision::Approved => {}
                            ReviewDecision::ApprovedForSession => {
                                sess.add_approved_command(ApprovedCommandPattern::new(
                                    wrapper.clone(),
                                    ApprovedCommandMatchKind::Exact,
                                    None,
                                ));
                            }
                            ReviewDecision::Denied | ReviewDecision::Abort => {
                                return unsupported_tool_call_output(
                                    &call_id,
                                    false,
                                    "exec_command rejected by user".to_string(),
                                );
                            }
                        }
                    }

                    let mut env_overrides = HashMap::new();
                    let network_attempt_guard = if let Some(proxy) = sess.managed_network_proxy() {
                        let attempt_id = uuid::Uuid::new_v4().to_string();
                        let network_approval = sess.network_approval();
                        network_approval
                            .register_attempt(
                                attempt_id.clone(),
                                sub_id.clone(),
                                call_id.clone(),
                                wrapper.clone(),
                                effective_workdir.clone(),
                            )
                            .await;
                        proxy.apply_to_env_for_attempt(&mut env_overrides, Some(&attempt_id));
                        Some(crate::network_approval::NetworkAttemptGuard::new(
                            network_approval,
                            attempt_id,
                        ))
                    } else {
                        None
                    };

                    let output = crate::exec_command::result_into_payload(
                        mgr.handle_exec_command_request(
                            params,
                            env_overrides,
                            network_attempt_guard,
                            sandbox_policy,
                            sandbox_policy_cwd,
                            enforce_managed_network,
                        )
                        .await,
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
