use super::*;
use super::fs_utils::{ensure_agent_dir, write_agent_file};
use super::streaming::AgentTask;
use super::truncation::truncate_middle_bytes;
use crate::tools::events::execute_custom_tool;
use code_protocol::models::FunctionCallOutputBody;

const AGENT_PREVIEW_MAX_BYTES: usize = 32 * 1024; // 32 KiB

fn preview_first_n_lines(s: &str, n: usize) -> (String, usize) {
    let total_lines = s.lines().count();
    let mut preview = s.lines().take(n).collect::<Vec<_>>().join("\n");

    let (maybe_truncated, was_truncated, _, _) =
        truncate_middle_bytes(&preview, AGENT_PREVIEW_MAX_BYTES);
    if was_truncated {
        preview = maybe_truncated;
        preview.push_str(&format!(
            "\n…preview truncated to roughly {AGENT_PREVIEW_MAX_BYTES} bytes…"
        ));
    } else {
        preview = maybe_truncated;
    }

    if total_lines > n {
        if !preview.ends_with('\n') {
            preview.push('\n');
        }
        preview.push_str("…additional lines omitted…");
    }

    (preview, total_lines)
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn truncates_excessively_long_single_line() {
        let input = "x".repeat(AGENT_PREVIEW_MAX_BYTES + 1024);
        let (preview, total_lines) = preview_first_n_lines(&input, 500);
        assert_eq!(total_lines, 1);
        assert!(preview.contains("…truncated…"));
        assert!(preview.contains("preview truncated to roughly"));
    }

    #[test]
    fn notes_when_additional_lines_omitted() {
        let input = (0..600)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (preview, total_lines) = preview_first_n_lines(&input, 500);
        assert_eq!(total_lines, 600);
        assert!(preview.contains("…additional lines omitted…"));
        assert!(!preview.contains("preview truncated to roughly"));
    }
}

fn resolve_agent_read_only(
    write: Option<bool>,
    read_only: Option<bool>,
    config: Option<&crate::config_types::AgentConfig>,
) -> bool {
    if let Some(flag) = write {
        return !flag;
    }
    if let Some(flag) = read_only {
        return flag;
    }
    config.map(|c| c.read_only).unwrap_or(false)
}

#[cfg(test)]
mod resolve_read_only_tests {
    use super::*;
    use crate::config_types::AgentConfig;

    fn make_config(read_only: bool) -> AgentConfig {
        AgentConfig {
            name: "test".into(),
            command: "test".into(),
            args: Vec::new(),
            read_only,
            enabled: true,
            description: None,
            env: None,
            args_read_only: None,
            args_write: None,
            instructions: None,
        }
    }

    #[test]
    fn explicit_write_overrides_config_read_only() {
        let cfg = make_config(true);
        assert!(
            !resolve_agent_read_only(Some(true), None, Some(&cfg)),
            "write=true should allow writes even when config prefers read-only"
        );
    }

    #[test]
    fn explicit_read_only_flag_takes_precedence() {
        let cfg = make_config(false);
        assert!(
            resolve_agent_read_only(None, Some(true), Some(&cfg)),
            "read_only=true should force read-only even when config allows writes"
        );
        assert!(
            resolve_agent_read_only(Some(false), None, Some(&cfg)),
            "write=false should force read-only"
        );
    }

    #[test]
    fn falls_back_to_config_when_request_absent() {
        let cfg = make_config(true);
        assert!(resolve_agent_read_only(None, None, Some(&cfg)));
    }

    #[test]
    fn defaults_to_false_without_config() {
        assert!(!resolve_agent_read_only(None, None, None));
    }
}

#[cfg(test)]
mod resolve_agent_command_for_check_tests {
    use super::resolve_agent_command_for_check;

    #[test]
    fn external_models_use_cli_for_command_checks() {
        let (cmd, is_builtin) = resolve_agent_command_for_check("claude-opus-4.6", None);
        assert_eq!(cmd, "claude");
        assert!(!is_builtin, "Claude should not be treated as a built-in family");
    }
}


fn agent_tool_failure(ctx: &ToolCallCtx, message: impl Into<String>) -> ResponseInputItem {
    ResponseInputItem::FunctionCallOutput {
        call_id: ctx.call_id.clone(),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(message.into()),
            success: Some(false),
        },
    }
}

pub(crate) async fn handle_agent_tool(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    let parsed = serde_json::from_str::<AgentToolRequest>(&arguments);
    let mut req = match parsed {
        Ok(req) => req,
        Err(e) => {
            return agent_tool_failure(ctx, format!("Invalid agent arguments: {e}"));
        }
    };

    let action = req.action.to_ascii_lowercase();
    match action.as_str() {
        "create" => {
            let mut create_opts = match req.create.take() {
                Some(opts) => opts,
                None => {
                    return agent_tool_failure(
                        ctx,
                        "action=create requires a 'create' object",
                    );
                }
            };

            let task = match create_opts.task.take() {
                Some(task) if !task.trim().is_empty() => task,
                _ => {
                    return agent_tool_failure(
                        ctx,
                        "action=create requires a non-empty 'create.task' field",
                    );
                }
            };

            let models = std::mem::take(&mut create_opts.models);
            let context = create_opts.context.take();
            let output = create_opts.output.take();
            let files = create_opts.files.take();
            let write = create_opts.write.take();
            let read_only = create_opts.read_only.take();
            let mut normalized_name = normalize_agent_name(create_opts.name.take());
            if normalized_name.is_none() {
                normalized_name = derive_agent_name_from_task(&task);
            }

            let run_params = RunAgentParams {
                task: task.clone(),
                models: models.clone(),
                context: context.clone(),
                output: output.clone(),
                files: files.clone(),
                write,
                read_only,
                name: normalized_name.clone(),
            };

            let mut create_event = serde_json::Map::new();
            create_event.insert("task".to_string(), serde_json::Value::String(task));
            if !models.is_empty() {
                create_event.insert(
                    "models".to_string(),
                    serde_json::Value::Array(
                        models
                            .iter()
                            .cloned()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
            if let Some(ref ctx_str) = context
                && !ctx_str.is_empty() {
                    create_event.insert("context".to_string(), serde_json::Value::String(ctx_str.clone()));
                }
            if let Some(ref output_str) = output
                && !output_str.is_empty() {
                    create_event.insert("output".to_string(), serde_json::Value::String(output_str.clone()));
                }
            if let Some(ref files_vec) = files
                && !files_vec.is_empty() {
                    create_event.insert(
                        "files".to_string(),
                        serde_json::Value::Array(
                            files_vec
                                .iter()
                                .cloned()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    );
                }
            if let Some(flag) = write {
                create_event.insert("write".to_string(), serde_json::Value::Bool(flag));
            }
            if let Some(flag) = read_only {
                create_event.insert("read_only".to_string(), serde_json::Value::Bool(flag));
            }
            if let Some(ref name_str) = normalized_name
                && !name_str.is_empty() {
                    create_event.insert("name".to_string(), serde_json::Value::String(name_str.clone()));
                }

            let mut event_root = serde_json::Map::new();
            event_root.insert("action".to_string(), serde_json::Value::String("create".to_string()));
            event_root.insert("create".to_string(), serde_json::Value::Object(create_event));
            let event_payload = serde_json::Value::Object(event_root);

            match serde_json::to_string(&run_params) {
                Ok(json) => handle_run_agent(sess, ctx, json, event_payload).await,
                Err(e) => agent_tool_failure(ctx, format!("Failed to encode create arguments: {e}")),
            }
        }
        "status" => {
            let mut status_opts = match req.status.take() {
                Some(opts) => opts,
                None => {
                    return agent_tool_failure(
                        ctx,
                        "action=status requires a 'status' object",
                    );
                }
            };
            let agent_id = match status_opts.agent_id.take() {
                Some(id) if !id.trim().is_empty() => id,
                _ => {
                    return agent_tool_failure(
                        ctx,
                        "action=status requires 'status.agent_id'",
                    );
                }
            };
            let batch_id = match status_opts.batch_id.take() {
                Some(batch) if !batch.trim().is_empty() => batch,
                _ => {
                    return agent_tool_failure(
                        ctx,
                        "action=status requires 'status.batch_id'",
                    );
                }
            };
            let params = CheckAgentStatusParams {
                agent_id: agent_id.clone(),
                batch_id: batch_id.clone(),
            };
            let mut status_event = serde_json::Map::new();
            status_event.insert("agent_id".to_string(), serde_json::Value::String(agent_id));
            status_event.insert("batch_id".to_string(), serde_json::Value::String(batch_id));
            let mut status_event_root = serde_json::Map::new();
            status_event_root.insert("action".to_string(), serde_json::Value::String("status".to_string()));
            status_event_root.insert("status".to_string(), serde_json::Value::Object(status_event));
            let status_event_payload = serde_json::Value::Object(status_event_root);
            match serde_json::to_string(&params) {
                Ok(json) => handle_check_agent_status(sess, ctx, json, status_event_payload).await,
                Err(e) => agent_tool_failure(ctx, format!("Failed to encode status arguments: {e}")),
            }
        }
        "result" => {
            let mut result_opts = match req.result.take() {
                Some(opts) => opts,
                None => {
                    return agent_tool_failure(
                        ctx,
                        "action=result requires a 'result' object",
                    );
                }
            };
            let agent_id = match result_opts.agent_id.take() {
                Some(id) if !id.trim().is_empty() => id,
                _ => {
                    return agent_tool_failure(
                        ctx,
                        "action=result requires 'result.agent_id'",
                    );
                }
            };
            let batch_id = match result_opts.batch_id.take() {
                Some(batch) if !batch.trim().is_empty() => batch,
                _ => {
                    return agent_tool_failure(
                        ctx,
                        "action=result requires 'result.batch_id'",
                    );
                }
            };
            let params = GetAgentResultParams {
                agent_id: agent_id.clone(),
                batch_id: batch_id.clone(),
            };
            let mut result_event = serde_json::Map::new();
            result_event.insert("agent_id".to_string(), serde_json::Value::String(agent_id));
            result_event.insert("batch_id".to_string(), serde_json::Value::String(batch_id));
            let mut result_event_root = serde_json::Map::new();
            result_event_root.insert("action".to_string(), serde_json::Value::String("result".to_string()));
            result_event_root.insert("result".to_string(), serde_json::Value::Object(result_event));
            let result_event_payload = serde_json::Value::Object(result_event_root);
            match serde_json::to_string(&params) {
                Ok(json) => handle_get_agent_result(sess, ctx, json, result_event_payload).await,
                Err(e) => agent_tool_failure(ctx, format!("Failed to encode result arguments: {e}")),
            }
        }
        "cancel" => {
            let mut cancel_opts = match req.cancel.take() {
                Some(opts) => opts,
                None => {
                    return agent_tool_failure(
                        ctx,
                        "action=cancel requires a 'cancel' object",
                    );
                }
            };
            let cancel_agent_id = cancel_opts.agent_id.clone();
            let cancel_batch_id = match cancel_opts.batch_id.take() {
                Some(batch) if !batch.trim().is_empty() => batch,
                _ => {
                    return agent_tool_failure(
                        ctx,
                        "action=cancel requires 'cancel.batch_id'",
                    );
                }
            };
            let params = CancelAgentParams {
                agent_id: cancel_opts.agent_id.take(),
                batch_id: Some(cancel_batch_id.clone()),
            };
            let mut cancel_event = serde_json::Map::new();
            if let Some(id) = cancel_agent_id {
                cancel_event.insert("agent_id".to_string(), serde_json::Value::String(id));
            }
            cancel_event.insert("batch_id".to_string(), serde_json::Value::String(cancel_batch_id));
            let mut cancel_event_root = serde_json::Map::new();
            cancel_event_root.insert("action".to_string(), serde_json::Value::String("cancel".to_string()));
            cancel_event_root.insert("cancel".to_string(), serde_json::Value::Object(cancel_event));
            let cancel_event_payload = serde_json::Value::Object(cancel_event_root);
            match serde_json::to_string(&params) {
                Ok(json) => handle_cancel_agent(sess, ctx, json, cancel_event_payload).await,
                Err(e) => agent_tool_failure(ctx, format!("Failed to encode cancel arguments: {e}")),
            }
        }
        "wait" => {
            let mut wait_opts = match req.wait.take() {
                Some(opts) => opts,
                None => {
                    return agent_tool_failure(
                        ctx,
                        "action=wait requires a 'wait' object",
                    );
                }
            };
            let wait_agent_id = wait_opts.agent_id.clone();
            let wait_batch_id = match wait_opts.batch_id.take() {
                Some(batch) if !batch.trim().is_empty() => batch,
                _ => {
                    return agent_tool_failure(
                        ctx,
                        "action=wait requires 'wait.batch_id'",
                    );
                }
            };
            let wait_timeout = wait_opts.timeout_seconds;
            let wait_return_all = wait_opts.return_all;
            let params = WaitForAgentParams {
                agent_id: wait_opts.agent_id.take(),
                batch_id: Some(wait_batch_id.clone()),
                timeout_seconds: wait_timeout,
                return_all: wait_return_all,
            };
            let mut wait_event = serde_json::Map::new();
            if let Some(id) = wait_agent_id {
                wait_event.insert("agent_id".to_string(), serde_json::Value::String(id));
            }
            wait_event.insert("batch_id".to_string(), serde_json::Value::String(wait_batch_id));
            if let Some(timeout) = wait_timeout {
                wait_event.insert("timeout_seconds".to_string(), serde_json::Value::from(timeout));
            }
            if let Some(return_all) = wait_return_all {
                wait_event.insert("return_all".to_string(), serde_json::Value::Bool(return_all));
            }
            let mut wait_event_root = serde_json::Map::new();
            wait_event_root.insert("action".to_string(), serde_json::Value::String("wait".to_string()));
            wait_event_root.insert("wait".to_string(), serde_json::Value::Object(wait_event));
            let wait_event_payload = serde_json::Value::Object(wait_event_root);
            match serde_json::to_string(&params) {
                Ok(json) => handle_wait_for_agent(sess, ctx, json, wait_event_payload).await,
                Err(e) => agent_tool_failure(ctx, format!("Failed to encode wait arguments: {e}")),
            }
        }
        "list" => {
            let mut list_opts = match req.list.take() {
                Some(opts) => opts,
                None => {
                    return agent_tool_failure(
                        ctx,
                        "action=list requires a 'list' object",
                    );
                }
            };
            let status_filter = list_opts.status_filter.take();
            let batch_id = match list_opts.batch_id.take() {
                Some(batch) if !batch.trim().is_empty() => batch,
                _ => {
                    return agent_tool_failure(
                        ctx,
                        "action=list requires 'list.batch_id'",
                    );
                }
            };
            let recent_only = list_opts.recent_only;
            let params = ListAgentsParams {
                status_filter: status_filter.clone(),
                batch_id: Some(batch_id.clone()),
                recent_only,
            };
            let mut list_event = serde_json::Map::new();
            if let Some(ref status) = status_filter
                && !status.is_empty() {
                    list_event.insert("status_filter".to_string(), serde_json::Value::String(status.clone()));
                }
            list_event.insert("batch_id".to_string(), serde_json::Value::String(batch_id));
            if let Some(recent) = recent_only {
                list_event.insert("recent_only".to_string(), serde_json::Value::Bool(recent));
            }
            let mut list_event_root = serde_json::Map::new();
            list_event_root.insert("action".to_string(), serde_json::Value::String("list".to_string()));
            list_event_root.insert("list".to_string(), serde_json::Value::Object(list_event));
            let list_event_payload = serde_json::Value::Object(list_event_root);
            match serde_json::to_string(&params) {
                Ok(json) => handle_list_agents(sess, ctx, json, list_event_payload).await,
                Err(e) => agent_tool_failure(ctx, format!("Failed to encode list arguments: {e}")),
            }
        }
        other => agent_tool_failure(ctx, format!("Unsupported agent action: {other}")),
    }
}

fn resolve_agent_command_for_check(
    model: &str,
    cfg: Option<&crate::config_types::AgentConfig>,
) -> (String, bool) {
    let spec = agent_model_spec(model)
        .or_else(|| cfg.and_then(|c| agent_model_spec(&c.name)))
        .or_else(|| cfg.and_then(|c| agent_model_spec(&c.command)));

    let cfg_trimmed = cfg.map(|c| {
        let (base, _) = split_command_and_args(&c.command);
        let trimmed = base.trim();
        if trimmed.is_empty() {
            c.command.trim().to_string()
        } else {
            trimmed.to_string()
        }
    });

    if let Some(spec) = spec {
        let is_builtin_family = matches!(spec.family, "code" | "codex" | "cloud");
        let uses_default_cli = cfg_trimmed
            .as_ref()
            .map(|cmd| cmd.is_empty() || cmd.eq_ignore_ascii_case(spec.cli))
            .unwrap_or(true);

        if uses_default_cli {
            return (spec.cli.to_string(), is_builtin_family);
        }
    }

    if let Some(cmd) = cfg_trimmed
        && !cmd.is_empty() {
            return (cmd, false);
        }

    let m = model.to_lowercase();
    match m.as_str() {
        "code" | "codex" | "cloud" => ("coder".to_string(), true),
        "claude" => ("claude".to_string(), false),
        "gemini" => ("gemini".to_string(), false),
        "qwen" => ("qwen".to_string(), false),
        other => (other.to_string(), false),
    }
}

pub(crate) async fn handle_run_agent(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
    event_payload: serde_json::Value,
) -> ResponseInputItem {
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    let generated_batch_id = Uuid::new_v4().to_string();
    let payload_with_batch = match event_payload {
        serde_json::Value::Object(mut map) => {
            map.insert(
                "batch_id".to_string(),
                serde_json::Value::String(generated_batch_id.clone()),
            );
            serde_json::Value::Object(map)
        }
        other => other,
    };
    let closure_batch_id = generated_batch_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent".to_string(),
        Some(payload_with_batch),
        move || async move {
            let batch_id = closure_batch_id.clone();
    match serde_json::from_str::<RunAgentParams>(&arguments_clone) {
        Ok(mut params) => {
            let trimmed_task = params.task.trim().to_string();
            let word_count = trimmed_task
                .split_whitespace()
                .filter(|segment| !segment.is_empty())
                .count();

            if trimmed_task.is_empty() || word_count < 4 {
                let guidance = format!(
                    "⚠️ Agent prompt too short: give the manager more context (at least a full sentence) before running agents. Current prompt: \"{trimmed_task}\"."
                );
                let req = sess.current_request_ordinal();
                let order = sess.background_order_for_ctx(ctx, req);
                sess
                    .notify_background_event_with_order(&ctx.sub_id, order, guidance.clone())
                    .await;

                let response = serde_json::json!({
                    "status": "blocked",
                    "reason": "prompt_too_short",
                    "message": guidance,
                });
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(response.to_string()),
                        success: Some(false),
                    },
                };
            }

            let mut manager = AGENT_MANAGER.write().await;
            let mut agent_name = params.name.clone();
            if agent_name.is_none()
                && let Some(fallback) = derive_agent_name_from_task(trimmed_task.as_str()) {
                    agent_name = Some(fallback.clone());
                    params.name = Some(fallback);
                }

            // Collect requested models from the `models` field.
            let explicit_models = params.models.iter().any(|model| !model.trim().is_empty());
            let raw_models: Vec<String> = params.models.clone();

            // Split comma-delimited strings, trim whitespace, and deduplicate case-insensitively.
            let mut seen_models = HashSet::new();
            let mut models: Vec<String> = Vec::new();
            for entry in raw_models {
                for candidate in entry.split(',') {
                    let trimmed = candidate.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let dedupe_key = trimmed.to_lowercase();
                    if seen_models.insert(dedupe_key) {
                        models.push(trimmed.to_string());
                    }
                }
            }

            if models.is_empty() {
                if sess.tools_config.agent_model_allowed_values.is_empty() {
                    models.push("code".to_string());
                } else {
                    models.extend(
                        sess
                            .tools_config
                            .agent_model_allowed_values
                            .iter()
                            .cloned(),
                    );
                }
            }

            models.sort_by_key(|a| a.to_ascii_lowercase());
            models.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

            // Helper: PATH lookup to determine if a command exists.
            fn command_exists(cmd: &str) -> bool {
                // Absolute/relative path with separators: verify it points to a file.
                if cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') || cmd.contains('\\') {
                    return std::fs::metadata(cmd).map(|m| m.is_file()).unwrap_or(false);
                }

                #[cfg(target_os = "windows")]
                {
                    return which::which(cmd).map(|p| p.is_file()).unwrap_or(false);
                }

                #[cfg(not(target_os = "windows"))]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let Some(path_os) = std::env::var_os("PATH") else { return false; };
                    for dir in std::env::split_paths(&path_os) {
                        if dir.as_os_str().is_empty() { continue; }
                        let candidate = dir.join(cmd);
                        if let Ok(meta) = std::fs::metadata(&candidate)
                            && meta.is_file() {
                                let mode = meta.permissions().mode();
                                if mode & 0o111 != 0 { return true; }
                            }
                    }
                    false
                }
            }

            let multi_model = models.len() > 1;
            let display_label_for = |model: &str| -> String {
                agent_name
                    .as_ref()
                    .and_then(|value| {
                        if value.is_empty() {
                            None
                        } else if multi_model {
                            Some(format!("{value} ({model})"))
                        } else {
                            Some(value.to_string())
                        }
                    })
                    .unwrap_or_else(|| model.to_string())
            };

            let mut agent_ids = Vec::new();
            let mut agent_labels: Vec<(String, String)> = Vec::new();
            let mut skipped: Vec<String> = Vec::new();
            for model in models {
                let model_key = model.to_lowercase();
                // Check if this model is configured and enabled
                let agent_config = sess.agents.iter().find(|a| {
                    a.name.to_lowercase() == model_key
                        || a.command.to_lowercase() == model_key
                });

                if let Some(config) = agent_config {
                    if !config.enabled {
                        continue; // Skip disabled agents
                    }

                    let (cmd_to_check, is_builtin) =
                        resolve_agent_command_for_check(&model, Some(config));
                    if !is_builtin && !command_exists(&cmd_to_check) {
                        skipped.push(format!("{model} (missing: {cmd_to_check})"));
                        continue;
                    }

                    // Respect explicit read_only flag from the caller; otherwise fall back to the config default.
                    let read_only = resolve_agent_read_only(
                        params.write,
                        params.read_only,
                        Some(config),
                    );

                    let agent_id = manager
                        .create_agent_with_config(
                            crate::agent_tool::AgentCreateRequest {
                                model: model.clone(),
                                name: agent_name.clone(),
                                prompt: params.task.clone(),
                                context: params.context.clone(),
                                output_goal: params.output.clone(),
                                files: params.files.clone().unwrap_or_default(),
                                read_only,
                                batch_id: Some(batch_id.clone()),
                                config: None,
                                worktree_branch: None,
                                worktree_base: None,
                                source_kind: None,
                                reasoning_effort: sess.model_reasoning_effort.into(),
                            },
                            config.clone(),
                        )
                        .await;
                    agent_ids.push(agent_id);
                    let label = display_label_for(&model);
                    agent_labels.push((agent_ids.last().cloned().unwrap(), label));
                } else {
                    // Use default configuration for unknown agents
                    let (cmd_to_check, is_builtin) = resolve_agent_command_for_check(&model, None);
                    if !is_builtin && !command_exists(&cmd_to_check) {
                        skipped.push(format!("{model} (missing: {cmd_to_check})"));
                        continue;
                    }
                    let read_only = resolve_agent_read_only(params.write, params.read_only, None);
                    let agent_id = manager
                        .create_agent(crate::agent_tool::AgentCreateRequest {
                            model: model.clone(),
                            name: agent_name.clone(),
                            prompt: params.task.clone(),
                            context: params.context.clone(),
                            output_goal: params.output.clone(),
                            files: params.files.clone().unwrap_or_default(),
                            read_only,
                            batch_id: Some(batch_id.clone()),
                            config: None,
                            worktree_branch: None,
                            worktree_base: None,
                            source_kind: None,
                            reasoning_effort: sess.model_reasoning_effort.into(),
                        })
                        .await;
                    agent_ids.push(agent_id);
                    let label = display_label_for(&model);
                    agent_labels.push((agent_ids.last().cloned().unwrap(), label));
                }
            }

            // If nothing runnable remains, only fall back to a built‑in Codex agent when
            // the caller did not explicitly request models.
            if agent_ids.is_empty() {
                if explicit_models {
                    let mut response_map = serde_json::Map::new();
                    response_map.insert(
                        "batch_id".to_string(),
                        serde_json::Value::String(batch_id.clone()),
                    );
                    response_map.insert(
                        "status".to_string(),
                        serde_json::Value::String("failed".to_string()),
                    );
                    let message = if skipped.is_empty() {
                        "No runnable agents matched the requested models.".to_string()
                    } else {
                        format!(
                            "No runnable agents matched the requested models. Skipped: {}",
                            skipped.join(", ")
                        )
                    };
                    response_map.insert(
                        "message".to_string(),
                        serde_json::Value::String(message),
                    );
                    response_map.insert(
                        "skipped".to_string(),
                        if skipped.is_empty() {
                            serde_json::Value::Null
                        } else {
                            serde_json::Value::Array(
                                skipped
                                    .iter()
                                    .cloned()
                                    .map(serde_json::Value::String)
                                    .collect(),
                            )
                        },
                    );
                    let response = serde_json::Value::Object(response_map);
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(response.to_string()),
                            success: Some(false),
                        },
                    };
                }

                let read_only = resolve_agent_read_only(params.write, params.read_only, None);
                let agent_id = manager
                    .create_agent(crate::agent_tool::AgentCreateRequest {
                        model: "code".to_string(),
                        name: agent_name.clone(),
                        prompt: params.task.clone(),
                        context: params.context.clone(),
                        output_goal: params.output.clone(),
                        files: params.files.clone().unwrap_or_default(),
                        read_only,
                        batch_id: Some(batch_id.clone()),
                        config: None,
                        worktree_branch: None,
                        worktree_base: None,
                        source_kind: None,
                        reasoning_effort: sess.model_reasoning_effort.into(),
                    })
                    .await;
                agent_ids.push(agent_id);
                let label = display_label_for("code");
                agent_labels.push((agent_ids.last().cloned().unwrap(), label));
            }

            // Send agent status update event
            drop(manager); // Release the write lock first
            if !agent_ids.is_empty() {
                send_agent_status_update(sess).await;
            }

            let launch_hint = if agent_ids.len() > 1 {
                let short_batch = short_id(&batch_id);
                let agent_phrase = agent_labels
                    .iter()
                    .map(|(id, label)| format!("{} [{}]", short_id(id), label))
                    .collect::<Vec<_>>()
                    .join(", ");
                let first_agent = agent_labels
                    .first()
                    .map(|(id, _)| id.as_str())
                    .unwrap_or(batch_id.as_str());
                format!(
                    "🤖 Agent batch {short_batch} started: {agent_phrase}.\nUse `agent {{\"action\":\"wait\",\"wait\":{{\"batch_id\":\"{batch_id}\",\"return_all\":true}}}}` to wait for all agents, then `agent {{\"action\":\"result\",\"result\":{{\"agent_id\":\"{first_agent}\"}}}}` for a detailed report.",
                )
            } else {
                let (single_id, single_model) = agent_labels
                    .first()
                    .map(|(id, model)| (id.as_str(), model.as_str()))
                    .unwrap();
                let short_batch = short_id(&batch_id);
                format!(
                    "🤖 Agent batch {short_batch} started with {single_model}. Use `agent {{\"action\":\"wait\",\"wait\":{{\"batch_id\":\"{batch_id}\",\"return_all\":true}}}}` to follow progress, or `agent {{\"action\":\"result\",\"result\":{{\"agent_id\":\"{single_id}\"}}}}` when it finishes.",
                )
            };

            let mut response_map = serde_json::Map::new();
            response_map.insert(
                "batch_id".to_string(),
                serde_json::Value::String(batch_id.clone()),
            );
            response_map.insert(
                "agent_ids".to_string(),
                serde_json::Value::Array(
                    agent_ids
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            response_map.insert(
                "status".to_string(),
                serde_json::Value::String("started".to_string()),
            );
            let message = if agent_ids.len() > 1 {
                format!("Started {} agents", agent_labels.len())
            } else {
                "Agent started successfully".to_string()
            };
            response_map.insert(
                "message".to_string(),
                serde_json::Value::String(message),
            );
            response_map.insert(
                "next_steps".to_string(),
                serde_json::Value::String(launch_hint),
            );
            if agent_ids.len() == 1
                && let Some(first) = agent_ids.first() {
                    response_map.insert(
                        "agent_id".to_string(),
                        serde_json::Value::String(first.clone()),
                    );
                }
            if skipped.is_empty() {
                response_map.insert("skipped".to_string(), serde_json::Value::Null);
            } else {
                response_map.insert(
                    "skipped".to_string(),
                    serde_json::Value::Array(
                        skipped
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
            let response = serde_json::Value::Object(response_map);

            ResponseInputItem::FunctionCallOutput {
                call_id: call_id_clone,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(response.to_string()),
                    success: Some(true),
                },
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(format!("Invalid agent arguments: {e}")),
                success: Some(false),
            },
        },
    }
        }
    ).await
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn derive_agent_name_from_task(task: &str) -> Option<String> {
    let trimmed = task.trim();
    if trimmed.is_empty() {
        return None;
    }

    let first_clause = trimmed
        .split(['.', '!', '?', '\n'])
        .find(|part| !part.trim().is_empty())
        .unwrap_or(trimmed)
        .trim();

    let words: Vec<&str> = first_clause.split_whitespace().take(5).collect();
    if words.is_empty() {
        return None;
    }

    normalize_agent_name(Some(words.join(" ")))
}

async fn handle_check_agent_status(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
    event_payload: serde_json::Value,
) -> ResponseInputItem {
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent".to_string(),
        Some(event_payload),
        || async move {
    match serde_json::from_str::<CheckAgentStatusParams>(&arguments_clone) {
        Ok(params) => {
            let manager = AGENT_MANAGER.read().await;

            if let Some(agent) = manager.get_agent(&params.agent_id) {
                match agent.batch_id.as_deref() {
                    Some(batch) if batch == params.batch_id => {}
                    _ => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Agent {} does not belong to batch {}",
                                    params.agent_id, params.batch_id
                                )),
                                success: Some(false),
                            },
                        };
                    }
                }

                // Limit progress in the response; write full progress to file if large
                let max_progress_lines = 50usize;
                let total_progress = agent.progress.len();
                let progress_preview: Vec<String> = if total_progress > max_progress_lines {
                    agent
                        .progress
                        .iter()
                        .skip(total_progress - max_progress_lines)
                        .cloned()
                        .collect()
                } else {
                    agent.progress.clone()
                };

                let mut progress_file: Option<String> = None;
                if total_progress > max_progress_lines {
                    let cwd = sess.get_cwd().to_path_buf();
                    drop(manager);
                    let dir = match ensure_agent_dir(&cwd, &agent.id) {
                        Ok(d) => d,
                        Err(e) => {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(format!("Failed to prepare agent progress file: {e}")),
                                    success: Some(false),
                                },
                            };
                        }
                    };
                    // Re-acquire manager to get fresh progress after potential delay
                    let manager = AGENT_MANAGER.read().await;
                    if let Some(agent) = manager.get_agent(&params.agent_id) {
                        let joined = agent.progress.join("\n");
                        match write_agent_file(&dir, "progress.log", &joined) {
                            Ok(p) => progress_file = Some(p.display().to_string()),
                            Err(e) => {
                                return ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone,
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(format!("Failed to write progress file: {e}")),
                                        success: Some(false),
                                    },
                                };
                            }
                        }
                    }
                } else {
                    drop(manager);
                }

                let response = serde_json::json!({
                    "agent_id": params.agent_id,
                    "name": agent.name,
                    "status": agent.status,
                    "model": agent.model,
                    "batch_id": agent.batch_id,
                    "created_at": agent.created_at,
                    "started_at": agent.started_at,
                    "completed_at": agent.completed_at,
                    "progress_preview": progress_preview,
                    "progress_total": total_progress,
                    "progress_file": progress_file,
                    "error": agent.error,
                    "worktree_path": agent.worktree_path,
                    "branch_name": agent.branch_name,
                });

                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(response.to_string()),
                        success: Some(true),
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!("Agent not found: {}", params.agent_id)),
                        success: Some(false),
                    },
                }
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(format!("Invalid agent arguments for action=status: {e}")),
                success: Some(false),
            },
        },
    }
        },
    ).await
}

async fn handle_get_agent_result(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
    event_payload: serde_json::Value,
) -> ResponseInputItem {
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent".to_string(),
        Some(event_payload),
        || async move {
    match serde_json::from_str::<GetAgentResultParams>(&arguments_clone) {
        Ok(params) => {
            let manager = AGENT_MANAGER.read().await;

            if let Some(agent) = manager.get_agent(&params.agent_id) {
                match agent.batch_id.as_deref() {
                    Some(batch) if batch == params.batch_id => {}
                    _ => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Agent {} does not belong to batch {}",
                                    params.agent_id, params.batch_id
                                )),
                                success: Some(false),
                            },
                        };
                    }
                }
                let cwd = sess.get_cwd().to_path_buf();
                let dir = match ensure_agent_dir(&cwd, &params.agent_id) {
                    Ok(d) => d,
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!("Failed to prepare agent output dir: {e}")),
                                success: Some(false),
                            },
                        };
                    }
                };

                match agent.status {
                    AgentStatus::Completed => {
                        let output_text = agent.result.unwrap_or_default();
                        let (preview, total_lines) = preview_first_n_lines(&output_text, 500);
                        let file_path = match write_agent_file(&dir, "result.txt", &output_text) {
                            Ok(p) => p.display().to_string(),
                            Err(e) => format!("Failed to write result file: {e}"),
                        };
                        let response = serde_json::json!({
                            "agent_id": params.agent_id,
                            "batch_id": params.batch_id.clone(),
                            "status": agent.status,
                            "output_preview": preview,
                            "output_total_lines": total_lines,
                            "output_file": file_path,
                        });
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(response.to_string()),
                                success: Some(true),
                            },
                        }
                    }
                    AgentStatus::Failed => {
                        let error_text = agent.error.unwrap_or_else(|| "Unknown error".to_string());
                        let (preview, total_lines) = preview_first_n_lines(&error_text, 500);
                        let file_path = match write_agent_file(&dir, "error.txt", &error_text) {
                            Ok(p) => p.display().to_string(),
                            Err(e) => format!("Failed to write error file: {e}"),
                        };
                        let response = serde_json::json!({
                            "agent_id": params.agent_id,
                            "batch_id": params.batch_id.clone(),
                            "status": agent.status,
                            "error_preview": preview,
                            "error_total_lines": total_lines,
                            "error_file": file_path,
                        });
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(response.to_string()),
                                success: Some(false),
                            },
                        }
                    }
                    _ => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!(
                                "Agent is still {}: cannot get result yet",
                                serde_json::to_string(&agent.status)
                                    .unwrap_or_else(|_| "running".to_string())
                            )),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!("Agent not found: {}", params.agent_id)),
                        success: Some(false),
                    },
                }
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(format!("Invalid agent arguments for action=result: {e}")),
                success: Some(false),
            },
        },
    }
        },
    ).await
}

async fn handle_cancel_agent(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
    event_payload: serde_json::Value,
) -> ResponseInputItem {
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent".to_string(),
        Some(event_payload),
        || async move {
    match serde_json::from_str::<CancelAgentParams>(&arguments_clone) {
        Ok(params) => {
            let mut manager = AGENT_MANAGER.write().await;

            if let Some(agent_id) = params.agent_id {
                let batch_id = match params.batch_id.as_ref() {
                    Some(batch) => batch,
                    None => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text("action=cancel requires 'cancel.batch_id'".to_string()),
                                success: Some(false),
                            },
                        };
                    }
                };
                if let Some(agent) = manager.get_agent(&agent_id)
                    && agent.batch_id.as_deref() != Some(batch_id.as_str()) {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Agent {agent_id} does not belong to batch {batch_id}"
                                )),
                                success: Some(false),
                            },
                        };
                    }
                if manager.cancel_agent(&agent_id).await {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Agent {agent_id} cancelled")),
                            success: Some(true),
                        },
                    }
                } else {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to cancel agent {agent_id}")),
                            success: Some(false),
                        },
                    }
                }
            } else if let Some(batch_id) = params.batch_id {
                let count = manager.cancel_batch(&batch_id).await;
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!("Cancelled {count} agents in batch {batch_id}")),
                        success: Some(true),
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text("Either agent_id or batch_id must be provided".to_string()),
                        success: Some(false),
                    },
                }
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(format!("Invalid agent arguments for action=cancel: {e}")),
                success: Some(false),
            },
        },
    }
        },
    ).await
}

async fn handle_wait_for_agent(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
    event_payload: serde_json::Value,
) -> ResponseInputItem {
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent".to_string(),
        Some(event_payload),
        || async move {
            let (initial_wait_epoch, _) = sess.wait_interrupt_snapshot();
            match serde_json::from_str::<WaitForAgentParams>(&arguments_clone) {
                Ok(params) => {
                    let batch_id = match params.batch_id.as_ref() {
                        Some(batch) if !batch.trim().is_empty() => batch.clone(),
                        _ => {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text("action=wait requires 'wait.batch_id'".to_string()),
                                    success: Some(false),
                                },
                            };
                        }
                    };
                    let timeout = std::time::Duration::from_secs(
                        params.timeout_seconds.unwrap_or(300).min(600),
                    );
                    let start = std::time::Instant::now();

                    loop {
                        if start.elapsed() > timeout {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text("Timeout waiting for agent completion".to_string()),
                                    success: Some(false),
                                },
                            };
                        }

                        let manager = AGENT_MANAGER.read().await;

                        if let Some(agent_id) = &params.agent_id {
                            if let Some(agent) = manager.get_agent(agent_id) {
                                match agent.batch_id.as_deref() {
                                    Some(batch) if batch == batch_id => {}
                                    _ => {
                                        return ResponseInputItem::FunctionCallOutput {
                                            call_id: call_id_clone,
                                            output: FunctionCallOutputPayload {
                                                body: FunctionCallOutputBody::Text(format!(
                                                    "Agent {agent_id} does not belong to batch {batch_id}"
                                                )),
                                                success: Some(false),
                                            },
                                        };
                                    }
                                }
                                if matches!(
                            agent.status,
                            AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled
                        ) {
                            // Include output/error preview and file path
                            // Avoid holding manager lock during filesystem I/O
                            drop(manager);
                            let cwd = sess.get_cwd().to_path_buf();
                            let dir = match ensure_agent_dir(&cwd, &agent.id) {
                                Ok(d) => d,
                                Err(e) => {
                                    return ResponseInputItem::FunctionCallOutput {
                                        call_id: call_id_clone,
                                        output: FunctionCallOutputPayload {
                                            body: FunctionCallOutputBody::Text(format!("Failed to prepare agent output dir: {e}")),
                                            success: Some(false),
                                        },
                                    };
                                }
                            };
                            let (preview_key, file_key, preview, file_path, total_lines) = match agent.status {
                                AgentStatus::Completed => {
                                    let text = agent.result.clone().unwrap_or_default();
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "result.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write result file: {e}"));
                                    ("output_preview", "output_file", p, fp, total)
                                }
                                AgentStatus::Failed => {
                                    let text = agent.error.clone().unwrap_or_else(|| "Unknown error".to_string());
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "error.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write error file: {e}"));
                                    ("error_preview", "error_file", p, fp, total)
                                }
                                AgentStatus::Cancelled => {
                                    let text = "Agent cancelled".to_string();
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "status.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write status file: {e}"));
                                    ("status_preview", "status_file", p, fp, total)
                                }
                                _ => unreachable!(),
                            };

                            let hint = format!(
                                "agent {{\"action\":\"result\",\"result\":{{\"agent_id\":\"{}\",\"batch_id\":\"{}\"}}}}",
                                agent.id,
                                batch_id
                            );
                            let mut response = serde_json::json!({
                                "agent_id": agent.id,
                                "batch_id": batch_id,
                                "status": agent.status,
                                "wait_time_seconds": start.elapsed().as_secs(),
                                "total_lines": total_lines,
                                "agent_result_hint": hint,
                                "agent_result_params": { "action": "result", "result": { "agent_id": agent.id, "batch_id": batch_id } },
                            });
                            if let Some(obj) = response.as_object_mut() {
                                obj.insert(preview_key.to_string(), serde_json::Value::String(preview));
                                obj.insert(file_key.to_string(), serde_json::Value::String(file_path));
                            }
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(response.to_string()),
                                    success: Some(true),
                                },
                            };
                        }
                    }
                } else {
                    let agents = manager.list_agents(None, Some(batch_id.clone()), false);

                    // Separate terminal vs non-terminal agents
                    let completed_agents: Vec<_> = agents
                        .iter()
                        .filter(|t| {
                            matches!(
                                t.status,
                                AgentStatus::Completed
                                    | AgentStatus::Failed
                                    | AgentStatus::Cancelled
                            )
                        })
                        .cloned()
                        .collect();
                    let any_in_progress = agents.iter().any(|a| {
                        matches!(a.status, AgentStatus::Pending | AgentStatus::Running)
                    });

                    if params.return_all.unwrap_or(false) {
                        // Wait for ALL agents in the batch to reach a terminal state
                        if !any_in_progress {
                            // Enriched response: include per-agent previews and file paths
                            // Avoid holding manager lock during filesystem I/O
                            drop(manager);
                            let cwd = sess.get_cwd().to_path_buf();
                            let mut summaries: Vec<serde_json::Value> = Vec::new();
                            for a in &completed_agents {
                                let dir = match ensure_agent_dir(&cwd, &a.id) {
                                    Ok(d) => d,
                                    Err(e) => {
                                        return ResponseInputItem::FunctionCallOutput {
                                            call_id: call_id_clone,
                                            output: FunctionCallOutputPayload {
                                                body: FunctionCallOutputBody::Text(format!("Failed to prepare agent output dir: {e}")),
                                                success: Some(false),
                                            },
                                        };
                                    }
                                };
                                let (preview_key, file_key, preview, file_path, total_lines) = match a.status {
                                    AgentStatus::Completed => {
                                        let text = a.result.clone().unwrap_or_default();
                                        let (p, total) = preview_first_n_lines(&text, 500);
                                        let fp = write_agent_file(&dir, "result.txt", &text)
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(|e| format!("Failed to write result file: {e}"));
                                        ("output_preview", "output_file", p, fp, total)
                                    }
                                    AgentStatus::Failed => {
                                        let text = a.error.clone().unwrap_or_else(|| "Unknown error".to_string());
                                        let (p, total) = preview_first_n_lines(&text, 500);
                                        let fp = write_agent_file(&dir, "error.txt", &text)
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(|e| format!("Failed to write error file: {e}"));
                                        ("error_preview", "error_file", p, fp, total)
                                    }
                                    AgentStatus::Cancelled => {
                                        let text = "Agent cancelled".to_string();
                                        let (p, total) = preview_first_n_lines(&text, 500);
                                        let fp = write_agent_file(&dir, "status.txt", &text)
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(|e| format!("Failed to write status file: {e}"));
                                        ("status_preview", "status_file", p, fp, total)
                                    }
                                    _ => unreachable!(),
                                };

                                let hint = format!(
                                    "agent {{\"action\":\"result\",\"result\":{{\"agent_id\":\"{}\",\"batch_id\":\"{}\"}}}}",
                                    a.id,
                                    batch_id
                                );
                                let mut obj = serde_json::json!({
                                    "agent_id": a.id,
                                    "status": a.status,
                                    "total_lines": total_lines,
                                    "agent_result_hint": hint,
                                "agent_result_params": { "action": "result", "result": { "agent_id": a.id, "batch_id": batch_id } },
                                });
                                if let Some(map) = obj.as_object_mut() {
                                    map.insert(preview_key.to_string(), serde_json::Value::String(preview));
                                    map.insert(file_key.to_string(), serde_json::Value::String(file_path));
                                }
                                summaries.push(obj);
                            }

                            let response = serde_json::json!({
                                "batch_id": batch_id,
                                "completed_agents": completed_agents.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
                                "completed_summaries": summaries,
                                "wait_time_seconds": start.elapsed().as_secs(),
                            });
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(response.to_string()),
                                    success: Some(true),
                                },
                            };
                        }
                    } else {
                        // Sequential behavior: return the next unseen completed agent if available
                        let mut state = sess.state.lock().unwrap();
                        if !state.seen_completed_agents_by_batch.contains_key(&batch_id) {
                            state.seen_completed_batch_order.push_back(batch_id.clone());
                            while state.seen_completed_batch_order.len()
                                > super::session::MAX_WAIT_TRACKED_BATCHES
                            {
                                let Some(evict_batch_id) =
                                    state.seen_completed_batch_order.pop_front()
                                else {
                                    break;
                                };
                                state.seen_completed_agents_by_batch.remove(&evict_batch_id);
                            }
                        }
                        let seen = state
                            .seen_completed_agents_by_batch
                            .entry(batch_id.clone())
                            .or_default();

                        // Find the first completed agent that we haven't returned yet
                        if let Some(unseen) = completed_agents
                            .iter()
                            .find(|a| !seen.contains(&a.id))
                            .cloned()
                        {
                            // Record as seen and return immediately
                            seen.insert(unseen.id.clone());
                            if seen.len() > super::session::MAX_WAIT_TRACKED_AGENT_IDS_PER_BATCH {
                                warn!(
                                    batch_id,
                                    limit = super::session::MAX_WAIT_TRACKED_AGENT_IDS_PER_BATCH,
                                    "seen-completed agent tracker exceeded limit; clearing batch cache"
                                );
                                seen.clear();
                                seen.insert(unseen.id.clone());
                            }
                            drop(state);

                            // Include output/error preview for the unseen completed agent
                            // Avoid holding manager lock during filesystem I/O
                            drop(manager);
                            let cwd = sess.get_cwd().to_path_buf();
                            let dir = match ensure_agent_dir(&cwd, &unseen.id) {
                                Ok(d) => d,
                                Err(e) => {
                                    return ResponseInputItem::FunctionCallOutput {
                                        call_id: call_id_clone,
                                        output: FunctionCallOutputPayload {
                                            body: FunctionCallOutputBody::Text(format!("Failed to prepare agent output dir: {e}")),
                                            success: Some(false),
                                        },
                                    };
                                }
                            };
                            let (preview_key, file_key, preview, file_path, total_lines) = match unseen.status {
                                AgentStatus::Completed => {
                                    let text = unseen.result.clone().unwrap_or_default();
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "result.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write result file: {e}"));
                                    ("output_preview", "output_file", p, fp, total)
                                }
                                AgentStatus::Failed => {
                                    let text = unseen.error.clone().unwrap_or_else(|| "Unknown error".to_string());
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "error.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write error file: {e}"));
                                    ("error_preview", "error_file", p, fp, total)
                                }
                                AgentStatus::Cancelled => {
                                    let text = "Agent cancelled".to_string();
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "status.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write status file: {e}"));
                                    ("status_preview", "status_file", p, fp, total)
                                }
                                _ => unreachable!(),
                            };

                            let hint = format!(
                                "agent {{\"action\":\"result\",\"result\":{{\"agent_id\":\"{}\",\"batch_id\":\"{}\"}}}}",
                                unseen.id,
                                batch_id
                            );
                            let mut response = serde_json::json!({
                                "agent_id": unseen.id,
                                "status": unseen.status,
                                "wait_time_seconds": start.elapsed().as_secs(),
                                "total_lines": total_lines,
                                "agent_result_hint": hint,
                                "agent_result_params": { "action": "result", "result": { "agent_id": unseen.id, "batch_id": batch_id } },
                            });
                            if let Some(obj) = response.as_object_mut() {
                                obj.insert(preview_key.to_string(), serde_json::Value::String(preview));
                                obj.insert(file_key.to_string(), serde_json::Value::String(file_path));
                            }
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(response.to_string()),
                                    success: Some(true),
                                },
                            };
                        }

                        // If all agents in the batch are terminal and all have been seen, return immediately
                        if !any_in_progress && !completed_agents.is_empty() {
                            // Mark all as seen to keep state consistent
                            for a in &completed_agents {
                                seen.insert(a.id.clone());
                            }
                            drop(state);

                            let response = serde_json::json!({
                                "batch_id": batch_id,
                                "status": "no_agents_remaining",
                                "wait_time_seconds": start.elapsed().as_secs(),
                            });
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(response.to_string()),
                                    success: Some(true),
                                },
                            };
                        }
                    }
                }

                drop(manager);

                let time_budget_message = {
                    let mut guard = sess.time_budget.lock().unwrap();
                    guard
                        .as_mut()
                        .and_then(|budget| budget.maybe_nudge(Instant::now()))
                };

                if let Some(budget_text) = time_budget_message {
                    let response = serde_json::json!({
                        "batch_id": batch_id,
                        "status": "time_budget_update",
                        "wait_time_seconds": start.elapsed().as_secs(),
                        "time_budget_message": budget_text,
                        "message": "Wait interrupted so the assistant can adapt. Agents may still be running; call agent wait again to continue.",
                    });
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(response.to_string()),
                            success: Some(false),
                        },
                    };
                }

                let (current_epoch, reason) = sess.wait_interrupt_snapshot();
                if current_epoch != initial_wait_epoch {
                    let message = match reason {
                        Some(WaitInterruptReason::UserMessage) => {
                            "wait ended due to new user message".to_string()
                        }
                        _ => "wait ended because the session was interrupted".to_string(),
                    };
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(message),
                            success: Some(false),
                        },
                    };
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!("Invalid agent arguments for action=wait: {e}")),
                        success: Some(false),
                    },
                },
            }
        },
    ).await
}

async fn handle_list_agents(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
    event_payload: serde_json::Value,
) -> ResponseInputItem {
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent".to_string(),
        Some(event_payload),
        || async move {
    match serde_json::from_str::<ListAgentsParams>(&arguments_clone) {
        Ok(params) => {
            let manager = AGENT_MANAGER.read().await;

            let batch_id = match params.batch_id.clone() {
                Some(batch) if !batch.trim().is_empty() => batch,
                _ => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text("action=list requires 'list.batch_id'".to_string()),
                            success: Some(false),
                        },
                    };
                }
            };

            let status_filter =
                params
                    .status_filter
                    .and_then(|s| match s.to_lowercase().as_str() {
                        "pending" => Some(AgentStatus::Pending),
                        "running" => Some(AgentStatus::Running),
                        "completed" => Some(AgentStatus::Completed),
                        "failed" => Some(AgentStatus::Failed),
                        "cancelled" => Some(AgentStatus::Cancelled),
                        _ => None,
                    });

            let agents = manager.list_agents(
                status_filter,
                Some(batch_id.clone()),
                params.recent_only.unwrap_or(false),
            );

            // Count running agents for status update
            let running_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Running)
                .count();
            if running_count > 0 {
                let status_msg = format!(
                    "🤖 {} agent{} currently running",
                    running_count,
                    if running_count != 1 { "s" } else { "" }
                );
                let event = sess.make_event(
                    "agent-status",
                    EventMsg::BackgroundEvent(BackgroundEventEvent { message: status_msg }),
                );
                let _ = sess.tx_event.send(event).await;
            }

            // Add status counts to summary
            let pending_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Pending)
                .count();
            let running_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Running)
                .count();
            let completed_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Completed)
                .count();
            let failed_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Failed)
                .count();
            let cancelled_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Cancelled)
                .count();

            let summary = serde_json::json!({
                "total_agents": agents.len(),
                "status_counts": {
                    "pending": pending_count,
                    "running": running_count,
                    "completed": completed_count,
                    "failed": failed_count,
                    "cancelled": cancelled_count,
                },
                "batch_id": batch_id,
                "agents": agents.iter().map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "name": t.name.clone(),
                        "model": t.model,
                        "status": t.status,
                        "created_at": t.created_at,
                        "batch_id": t.batch_id,
                        "worktree_path": t.worktree_path,
                        "branch_name": t.branch_name,
                    })
                }).collect::<Vec<_>>(),
            });

            ResponseInputItem::FunctionCallOutput {
                call_id: call_id_clone,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(summary.to_string()),
                    success: Some(true),
                },
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(format!("Invalid agent arguments for action=list: {e}")),
                success: Some(false),
            },
        },
    }
        },
    ).await
}



pub(super) fn get_last_assistant_message_from_turn(responses: &[ResponseItem]) -> Option<String> {
    responses.iter().rev().find_map(|item| {
        if let ResponseItem::Message { role, content, .. } = item {
            if role == "assistant" {
                content.iter().rev().find_map(|ci| {
                    if let ContentItem::OutputText { text } = ci {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        } else {
            None
        }
    })
}

/// Capture a screenshot from the browser and store it for the next model request
pub(super) async fn capture_browser_screenshot(
    _sess: &Session,
) -> Result<(PathBuf, String), String> {
    let browser_manager = code_browser::global::get_browser_manager()
        .await
        .ok_or_else(|| "No browser manager available".to_string())?;

    if !browser_manager.is_enabled().await {
        return Err("Browser manager is not enabled".to_string());
    }

    // Get current URL first
    let url = browser_manager
        .get_current_url()
        .await
        .unwrap_or_else(|| "Browser".to_string());
    tracing::debug!("Attempting to capture screenshot at URL: {}", url);

    match browser_manager.capture_screenshot().await {
        Ok(screenshots) => {
            if let Some(first_screenshot) = screenshots.first() {
                tracing::info!(
                    "Captured browser screenshot: {} at URL: {}",
                    first_screenshot.display(),
                    url
                );
                Ok((first_screenshot.clone(), url))
            } else {
                let msg = format!("Screenshot capture returned empty results at URL: {url}");
                tracing::warn!("{}", msg);
                Err(msg)
            }
        }
        Err(e) => {
            let msg = format!("Failed to capture screenshot at {url}: {e}");
            tracing::warn!("{}", msg);
            Err(msg)
        }
    }
}

#[derive(Default)]
struct AgentBatchCompletionStatus {
    has_terminal: bool,
    has_non_terminal: bool,
}

fn is_terminal_agent_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "failed" | "cancelled" | "canceled"
    )
}

fn is_auto_review_agent_info(agent: &crate::protocol::AgentInfo) -> bool {
    matches!(agent.source_kind, Some(AgentSourceKind::AutoReview))
        || agent
            .batch_id
            .as_deref()
            .map(|batch| batch.eq_ignore_ascii_case("auto-review"))
            .unwrap_or(false)
}

fn build_agent_completion_wake_message(batch_id: &str) -> ResponseInputItem {
    let text = format!(
        "Agents in batch {batch_id} have completed. Call agent {{\"action\":\"wait\",\"wait\":{{\"batch_id\":\"{batch_id}\",\"return_all\":true}}}} to collect their results, then continue the task.",
    );
    ResponseInputItem::Message {
        role: "developer".to_string(),
        content: vec![ContentItem::InputText { text }],
    }
}

pub(super) fn agent_completion_wake_messages(
    payload: &AgentStatusUpdatePayload,
    seen_batches: &mut HashSet<String>,
) -> Vec<ResponseInputItem> {
    let mut batches: HashMap<String, AgentBatchCompletionStatus> = HashMap::new();

    for agent in &payload.agents {
        if is_auto_review_agent_info(agent) {
            continue;
        }
        let Some(batch_id) = agent.batch_id.as_ref() else {
            continue;
        };
        let trimmed = batch_id.trim();
        if trimmed.is_empty() {
            continue;
        }

        let status = batches.entry(trimmed.to_string()).or_default();
        if is_terminal_agent_status(agent.status.as_str()) {
            status.has_terminal = true;
        } else {
            status.has_non_terminal = true;
        }
    }

    let mut messages = Vec::new();
    for (batch_id, status) in batches {
        if !status.has_terminal || status.has_non_terminal {
            continue;
        }
        if !seen_batches.insert(batch_id.clone()) {
            continue;
        }
        messages.push(build_agent_completion_wake_message(batch_id.as_str()));
    }

    messages
}

pub(super) async fn enqueue_agent_completion_wake(
    sess: &Arc<Session>,
    messages: Vec<ResponseInputItem>,
) {
    if messages.is_empty() {
        return;
    }

    let mut should_start_turn = false;
    for message in messages {
        if sess.enqueue_out_of_turn_item(message) {
            should_start_turn = true;
        }
    }

    if should_start_turn {
        sess.cleanup_old_status_items().await;
        let turn_context = sess.make_turn_context();
        let sub_id = sess.next_internal_sub_id();
        let sentinel_input = vec![InputItem::Text {
            text: PENDING_ONLY_SENTINEL.to_string(),
        }];
        let agent = AgentTask::spawn(Arc::clone(sess), turn_context, sub_id, sentinel_input);
        sess.set_task(agent);
    }
}

#[cfg(test)]
mod agent_completion_wake_tests {
    use std::collections::HashSet;

    use super::agent_completion_wake_messages;
    use super::AgentSourceKind;
    use crate::agent_tool::AgentStatusUpdatePayload;
    use crate::protocol::AgentInfo;

    fn agent_info(
        id: &str,
        status: &str,
        batch_id: Option<&str>,
        source_kind: Option<AgentSourceKind>,
    ) -> AgentInfo {
        AgentInfo {
            id: id.to_string(),
            name: id.to_string(),
            status: status.to_string(),
            batch_id: batch_id.map(str::to_string),
            model: None,
            last_progress: None,
            result: None,
            error: None,
            elapsed_ms: None,
            token_count: None,
            last_activity_at: None,
            seconds_since_last_activity: None,
            source_kind,
        }
    }

    #[test]
    fn agent_completion_wake_messages_dedupes_and_skips_non_terminal() {
        let mut seen = HashSet::new();
        let running = AgentStatusUpdatePayload {
            agents: vec![agent_info("agent-1", "running", Some("batch-1"), None)],
            context: None,
            task: None,
        };
        assert!(agent_completion_wake_messages(&running, &mut seen).is_empty());

        let mixed = AgentStatusUpdatePayload {
            agents: vec![
                agent_info("agent-1", "completed", Some("batch-1"), None),
                agent_info("agent-2", "running", Some("batch-1"), None),
            ],
            context: None,
            task: None,
        };
        assert!(agent_completion_wake_messages(&mixed, &mut seen).is_empty());

        let completed = AgentStatusUpdatePayload {
            agents: vec![agent_info("agent-1", "completed", Some("batch-1"), None)],
            context: None,
            task: None,
        };
        let messages = agent_completion_wake_messages(&completed, &mut seen);
        assert_eq!(messages.len(), 1);

        let messages_again = agent_completion_wake_messages(&completed, &mut seen);
        assert!(messages_again.is_empty());

        let auto_review = AgentStatusUpdatePayload {
            agents: vec![agent_info(
                "agent-3",
                "completed",
                Some("auto-review"),
                Some(AgentSourceKind::AutoReview),
            )],
            context: None,
            task: None,
        };
        assert!(agent_completion_wake_messages(&auto_review, &mut seen).is_empty());
    }
}

/// Send agent status update event to the TUI
pub(super) async fn send_agent_status_update(sess: &Session) {
    let manager = AGENT_MANAGER.read().await;

    // Collect all agents; include completed/failed so HUD can show final messages
    let now = Utc::now();
    let agents: Vec<crate::protocol::AgentInfo> = manager
        .get_all_agents()
        .map(|agent| {
            let start = agent.started_at.unwrap_or(agent.created_at);
            let end = agent.completed_at.unwrap_or(now);
            let elapsed_ms = match end.signed_duration_since(start).num_milliseconds() {
                value if value >= 0 => Some(value as u64),
                _ => None,
            };

            crate::protocol::AgentInfo {
                id: agent.id.clone(),
                name: agent.model.clone(), // Use model name as the display name
                status: match agent.status {
                    AgentStatus::Pending => "pending".to_string(),
                    AgentStatus::Running => "running".to_string(),
                    AgentStatus::Completed => "completed".to_string(),
                    AgentStatus::Failed => "failed".to_string(),
                    AgentStatus::Cancelled => "cancelled".to_string(),
                },
                batch_id: agent.batch_id.clone(),
                model: Some(agent.model.clone()),
                last_progress: agent.progress.last().cloned(),
                result: agent.result.clone(),
                error: agent.error.clone(),
                elapsed_ms,
                token_count: None,
                last_activity_at: matches!(agent.status, AgentStatus::Pending | AgentStatus::Running)
                    .then(|| agent.last_activity.to_rfc3339()),
                seconds_since_last_activity: matches!(
                    agent.status,
                    AgentStatus::Pending | AgentStatus::Running
                )
                .then(|| {
                    Utc::now()
                        .signed_duration_since(agent.last_activity)
                        .num_seconds()
                        .max(0) as u64
                }),
                source_kind: agent.source_kind.clone(),
            }
        })
        .collect();

    let event = sess.make_event(
        "agent_status",
        EventMsg::AgentStatusUpdate(AgentStatusUpdateEvent {
            agents,
            context: None,
            task: None,
        }),
    );

    // Send event asynchronously
    let tx_event = sess.tx_event.clone();
    tokio::spawn(async move {
        if let Err(e) = tx_event.send(event).await {
            tracing::error!("Failed to send agent status update event: {}", e);
        }
    });
}
