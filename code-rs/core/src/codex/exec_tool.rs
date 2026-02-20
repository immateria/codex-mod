use super::*;
use super::exec::{
    ApplyPatchCommandContext,
    ExecCommandContext,
    maybe_run_with_user_profile,
};
use super::fs_utils::{ensure_agent_dir, write_agent_file};
use super::session::BackgroundExecState;
use super::truncation::truncate_middle_bytes;
use crate::tools::events::execute_custom_tool;
use crate::tools::output_format::{format_exec_output_payload, format_exec_output_str};
use code_protocol::models::FunctionCallOutputBody;

pub(crate) async fn handle_wait(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    use serde::Deserialize;
    #[derive(Deserialize, Clone)]
    struct Params { #[serde(default)] call_id: Option<String>, #[serde(default)] timeout_ms: Option<u64> }
    let mut params_for_event = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    if let Some(serde_json::Value::Object(map)) = params_for_event.as_mut()
        && let Some(serde_json::Value::String(cid)) = map.get("call_id")
        && let Some(display) = sess.background_exec_cmd_display(cid)
    {
        map.insert("for".to_string(), serde_json::Value::String(display));
    }
    let arguments_clone = arguments.clone();
    let ctx_clone = ToolCallCtx::new(ctx.sub_id.clone(), ctx.call_id.clone(), ctx.seq_hint, ctx.output_index);
    let ctx_for_closure = ctx_clone.clone();
    execute_custom_tool(
        sess,
        &ctx_clone,
        "wait".to_string(),
        params_for_event,
        move || async move {
            let ctx_inner = ctx_for_closure.clone();
                let parsed: Params = match serde_json::from_str(&arguments_clone) {
                    Ok(p) => p,
                    Err(e) => {
                    return ResponseInputItem::FunctionCallOutput { call_id: ctx_inner.call_id.clone(), output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(format!("Invalid wait arguments: {e}")), success: Some(false) } };
                    }
                };
                let call_id = match parsed.call_id {
                    Some(cid) if !cid.is_empty() => cid,
                    _ => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: ctx_inner.call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text("wait requires a call_id".to_string()),
                                success: Some(false),
                            },
                        };
                    }
                };
                let max_ms: u64 = 3_600_000; // 60 minutes cap
                let default_ms: u64 = 600_000; // 10 minutes default
                let timeout_ms = parsed.timeout_ms.unwrap_or(default_ms).min(max_ms);
                use std::sync::atomic::Ordering;
                let (initial_wait_epoch, _) = sess.wait_interrupt_snapshot();
                let (notify_opt, done_opt, tail, suppress_flag) = {
                    let st = sess.state.lock().unwrap();
                    match st.background_execs.get(&call_id) {
                        Some(bg) => (
                            Some(bg.notify.clone()),
                            bg.result_cell.lock().unwrap().clone(),
                            bg.tail_buf.clone(),
                            Some(bg.suppress_event.clone()),
                        ),
                        None => (None, None, None, None),
                    }
                };

                struct WaitSuppressGuard {
                    flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
                }

                impl WaitSuppressGuard {
                    fn new(flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>) -> Self {
                        if let Some(flag) = flag.as_ref() {
                            flag.store(true, Ordering::Relaxed);
                        }
                        Self { flag }
                    }

                    fn disarm(mut self) {
                        self.flag = None;
                    }
                }

                impl Drop for WaitSuppressGuard {
                    fn drop(&mut self) {
                        if let Some(flag) = self.flag.as_ref() {
                            flag.store(false, Ordering::Relaxed);
                        }
                    }
                }

                let suppress_guard = WaitSuppressGuard::new(suppress_flag.clone());

                if let Some(done) = done_opt {
                    {
                        let mut st = sess.state.lock().unwrap();
                        st.background_execs.remove(&call_id);
                    }
                    let content = format_exec_output_with_limit(
                        sess.get_cwd(),
                        &ctx_inner.sub_id,
                        &ctx_inner.call_id,
                        &done,
                        sess.tool_output_max_bytes,
                    );
                    suppress_guard.disarm();
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: ctx_inner.call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(content),
                            success: Some(done.exit_code == 0),
                        },
                    };
                }
                let Some(spec_notify) = notify_opt else {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: ctx_inner.call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("No background job found for call_id={call_id}")),
                            success: Some(false),
                        },
                    };
                };
                let any_notify = ANY_BG_NOTIFY.get().cloned().unwrap();

                let deadline = tokio::time::Instant::now()
                    + std::time::Duration::from_millis(timeout_ms);

                loop {
                    let (known_done, known_missing, task_finished) = {
                        let st = sess.state.lock().unwrap();
                        match st.background_execs.get(&call_id) {
                            Some(bg) => (
                                bg.result_cell.lock().unwrap().is_some(),
                                false,
                                bg.task_handle
                                    .as_ref()
                                    .is_some_and(tokio::task::JoinHandle::is_finished),
                            ),
                            None => (false, true, false),
                        }
                    };

                    if known_missing {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: ctx_inner.call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!("No background job found for call_id={call_id}")),
                                success: Some(false),
                            },
                        };
                    }

                    if task_finished && !known_done {
                        let mut st = sess.state.lock().unwrap();
                        st.background_execs.remove(&call_id);
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: ctx_inner.call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Background job {call_id} ended without a result; it may have been cancelled or crashed."
                                )),
                                success: Some(false),
                            },
                        };
                    }

                    if known_done {
                        break;
                    }

                    let time_budget_message = {
                        let mut guard = sess.time_budget.lock().unwrap();
                        guard
                            .as_mut()
                            .and_then(|budget| budget.maybe_nudge(Instant::now()))
                    };

                    if let Some(budget_text) = time_budget_message {
                        let msg = format!(
                            "{budget_text}\n\nWait interrupted so the assistant can adapt. Background job {call_id} still running.\n\nContinue by calling wait(call_id=\"{call_id}\")."
                        );
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: ctx_inner.call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(msg),
                                success: Some(false),
                            },
                        };
                    }

                    let (current_epoch, reason) = sess.wait_interrupt_snapshot();
                    if current_epoch != initial_wait_epoch {
                        let message = match reason {
                            Some(WaitInterruptReason::UserMessage) => {
                                format!(
                                    "wait ended due to new user message (background job {call_id} still running)"
                                )
                            }
                            _ => format!(
                                "wait ended because the session was interrupted (background job {call_id} still running)"
                            ),
                        };
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: ctx_inner.call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(message),
                                success: Some(false),
                            },
                        };
                    }

                    let now = tokio::time::Instant::now();
                    if now >= deadline {
                        let tail_text = tail
                            .as_ref()
                            .map(|arc| String::from_utf8_lossy(&arc.lock().unwrap()).to_string())
                            .unwrap_or_default();
                        let msg = if tail_text.is_empty() {
                            format!("Background job {call_id} still running...")
                        } else {
                            format!(
                                "Background job {call_id} still running...\n\nOutput so far (tail):\n{tail_text}"
                            )
                        };
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: ctx_inner.call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(msg),
                                success: Some(false),
                            },
                        };
                    }

                    let remaining = deadline - now;
                    let poll = std::time::Duration::from_millis(200);
                    let sleep_for = std::cmp::min(poll, remaining);

                    tokio::select! {
                        _ = spec_notify.notified() => {},
                        _ = any_notify.notified() => {},
                        _ = tokio::time::sleep(sleep_for) => {},
                    }
                }

                let done = {
                    let mut st = sess.state.lock().unwrap();
                    if let Some(bg) = st.background_execs.remove(&call_id) {
                        bg.result_cell.lock().unwrap().clone()
                    } else {
                        let found = st
                            .background_execs
                            .iter()
                            .find_map(|(k, v)| if v.result_cell.lock().unwrap().is_some() { Some(k.clone()) } else { None });
                        found
                            .and_then(|k| st.background_execs.remove(&k))
                            .and_then(|bg| bg.result_cell.lock().unwrap().clone())
                    }
                };
                if let Some(done) = done {
                    let content = format_exec_output_with_limit(
                        sess.get_cwd(),
                        &ctx_inner.sub_id,
                        &ctx_inner.call_id,
                        &done,
                        sess.tool_output_max_bytes,
                    );
                    suppress_guard.disarm();
                    ResponseInputItem::FunctionCallOutput {
                        call_id: ctx_inner.call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(content),
                            success: Some(done.exit_code == 0),
                        },
                    }
                } else {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: ctx_inner.call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text("No completed background job found".to_string()),
                            success: Some(false),
                        },
                    }
                }
        }
    ).await
}

pub(crate) async fn handle_kill(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    use serde::Deserialize;
    #[derive(Deserialize, Clone)]
    struct Params {
        call_id: String,
    }

    let mut params_for_event = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    let arguments_clone = arguments.clone();
    let ctx_clone = ToolCallCtx::new(ctx.sub_id.clone(), ctx.call_id.clone(), ctx.seq_hint, ctx.output_index);
    let ctx_for_closure = ctx_clone.clone();
    let tx_event = sess.tx_event.clone();

    execute_custom_tool(
        sess,
        &ctx_clone,
        "kill".to_string(),
        params_for_event.take(),
        move || async move {
            let ctx_inner = ctx_for_closure.clone();
            let parsed: Params = match serde_json::from_str(&arguments_clone) {
                Ok(p) => p,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: ctx_inner.call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Invalid kill arguments: {e}")),
                            success: Some(false),
                        },
                    };
                }
            };

            use std::sync::atomic::Ordering;

            let (
                notify,
                result_cell,
                suppress_flag,
                cmd_display,
                order_meta_for_end,
                sub_id_for_end,
                handle_opt,
                already_done,
            ) = {
                let mut st = sess.state.lock().unwrap();
                match st.background_execs.get_mut(&parsed.call_id) {
                    Some(bg) => {
                        let done = bg.result_cell.lock().unwrap().is_some();
                        let handle = bg.task_handle.take();
                        (
                            bg.notify.clone(),
                            bg.result_cell.clone(),
                            bg.suppress_event.clone(),
                            bg.cmd_display.clone(),
                            bg.order_meta_for_end.clone(),
                            bg.sub_id.clone(),
                            handle,
                            done,
                        )
                    }
                    None => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: ctx_inner.call_id.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!("No background job found for call_id={}", parsed.call_id)),
                                success: Some(false),
                            },
                        };
                    }
                }
            };

            if already_done {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: ctx_inner.call_id.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!("Background job {} has already completed.", parsed.call_id)),
                        success: Some(false),
                    },
                };
            }

            suppress_flag.store(true, Ordering::Relaxed);
            if let Some(handle) = handle_opt {
                handle.abort();
                let _ = handle.await;
            }

            let cancel_message = "Cancelled by user.".to_string();
            let output = ExecToolCallOutput {
                exit_code: 130,
                stdout: StreamOutput::new(String::new()),
                stderr: StreamOutput::new(cancel_message.clone()),
                aggregated_output: StreamOutput::new(cancel_message.clone()),
                duration: std::time::Duration::ZERO,
                timed_out: false,
            };

            {
                let mut slot = result_cell.lock().unwrap();
                *slot = Some(output.clone());
            }

            notify.notify_waiters();
            if let Some(global) = ANY_BG_NOTIFY.get() {
                global.notify_waiters();
            }

            let end_msg = EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id: parsed.call_id.clone(),
                stdout: output.stdout.text.clone(),
                stderr: output.stderr.text.clone(),
                exit_code: output.exit_code,
                duration: output.duration,
            });
            let event = Event {
                id: sub_id_for_end.clone(),
                event_seq: 0,
                msg: end_msg,
                order: Some(order_meta_for_end),
            };
            let _ = tx_event.send(event).await;

            let status = if cmd_display.trim().is_empty() {
                format!("Killed background job {}", parsed.call_id)
            } else {
                format!("Killed background command: {cmd_display}")
            };

            ResponseInputItem::FunctionCallOutput {
                call_id: ctx_inner.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(status),
                    success: Some(true),
                },
            }
        },
    ).await
}

pub(crate) fn to_exec_params(params: ShellToolCallParams, sess: &Session) -> ExecParams {
    let timeout_ms = params
        .timeout_ms
        .map(|ms| ms.max(MIN_SHELL_TIMEOUT_MS));
    let with_escalated_permissions = params
        .sandbox_permissions
        .and_then(|p| p.requires_escalated_permissions().then_some(true));
    ExecParams {
        command: params.command,
        cwd: sess.resolve_path(params.workdir.clone()),
        timeout_ms,
        env: create_env(&sess.shell_environment_policy),
        with_escalated_permissions,
        justification: params.justification,
    }
}

pub(crate) fn parse_container_exec_arguments(
    arguments: String,
    sess: &Session,
    call_id: &str,
) -> Result<ExecParams, Box<ResponseInputItem>> {
    // Parse command.
    //
    // Newer prompts use `sandbox_permissions` ("use_default" | "require_escalated");
    // older ones used `with_escalated_permissions: bool`. Accept both.
    let parsed: std::result::Result<serde_json::Value, serde_json::Error> =
        serde_json::from_str(&arguments);

    match parsed
        .and_then(|mut value| {
            if value.get("sandbox_permissions").is_none() {
                let needs_escalated = value
                    .get("with_escalated_permissions")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if needs_escalated {
                    value["sandbox_permissions"] = serde_json::json!(SandboxPermissions::RequireEscalated);
                }
            }
            serde_json::from_value::<ShellToolCallParams>(value)
        }) {
        Ok(shell_tool_call_params) => Ok(to_exec_params(shell_tool_call_params, sess)),
        Err(e) => {
            // allow model to re-sample
            let output = ResponseInputItem::FunctionCallOutput {
                call_id: call_id.to_string(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!("failed to parse function arguments: {e}")),
                    success: None,
                },
            };
            Err(Box::new(output))
        }
    }
}

pub(crate) async fn handle_container_exec_with_params(
    params: ExecParams,
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    ctx: &ToolCallCtx,
    attempt_req: u64,
) -> ResponseInputItem {
    let sub_id = ctx.sub_id.clone();
    let call_id = ctx.call_id.clone();
    let seq_hint = ctx.seq_hint;
    let output_index = ctx.output_index;
    // Intercept risky git commands and require an explicit confirm prefix.
    // We support a simple convention: prefix the script with `confirm:` to proceed.
    // The prefix is stripped before execution.
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    enum SensitiveGitKind {
        BranchChange,
        PathCheckout,
        Reset,
        Revert,
    }

    fn detect_sensitive_git(script: &str) -> Option<SensitiveGitKind> {
        // Goal: detect sensitive git invocations (branch changes, resets) while
        // avoiding false positives from commit messages or other quoted strings.
        // We do a lightweight scan that strips quoted regions before token analysis.

        // 1) Strip quote characters but preserve content inside quotes, while
        // neutralizing control separators to avoid over-splitting tokens.
        let mut cleaned = String::with_capacity(script.len());
        let mut in_squote = false;
        let mut in_dquote = false;
        let mut prev_was_backslash = false;
        for ch in script.chars() {
            let mut emit_space = false;
            match ch {
                '\\' => {
                    // Track escapes inside double quotes; in single quotes, backslash has no special meaning in POSIX sh.
                    prev_was_backslash = !prev_was_backslash;
                }
                '\'' if !in_dquote => {
                    in_squote = !in_squote;
                    emit_space = true; // token boundary at quote edges
                    prev_was_backslash = false;
                }
                '"' if !in_squote && !prev_was_backslash => {
                    in_dquote = !in_dquote;
                    emit_space = true; // token boundary at quote edges
                    prev_was_backslash = false;
                }
                _ => {
                    prev_was_backslash = false;
                }
            }
            if emit_space {
                cleaned.push(' ');
                continue;
            }
            if in_squote || in_dquote {
                if matches!(ch, '|' | '&' | ';' | '\n' | '\r') {
                    cleaned.push(' ');
                } else {
                    cleaned.push(ch);
                }
            } else {
                cleaned.push(ch);
            }
        }

        // 2) Split into simple commands at common separators.
        for chunk in cleaned.split([';', '\n', '\r']) {
            // Further split on conditional operators while keeping order.
            for part in chunk.split(['|', '&']) {
                let s = part.trim();
                if s.is_empty() { continue; }
                // Tokenize on whitespace, skip wrappers and git globals to find the real subcommand.
                let raw_tokens: Vec<&str> = s.split_whitespace().collect();
                if raw_tokens.is_empty() { continue; }
                fn strip_tok(t: &str) -> &str { t.trim_matches(|c| matches!(c, '(' | ')' | '{' | '}' | '\'' | '"')) }
                let mut i = 0usize;
                // Skip env assignments and lightweight wrappers/keywords.
                loop {
                    if i >= raw_tokens.len() { break; }
                    let tok = strip_tok(raw_tokens[i]);
                    if tok.is_empty() { i += 1; continue; }
                    // Skip KEY=val assignments.
                    if tok.contains('=') && !tok.starts_with('=') && !tok.starts_with('-') {
                        i += 1; continue;
                    }
                    // Skip simple wrappers and control keywords.
                    if matches!(tok, "env" | "sudo" | "command" | "time" | "nohup" | "nice" | "then" | "do" | "{" | "(") {
                        // Best-effort: skip immediate option-like flags after some wrappers.
                        i += 1;
                        while i < raw_tokens.len() {
                            let peek = strip_tok(raw_tokens[i]);
                            if peek.starts_with('-') { i += 1; } else { break; }
                        }
                        continue;
                    }
                    break;
                }
                if i >= raw_tokens.len() { continue; }
                let cmd = strip_tok(raw_tokens[i]);
                let is_git = cmd.ends_with("/git") || cmd == "git";
                if !is_git { continue; }
                i += 1; // advance past git
                // Skip git global options to find the real subcommand.
                while i < raw_tokens.len() {
                    let t = strip_tok(raw_tokens[i]);
                    if t.is_empty() { i += 1; continue; }
                    if matches!(t, "-C" | "--git-dir" | "--work-tree" | "-c") {
                        i += 1; // skip option key
                        if i < raw_tokens.len() { i += 1; } // skip its value
                        continue;
                    }
                    if t.starts_with("--git-dir=") || t.starts_with("--work-tree=") || t.starts_with("-c") {
                        i += 1; continue;
                    }
                    if t.starts_with('-') { i += 1; continue; }
                    break;
                }
                if i >= raw_tokens.len() { continue; }
                let sub = strip_tok(raw_tokens[i]);
                i += 1;
                match sub {
                    "checkout" => {
                        let args: Vec<&str> = raw_tokens[i..].iter().map(|t| strip_tok(t)).collect();
                        let has_path_delimiter = args.contains(&"--");
                        if has_path_delimiter {
                            return Some(SensitiveGitKind::PathCheckout);
                        }

                        // If any of the strong branch-changing flags are present, flag it.
                        let mut saw_branch_change_flag = false;
                        for a in &args {
                            if matches!(*a, "-b" | "-B" | "--orphan" | "--detach") {
                                saw_branch_change_flag = true;
                                break;
                            }
                        }
                        if saw_branch_change_flag { return Some(SensitiveGitKind::BranchChange); }

                        // `git checkout -` switches to previous branch.
                        if args.first().copied() == Some("-") {
                            return Some(SensitiveGitKind::BranchChange);
                        }

                        // Heuristic: a single non-flag argument likely denotes a branch.
                        if let Some(first_arg) = args.first() {
                            let a = *first_arg;
                            if !a.starts_with('-') && a != "." && a != ".." {
                                return Some(SensitiveGitKind::BranchChange);
                            }
                        }
                    }
                    "switch" => {
                        // `git switch -c <name>` creates; `git switch <name>` changes.
                        let mut saw_c = false;
                        let mut saw_detach = false;
                        let mut first_non_flag: Option<&str> = None;
                        for a in &raw_tokens[i..] {
                            let a = strip_tok(a);
                            if a == "-c" { saw_c = true; break; }
                            if a == "--detach" { saw_detach = true; break; }
                            if a.starts_with('-') { continue; }
                            first_non_flag = Some(a);
                            break;
                        }
                        if saw_c || saw_detach || first_non_flag.is_some() { return Some(SensitiveGitKind::BranchChange); }
                    }
                    "reset" => {
                        // Any form of git reset is considered sensitive.
                        return Some(SensitiveGitKind::Reset);
                    }
                    "revert" => {
                        // Any form of git revert is considered sensitive.
                        return Some(SensitiveGitKind::Revert);
                    }
                    // Future: consider `git branch -D/-m` as branch‑modifying, but keep
                    // this minimal to avoid over‑blocking normal workflows.
                    _ => {}
                }
            }
        }
        None
    }

    fn strip_leading_confirm_prefix(argv: &mut Vec<String>) -> bool {
        if argv.is_empty() {
            return false;
        }

        let first = argv[0].trim().to_string();
        for prefix in ["confirm:", "CONFIRM:"] {
            if first == prefix {
                argv.remove(0);
                return true;
            }
            if let Some(rest) = first.strip_prefix(prefix) {
                let trimmed = rest.trim_start();
                if trimmed.is_empty() {
                    argv.remove(0);
                } else {
                    argv[0] = trimmed.to_string();
                }
                return true;
            }
        }

        false
    }

    fn guidance_for_sensitive_git(kind: SensitiveGitKind, original_label: &str, original_value: &str, suggested: &str) -> String {
        match kind {
            SensitiveGitKind::BranchChange => format!(
                "Blocked git checkout/switch on a branch. Switching branches can discard or hide in-progress changes. Only continue if the user explicitly requested this branch change. Resend with 'confirm:' if you intend to proceed.\n\n{original_label}: {original_value}\nresend_exact_argv: {suggested}"
            ),
            SensitiveGitKind::PathCheckout => format!(
                "Blocked git checkout -- <paths>. This command overwrites local modifications to the specified files. Consider backing up the files first. If you intentionally want to discard those edits, resend the exact command prefixed with 'confirm:'.\n\n{original_label}: {original_value}\nresend_exact_argv: {suggested}"
            ),
            SensitiveGitKind::Reset => format!(
                "Blocked git reset. Reset rewrites the working tree/index and may delete local work. Consider backing up the files first. If backups exist and this was explicitly requested, resend prefixed with 'confirm:'.\n\n{original_label}: {original_value}\nresend_exact_argv: {suggested}"
            ),
            SensitiveGitKind::Revert => format!(
                "Blocked git revert. Reverting commits alters history and should only happen when the user asks for it. If that’s the case, resend the command with 'confirm:'.\n\n{original_label}: {original_value}\nresend_exact_argv: {suggested}"
            ),
        }
    }

    fn guidance_for_dry_run_guard(
        analysis: &DryRunAnalysis,
        original_label: &str,
        original_value: &str,
        resend_exact_argv: Vec<String>,
    ) -> String {
        let suggested_confirm = serde_json::to_string(&resend_exact_argv)
            .unwrap_or_else(|_| "<failed to serialize suggested argv>".to_string());
        let suggested_dry_run = analysis
            .suggested_dry_run()
            .unwrap_or_else(|| "<no canonical dry-run variant; remove mutating flags or use confirm:>".to_string());
        format!(
            "Blocked {} without a prior dry run. Run the dry-run variant first or resend with 'confirm:' if explicitly requested.\n\n{}: {}\nresend_exact_argv: {}\nsuggested_dry_run: {}",
            analysis.display_name(),
            original_label,
            original_value,
            suggested_confirm,
            suggested_dry_run
        )
    }


    // If the argv is a shell wrapper, analyze and optionally strip `confirm:`.
    let mut params = params;
    let seq_hint_for_exec = seq_hint;
    let otel_event_manager = sess.client.get_otel_event_manager();
    let tool_name = "local_shell";
    if let Some((script_index, script)) = extract_shell_script_from_wrapper(&params.command) {
        let trimmed = script.trim_start();
        let confirm_prefixes = ["confirm:", "CONFIRM:"];
        let has_confirm_prefix = confirm_prefixes
            .iter()
            .any(|p| trimmed.starts_with(p));

        // If no confirm prefix and it looks like a sensitive git command, reject with guidance.
        if !has_confirm_prefix {
            if let Some(pattern) = if sess.confirm_guard.is_empty() {
                None
            } else {
                sess.confirm_guard.matched_pattern(trimmed)
            } {
                let mut argv_confirm = params.command.clone();
                argv_confirm[script_index] = format!("confirm: {}", script.trim_start());
                let suggested = serde_json::to_string(&argv_confirm)
                    .unwrap_or_else(|_| "<failed to serialize suggested argv>".to_string());
                let guidance = pattern.guidance("original_script", &script, &suggested);

                let order = sess.next_background_order(&sub_id, attempt_req, output_index);
                sess
                    .notify_background_event_with_order(
                        &sub_id,
                        order,
                        format!("Command guard: {guidance}"),
                    )
                    .await;

                return ResponseInputItem::FunctionCallOutput {
                    call_id,
                    output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(guidance), success: None },
                };
            }

            if let Some(kind) = detect_sensitive_git(trimmed) {
                // Provide the exact argv the model should resend with the confirm prefix.
                let mut argv_confirm = params.command.clone();
                argv_confirm[script_index] = format!("confirm: {}", script.trim_start());
                let suggested = serde_json::to_string(&argv_confirm)
                    .unwrap_or_else(|_| "<failed to serialize suggested argv>".to_string());

                let guidance = guidance_for_sensitive_git(kind, "original_script", &script, &suggested);

                let order = sess.next_background_order(&sub_id, attempt_req, output_index);
                sess
                    .notify_background_event_with_order(
                        &sub_id,
                        order,
                        format!("Command guard: {}", guidance.clone()),
                    )
                    .await;

                return ResponseInputItem::FunctionCallOutput {
                    call_id,
                    output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(guidance), success: None },
                };
            }
        }

        // If confirm prefix present, strip it before execution.
        if has_confirm_prefix {
            let without_prefix = confirm_prefixes
                .iter()
                .find_map(|p| {
                    let t = trimmed.strip_prefix(p)?;
                    Some(t.trim_start().to_string())
                })
                .unwrap_or_else(|| trimmed.to_string());
            params.command[script_index] = without_prefix;
        }

        let dry_run_analysis = analyze_command(&params.command);
        if !has_confirm_prefix
            && let Some(analysis) = dry_run_analysis.as_ref()
                && analysis.disposition == DryRunDisposition::Mutating {
                    let needs_dry_run = {
                        let state = sess.state.lock().unwrap();
                        !state.dry_run_guard.has_recent_dry_run(analysis.key)
                    };
                    if needs_dry_run {
                        let mut argv_confirm = params.command.clone();
                        argv_confirm[script_index] = format!("confirm: {}", params.command[script_index].trim_start());
                        let guidance = guidance_for_dry_run_guard(
                            analysis,
                            "original_script",
                            &params.command[script_index],
                            argv_confirm,
                        );

                        let order = sess.next_background_order(&sub_id, attempt_req, output_index);
                        sess
                            .notify_background_event_with_order(
                                &sub_id,
                                order,
                                format!("Command guard: {}", guidance.clone()),
                            )
                            .await;

                        return ResponseInputItem::FunctionCallOutput {
                            call_id,
                            output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(guidance), success: None },
                        };
                    }
                }
    }

    strip_leading_confirm_prefix(&mut params.command);

    if let Some(redundant) = detect_redundant_cd(&params.command, &params.cwd) {
        let guidance = guidance_for_redundant_cd(&redundant);
        let order = sess.next_background_order(&sub_id, attempt_req, output_index);
        sess
            .notify_background_event_with_order(
                &sub_id,
                order,
                format!("Command guard: {}", guidance.clone()),
            )
            .await;

        return ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(guidance),
                success: None,
            },
        };
    }

    if let Some(cat_guard) = detect_cat_write(&params.command) {
        let guidance = guidance_for_cat_write(&cat_guard);
        let order = sess.next_background_order(&sub_id, attempt_req, output_index);
        sess
            .notify_background_event_with_order(
                &sub_id,
                order,
                format!("Command guard: {}", guidance.clone()),
            )
            .await;

        return ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(guidance),
                success: None,
            },
        };
    }

    if let Some(python_guard) = detect_python_write(&params.command) {
        let guidance = guidance_for_python_write(&python_guard);
        let order = sess.next_background_order(&sub_id, attempt_req, output_index);
        sess
            .notify_background_event_with_order(
                &sub_id,
                order,
                format!("Command guard: {}", guidance.clone()),
            )
            .await;

        return ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(guidance),
                success: None,
            },
        };
    }

    // If no shell wrapper, perform a lightweight argv inspection for sensitive git commands.
    if extract_shell_script_from_wrapper(&params.command).is_none() {
        let joined = params.command.join(" ");
        if !sess.confirm_guard.is_empty()
            && let Some(pattern) = sess.confirm_guard.matched_pattern(&joined) {
                let suggested = serde_json::to_string(&vec![
                    "bash".to_string(),
                    "-lc".to_string(),
                    format!("confirm: {}", joined),
                ])
                .unwrap_or_else(|_| "<failed to serialize suggested argv>".to_string());
                let guidance = pattern.guidance(
                    "original_argv",
                    &format!("{:?}", params.command),
                    &suggested,
                );

                let order = sess.next_background_order(&sub_id, attempt_req, output_index);
                sess
                    .notify_background_event_with_order(
                        &sub_id,
                        order,
                        format!("Command guard: {}", guidance.clone()),
                    )
                    .await;

                return ResponseInputItem::FunctionCallOutput {
                    call_id,
                    output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(guidance), success: None },
                };
            }

        if let Some(analysis) = analyze_command(&params.command)
            && analysis.disposition == DryRunDisposition::Mutating {
                let needs_dry_run = {
                    let state = sess.state.lock().unwrap();
                    !state.dry_run_guard.has_recent_dry_run(analysis.key)
                };
                if needs_dry_run {
                    let resend = vec![
                        "bash".to_string(),
                        "-lc".to_string(),
                        format!("confirm: {}", joined),
                    ];
                    let guidance = guidance_for_dry_run_guard(
                        &analysis,
                        "original_argv",
                        &format!("{:?}", params.command),
                        resend,
                    );

                    let order = sess.next_background_order(&sub_id, attempt_req, output_index);
                    sess
                        .notify_background_event_with_order(
                            &sub_id,
                            order,
                            format!("Command guard: {}", guidance.clone()),
                        )
                        .await;

                    return ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(guidance), success: None },
                    };
                }
            }

        fn strip_tok2(t: &str) -> &str { t.trim_matches(|c| matches!(c, '(' | ')' | '{' | '}' | '\'' | '"')) }
        let mut i = 0usize;
        // Skip env assignments and simple wrappers at the front
        while i < params.command.len() {
            let tok = strip_tok2(&params.command[i]);
            if tok.is_empty() { i += 1; continue; }
            if tok.contains('=') && !tok.starts_with('=') && !tok.starts_with('-') { i += 1; continue; }
            if matches!(tok, "env" | "sudo" | "command" | "time" | "nohup" | "nice") {
                i += 1;
                while i < params.command.len() && strip_tok2(&params.command[i]).starts_with('-') { i += 1; }
                continue;
            }
            break;
        }
        if i < params.command.len() {
            let cmd = strip_tok2(&params.command[i]);
            if cmd.ends_with("/git") || cmd == "git" {
                i += 1;
                while i < params.command.len() {
                    let t = strip_tok2(&params.command[i]);
                    if t.is_empty() { i += 1; continue; }
                    if matches!(t, "-C" | "--git-dir" | "--work-tree" | "-c") {
                        i += 1; if i < params.command.len() { i += 1; }
                        continue;
                    }
                    if t.starts_with("--git-dir=") || t.starts_with("--work-tree=") || t.starts_with("-c") { i += 1; continue; }
                    if t.starts_with('-') { i += 1; continue; }
                    break;
                }
                if i < params.command.len() {
                    let sub = strip_tok2(&params.command[i]);
                    let args: Vec<&str> = params.command[i + 1..].iter().map(|t| strip_tok2(t)).collect();
                    let kind = match sub {
                        "checkout" => {
                            if args.contains(&"--") {
                                Some(SensitiveGitKind::PathCheckout)
                            } else if args.iter().any(|a| matches!(*a, "-b" | "-B" | "--orphan" | "--detach"))
                                || args.first().copied() == Some("-")
                            {
                                Some(SensitiveGitKind::BranchChange)
                            } else if let Some(first_arg) = args.first() {
                                let a = *first_arg;
                                if !a.starts_with('-') && a != "." && a != ".." {
                                    Some(SensitiveGitKind::BranchChange)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        "switch" => Some(SensitiveGitKind::BranchChange),
                        "reset" => Some(SensitiveGitKind::Reset),
                        "revert" => Some(SensitiveGitKind::Revert),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        let suggested = serde_json::to_string(&vec![
                            "bash".to_string(),
                            "-lc".to_string(),
                            format!("confirm: {}", params.command.join(" ")),
                        ]).unwrap_or_else(|_| "<failed to serialize suggested argv>".to_string());

                        let guidance = guidance_for_sensitive_git(kind, "original_argv", &format!("{:?}", params.command), &suggested);

                        let order = sess.next_background_order(&sub_id, attempt_req, output_index);
                        sess
                            .notify_background_event_with_order(
                                &sub_id,
                                order,
                                format!("Command guard: {}", guidance.clone()),
                            )
                            .await;

                        return ResponseInputItem::FunctionCallOutput { call_id, output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(guidance), success: None } };
                    }
                }
            }
        }
    }

    // Check if this was a patch, and apply it in-process if so.
    match sess
        .maybe_parse_apply_patch_verified(&params.command, &params.cwd)
        .await
    {
        MaybeApplyPatchVerified::Body(action) => {
            if let Some(branch_root) = git_worktree::branch_worktree_root(sess.get_cwd())
                && let Some(guidance) = guard_apply_patch_outside_branch(&branch_root, &action) {
                    let order = sess.next_background_order(&sub_id, attempt_req, output_index);
                    sess
                        .notify_background_event_with_order(
                            &sub_id,
                            order,
                            format!("Command guard: {}", guidance.clone()),
                        )
                        .await;

                    return ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(guidance), success: None },
                    };
                }

            let changes = convert_apply_patch_to_protocol(&action);
            turn_diff_tracker.on_patch_begin(&changes);

            let mut hook_ctx = ExecCommandContext {
                sub_id: sub_id.clone(),
                call_id: call_id.clone(),
                command_for_display: params.command.clone(),
                cwd: params.cwd.clone(),
                apply_patch: Some(ApplyPatchCommandContext {
                    user_explicitly_approved_this_action: false,
                    changes: changes.clone(),
                }),
            };

            // FileBeforeWrite hook for apply_patch
            sess
                .run_hooks_for_exec_event(
                    turn_diff_tracker,
                    ProjectHookEvent::FileBeforeWrite,
                    &hook_ctx,
                    &params,
                    None,
                    attempt_req,
                )
                .await;

            let patch_start = std::time::Instant::now();

            match apply_patch::apply_patch(
                sess,
                &sub_id,
                &call_id,
                attempt_req,
                output_index,
                action,
            )
            .await
            {
                ApplyPatchResult::Reply(item) => return item,
                ApplyPatchResult::Applied(run) => {
                    if let Some(ctx) = hook_ctx.apply_patch.as_mut() { ctx.user_explicitly_approved_this_action = !run.auto_approved; }

                    let order_begin = crate::protocol::OrderMeta {
                        request_ordinal: attempt_req,
                        output_index,
                        sequence_number: seq_hint,
                    };
                    let begin_event = EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
                        call_id: call_id.clone(),
                        auto_approved: run.auto_approved,
                        changes,
                    });
                    let event = sess.make_event_with_order(&sub_id, begin_event, order_begin, seq_hint);
                    let _ = sess.tx_event.send(event).await;

                    let order_end = crate::protocol::OrderMeta {
                        request_ordinal: attempt_req,
                        output_index,
                        sequence_number: seq_hint.map(|h| h.saturating_add(1)),
                    };
                    let end_event = EventMsg::PatchApplyEnd(PatchApplyEndEvent {
                        call_id: call_id.clone(),
                        stdout: run.stdout.clone(),
                        stderr: run.stderr.clone(),
                        success: run.success,
                    });
                    let event = sess.make_event_with_order(
                        &sub_id,
                        end_event,
                        order_end,
                        seq_hint.map(|h| h.saturating_add(1)),
                    );
                    let _ = sess.tx_event.send(event).await;

                    let hook_output = ExecToolCallOutput {
                        exit_code: if run.success { 0 } else { 1 },
                        stdout: StreamOutput::new(run.stdout.clone()),
                        stderr: StreamOutput::new(run.stderr.clone()),
                        aggregated_output: StreamOutput::new({
                            if run.stdout.is_empty() {
                                run.stderr.clone()
                            } else if run.stderr.is_empty() {
                                run.stdout.clone()
                            } else {
                                format!("{}\n{}", run.stdout, run.stderr)
                            }
                        }),
                        duration: patch_start.elapsed(),
                        timed_out: false,
                    };

                    sess
                        .run_hooks_for_exec_event(
                            turn_diff_tracker,
                            ProjectHookEvent::FileAfterWrite,
                            &hook_ctx,
                            &params,
                            Some(&hook_output),
                            attempt_req,
                        )
                        .await;

                    if let Ok(Some(unified_diff)) = turn_diff_tracker.get_unified_diff() {
                        let diff_event = sess.make_event(
                            &sub_id,
                            EventMsg::TurnDiff(TurnDiffEvent { unified_diff }),
                        );
                        let _ = sess.tx_event.send(diff_event).await;
                    }

                    let mut content = run.stdout;
                    if !run.success && !run.stderr.is_empty() {
                        if !content.is_empty() {
                            content.push('\n');
                        }
                        content.push_str(&format!("stderr: {}", run.stderr));
                    }
                    if let Some(summary) = run.harness_summary_json
                        && !summary.is_empty() {
                            if !content.is_empty() {
                                content.push('\n');
                            }
                            content.push_str(&summary);
                        }

                    return ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(content),
                            success: Some(run.success),
                        },
                    };
                }
            }
        }
        MaybeApplyPatchVerified::CorrectnessError(parse_error) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!("error: {parse_error:#}")),
                    success: None,
                },
            };
        }
        MaybeApplyPatchVerified::ShellParseError(error) => {
            trace!("Failed to parse shell command, {error:?}");
        }
        MaybeApplyPatchVerified::NotApplyPatch => {}
    }

    let safety = {
        let state = sess.state.lock().unwrap();
        let command_safety_context =
            crate::command_safety::context::CommandSafetyContext::from_shell(&sess.user_shell)
                .with_command_shell(&params.command);
        let safety_config = crate::safety::CommandSafetyEvaluationConfig {
            context: command_safety_context,
            safe_rules: sess.safe_command_rules,
            dangerous_rules: sess.dangerous_command_rules,
            dangerous_command_detection_enabled: sess.dangerous_command_detection_enabled,
        };
        assess_command_safety(
            &params.command,
            safety_config,
            sess.approval_policy,
            &sess.sandbox_policy,
            &state.approved_commands,
            params.with_escalated_permissions.unwrap_or(false),
        )
    };
    let command_for_display = params.command.clone();
    let harness_summary_json: Option<String> = None;

    let sandbox_type = match safety {
        SafetyCheck::AutoApprove {
            sandbox_type,
            user_explicitly_approved,
        } => {
            if let Some(manager) = otel_event_manager.as_ref() {
                let (decision_for_log, source) = if user_explicitly_approved {
                    (
                        ReviewDecision::ApprovedForSession,
                        ToolDecisionSource::User,
                    )
                } else {
                    (ReviewDecision::Approved, ToolDecisionSource::Config)
                };
                manager.tool_decision(
                    tool_name,
                    call_id.as_str(),
                    to_proto_review_decision(decision_for_log),
                    source,
                );
            }
            sandbox_type
        }
        SafetyCheck::AskUser => {
            let rx_approve = sess
                .request_command_approval(
                    sub_id.clone(),
                    call_id.clone(),
                    params.command.clone(),
                    params.cwd.clone(),
                    params.justification.clone(),
                )
                .await;

            let decision = rx_approve.await.unwrap_or_default();
            if let Some(manager) = otel_event_manager.as_ref() {
                manager.tool_decision(
                    tool_name,
                    call_id.as_str(),
                    to_proto_review_decision(decision),
                    ToolDecisionSource::User,
                );
            }

            match decision {
                ReviewDecision::Approved => {}
                ReviewDecision::ApprovedForSession => {
                    sess.add_approved_command(ApprovedCommandPattern::new(
                        params.command.clone(),
                        ApprovedCommandMatchKind::Exact,
                        None,
                    ));
                }
                ReviewDecision::Denied | ReviewDecision::Abort => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text("exec command rejected by user".to_string()),
                            success: None,
                        },
                    };
                }
            }
            // No sandboxing is applied because the user has given
            // explicit approval. Often, we end up in this case because
            // the command cannot be run in a sandbox, such as
            // installing a new dependency that requires network access.
            SandboxType::None
        }
        SafetyCheck::Reject { reason } => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!("exec command rejected: {reason}")),
                    success: None,
                },
            };
        }
    };

    let exec_command_context = ExecCommandContext {
        sub_id: sub_id.clone(),
        call_id: call_id.clone(),
        command_for_display: command_for_display.clone(),
        cwd: params.cwd.clone(),
        apply_patch: None,
    };

    let display_label = crate::util::strip_bash_lc_and_escape(&exec_command_context.command_for_display);
    let params = maybe_run_with_user_profile(params, sess);

    // ToolBefore hook for shell/container.exec commands
    let params_for_hooks = params.clone();
    sess
        .run_hooks_for_exec_event(
            turn_diff_tracker,
            ProjectHookEvent::ToolBefore,
            &exec_command_context,
            &params_for_hooks,
            None,
            attempt_req,
        )
        .await;

    // Prepare tail buffer and background registry entry
    let tail_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let notify = std::sync::Arc::new(tokio::sync::Notify::new());
    let result_cell: std::sync::Arc<std::sync::Mutex<Option<ExecToolCallOutput>>> = std::sync::Arc::new(std::sync::Mutex::new(None));
    let backgrounded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let suppress_event_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let order_meta_for_end = crate::protocol::OrderMeta {
        request_ordinal: attempt_req,
        output_index,
        sequence_number: seq_hint_for_exec.map(|h| h.saturating_add(1)),
    };
    let order_meta_for_deltas = crate::protocol::OrderMeta {
        request_ordinal: attempt_req,
        output_index,
        sequence_number: None,
    };
    {
        let mut st = sess.state.lock().unwrap();
        st.background_execs.insert(
            call_id.clone(),
            BackgroundExecState {
                notify: notify.clone(),
                result_cell: result_cell.clone(),
                tail_buf: Some(tail_buf.clone()),
                cmd_display: display_label.clone(),
                suppress_event: suppress_event_flag.clone(),
                task_handle: None,
                order_meta_for_end: order_meta_for_end.clone(),
                sub_id: sub_id.clone(),
            },
        );
    }

    let sess_for_hooks = sess.self_handle.upgrade();
    let params_for_after_hooks = params_for_hooks.clone();
    let exec_ctx_for_hooks = exec_command_context.clone();
    let exec_ctx_for_task = exec_command_context.clone();
    let attempt_req_for_task = attempt_req;

    // Emit BEGIN event using the normal path so the TUI shows a running cell
    sess
        .on_exec_command_begin(
            turn_diff_tracker,
            exec_command_context.clone(),
            seq_hint_for_exec,
            output_index,
            attempt_req,
        )
        .await;

    // Spawn the runner that streams output and, on completion, emits END and records result.
    let tx_event = sess.tx_event.clone();
    let sub_id_for_events = sub_id.clone();
    let call_id_for_events = call_id.clone();
    let sandbox_policy = sess.sandbox_policy.clone();
    let sandbox_cwd = sess.get_cwd().to_path_buf();
    let code_linux_sandbox_exe = sess.code_linux_sandbox_exe.clone();
    let exec_spool_dir_for_task = if sess.client.debug_enabled() {
        Some(
            sess.client
                .code_home()
                .join("debug_logs")
                .join("exec"),
        )
    } else {
        None
    };
    let result_cell_for_task = result_cell.clone();
    let notify_task = notify.clone();
    let tail_buf_task = tail_buf.clone();
    let backgrounded_task = backgrounded.clone();
    let suppress_event_flag_task = suppress_event_flag.clone();
    let display_label_task = display_label.clone();
    let tool_output_max_bytes = sess.tool_output_max_bytes;
    let task_handle = tokio::spawn(async move {
        // Build stdout stream with tail capture. We cannot stamp via `Session` here,
        // but deltas will be delivered with neutral ordering which the UI tolerates.
        let stdout_stream = if exec_ctx_for_task.apply_patch.is_some() {
            None
        } else {
            Some(StdoutStream {
                sub_id: sub_id_for_events.clone(),
                call_id: call_id_for_events.clone(),
                tx_event: tx_event.clone(),
                session: None,
                tail_buf: Some(tail_buf_task.clone()),
                order: Some(order_meta_for_deltas.clone()),
                spool_dir: exec_spool_dir_for_task.clone(),
            })
        };

        let start = std::time::Instant::now();
        let res = crate::exec::process_exec_tool_call(
            params.clone(),
            sandbox_type,
            &sandbox_policy,
            &sandbox_cwd,
            &code_linux_sandbox_exe,
            stdout_stream,
        )
        .await;

        // Normalize to ExecToolCallOutput
        let (out, exit_code) = match res {
            Ok(o) => { let exit = o.exit_code; (o, exit) },
            Err(CodexErr::Sandbox(SandboxErr::Timeout { output })) => (output.as_ref().clone(), 124),
            Err(e) => {
                let msg = get_error_message_ui(&e);
                (
                    ExecToolCallOutput {
                        exit_code: -1,
                        stdout: StreamOutput::new(String::new()),
                        stderr: StreamOutput::new(msg.clone()),
                        aggregated_output: StreamOutput::new(msg),
                        duration: start.elapsed(),
                        timed_out: false,
                    },
                    -1,
                )
            }
        };

        // Emit END event directly
        let end_msg = EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: call_id_for_events.clone(),
            stdout: out.stdout.text.clone(),
            stderr: out.stderr.text.clone(),
            exit_code,
            duration: out.duration,
        });
        let ev = Event { id: sub_id_for_events.clone(), event_seq: 0, msg: end_msg, order: Some(order_meta_for_end) };
        let _ = tx_event.send(ev).await;

        // Store result for waiters
        {
            let mut slot = result_cell_for_task.lock().unwrap();
            *slot = Some(out.clone());
        }

        if backgrounded_task.load(std::sync::atomic::Ordering::Relaxed)
            && let Some(sess_arc) = sess_for_hooks.clone() {
                let mut hook_tracker = TurnDiffTracker::new();
                sess_arc
                    .run_hooks_for_exec_event(
                        &mut hook_tracker,
                        ProjectHookEvent::ToolAfter,
                        &exec_ctx_for_hooks,
                        &params_for_after_hooks,
                        Some(&out),
                        attempt_req_for_task,
                    )
                    .await;
            }
        // Only emit background completion notifications if the command actually backgrounded
        if backgrounded_task.load(std::sync::atomic::Ordering::Relaxed) {
            if !suppress_event_flag_task.load(std::sync::atomic::Ordering::Relaxed) {
                let label = display_label_task.trim();
                let message = if label.is_empty() {
                    format!("Background shell '{call_id_for_events}' completed.")
                } else {
                    format!("{label} completed in background")
                };
                let bg_event = EventMsg::BackgroundEvent(BackgroundEventEvent { message });
                let ev = Event { id: sub_id_for_events.clone(), event_seq: 0, msg: bg_event, order: None };
                let _ = tx_event.send(ev).await;

                if let Some(tx) = TX_SUB_GLOBAL.get() {
                    let header_label = if label.is_empty() {
                        format!("call_id={call_id_for_events}")
                    } else {
                        display_label_task.clone()
                    };
                    let header = format!("Background shell completed ({header_label}), exit_code={}, duration={:?}.", out.exit_code, out.duration);
                    let full_body = format_exec_output_str(&out);
                    let body = truncate_exec_output_for_storage(
                        &sandbox_cwd,
                        &sub_id_for_events,
                        &call_id_for_events,
                        &full_body,
                        tool_output_max_bytes,
                    );
                    let dev_text = format!("{header}\n\n{body}");
                    let _ = tx
                        .send(Submission { id: uuid::Uuid::new_v4().to_string(), op: Op::AddPendingInputDeveloper { text: dev_text } })
                        .await;
                }
            }
            if let Some(n) = ANY_BG_NOTIFY.get() { n.notify_waiters(); }
        }
        notify_task.notify_waiters();
    });

    {
        let mut st = sess.state.lock().unwrap();
        if let Some(bg) = st.background_execs.get_mut(&call_id) {
            bg.task_handle = Some(task_handle);
        }
    }

    // Wait up to 10 seconds for completion
    let waited = tokio::time::timeout(std::time::Duration::from_secs(10), notify.notified()).await;
    if waited.is_ok() {
        // Completed within 10s - return the real output and drop the background entry.
        let done_opt = {
            let mut st = sess.state.lock().unwrap();
            st.background_execs
                .remove(&call_id)
                .and_then(|bg| bg.result_cell.lock().unwrap().clone())
                .or_else(|| {
                    st.background_execs
                        .iter()
                        .find_map(|(k, v)| {
                            if v.result_cell.lock().unwrap().is_some() {
                                Some(k.clone())
                            } else {
                                None
                            }
                        })
                        .and_then(|k| st.background_execs.remove(&k))
                        .and_then(|bg| bg.result_cell.lock().unwrap().clone())
                })
        };
        if let Some(done) = done_opt {
            let is_success = done.exit_code == 0;
            let mut content = format_exec_output_with_limit(
                sess.get_cwd(),
                &sub_id,
                &call_id,
                &done,
                sess.tool_output_max_bytes,
            );
            if let Some(harness) = harness_summary_json.as_ref()
                && !harness.is_empty() {
                    content.push('\n');
                    content.push_str(harness);
                }

            sess
                .run_hooks_for_exec_event(
                    turn_diff_tracker,
                    ProjectHookEvent::ToolAfter,
                    &exec_command_context,
                    &params_for_hooks,
                    Some(&done),
                    attempt_req,
                )
                .await;

            return ResponseInputItem::FunctionCallOutput {
                call_id: call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(content),
                    success: Some(is_success),
                },
            };
        } else {
            // Fallback (should not happen): indicate completion without detail
            let msg = "Command completed.".to_string();
            return ResponseInputItem::FunctionCallOutput { call_id: call_id.clone(), output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(msg), success: Some(true) } };
        }
    }

    // Still running: mark as backgrounded and return background notice + tail and instructions
    backgrounded.store(true, std::sync::atomic::Ordering::Relaxed);
    let tail = String::from_utf8_lossy(&tail_buf.lock().unwrap()).to_string();
    let header = format!(
        "Command running in background (call_id={call_id}).\nTo wait: wait(call_id=\"{call_id}\")\nYou can continue other work or wait. You'll be notified when the command completes."
    );
    let msg = if tail.is_empty() {
        header
    } else {
        format!("{header}\n\nOutput so far (tail):\n{tail}")
    };
    ResponseInputItem::FunctionCallOutput { call_id: call_id.clone(), output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(msg), success: Some(true) } }
}

fn truncate_exec_output_for_storage(
    cwd: &Path,
    sub_id: &str,
    call_id: &str,
    full: &str,
    max_tool_output_bytes: usize,
) -> String {
    let (maybe_truncated, was_truncated, _, _) =
        truncate_middle_bytes(full, max_tool_output_bytes);
    if !was_truncated {
        return maybe_truncated;
    }

    let safe_call_id = crate::fs_sanitize::safe_path_component(call_id, "exec");
    let filename = format!("exec-{safe_call_id}.txt");
    let file_note = match ensure_agent_dir(cwd, sub_id)
        .and_then(|dir| write_agent_file(&dir, &filename, full))
    {
        Ok(path) => format!("\n\n[Full output saved to: {}]", path.display()),
        Err(e) => format!("\n\n[Full output was too large and truncation applied; failed to save file: {e}]")
    };
    let mut truncated = maybe_truncated;
    truncated.push_str(&file_note);
    truncated
}

/// Exec output serialized for the model. If the payload is too large,
/// write the full output to a file and include a truncated preview here.
fn format_exec_output_with_limit(
    cwd: &Path,
    sub_id: &str,
    call_id: &str,
    exec_output: &ExecToolCallOutput,
    max_tool_output_bytes: usize,
) -> String {
    let full = format_exec_output_str(exec_output);
    let final_output =
        truncate_exec_output_for_storage(cwd, sub_id, call_id, &full, max_tool_output_bytes);
    format_exec_output_payload(exec_output, &final_output)
}

fn extract_shell_script_from_wrapper(argv: &[String]) -> Option<(usize, String)> {
    // Return (index_of_script, script) if argv matches: <shell> (-lc|-c) <script>
    if argv.len() == 3 {
        let shell = std::path::Path::new(&argv[0])
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let is_shell = matches!(shell, "bash" | "sh" | "zsh");
        let is_flag = matches!(argv[1].as_str(), "-lc" | "-c");
        if is_shell && is_flag {
            return Some((2, argv[2].clone()));
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CatWriteSuggestion {
    label: &'static str,
    original_value: String,
}

fn detect_cat_write(argv: &[String]) -> Option<CatWriteSuggestion> {
    if let Some((_, script)) = extract_shell_script_from_wrapper(argv)
        && script_contains_cat_write(&script) {
            return Some(CatWriteSuggestion {
                label: "original_script",
                original_value: script,
            });
        }

    None
}

fn script_contains_cat_write(script: &str) -> bool {
    script
        .lines()
        .any(line_contains_cat_heredoc_write)
}

fn line_contains_cat_heredoc_write(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }

    let lower = line.to_ascii_lowercase();
    if !lower.contains("<<") || !lower.contains('>') {
        return false;
    }

    let bytes = lower.as_bytes();
    let mut idx = 0;
    while idx + 3 <= bytes.len() {
        if bytes[idx..].starts_with(b"cat") {
            if idx > 0 {
                let prev = bytes[idx - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    idx += 1;
                    continue;
                }
            }

            let after = &lower[idx + 3..];
            let after_trimmed = after.trim_start();
            if after_trimmed.starts_with("<<") {
                let heredoc_offset_in_after = after.find("<<").unwrap_or(0);
                let heredoc_offset = idx + 3 + heredoc_offset_in_after;
                let redirect_section = &lower[heredoc_offset..];
                if let Some(rel_redirect_idx) = redirect_section.find('>') {
                    let redirect_idx = heredoc_offset + rel_redirect_idx;
                    if redirect_idx > heredoc_offset {
                        let redirect_slice = &lower[redirect_idx..];
                        if redirect_slice.starts_with(">&") {
                            idx += 1;
                            continue;
                        }
                        let after_gt = redirect_slice[1..].trim_start();
                        if after_gt.starts_with('&') {
                            idx += 1;
                            continue;
                        }
                        if after_gt.starts_with('(') {
                            idx += 1;
                            continue;
                        }
                        return true;
                    }
                }
            }
        }
        idx += 1;
    }

    false
}

fn guard_apply_patch_outside_branch(branch_root: &Path, action: &ApplyPatchAction) -> Option<String> {
    let branch_norm = match normalize_absolute(branch_root) {
        Some(path) => path,
        None => {
            return Some(format!(
                "apply_patch blocked: failed to resolve /branch worktree root {}. Stay inside the worktree until you finish with `/merge`.",
                branch_root.display()
            ));
        }
    };
    let action_cwd_norm = match normalize_absolute(&action.cwd) {
        Some(path) => path,
        None => {
            return Some(format!(
                "apply_patch blocked: the command resolved outside the /branch worktree (cwd {}). Stay inside {} until you finish with `/merge`.",
                action.cwd.display(),
                branch_root.display()
            ));
        }
    };
    if !path_within(&action_cwd_norm, &branch_norm) {
        return Some(format!(
            "apply_patch blocked: the active /branch worktree is {} but the command tried to run from {}. Stay inside the worktree until you finish with `/merge`.",
            branch_root.display(),
            action.cwd.display()
        ));
    }

    for path in action.changes().keys() {
        let normalized = match normalize_absolute(path) {
            Some(value) => value,
            None => {
                return Some(format!(
                    "apply_patch blocked: could not resolve patch target {} inside worktree {}. Keep edits within the /branch directory.",
                    path.display(),
                    branch_root.display()
                ));
            }
        };
        if !path_within(&normalized, &branch_norm) {
            return Some(format!(
                "apply_patch blocked: patch would modify {} outside the active /branch worktree {}. Apply changes from within the worktree before `/merge`.",
                path.display(),
                branch_root.display()
            ));
        }
    }

    None
}

fn normalize_absolute(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return None;
                }
            }
            Component::Normal(part) => result.push(part),
        }
    }
    if result.as_os_str().is_empty() {
        None
    } else {
        Some(result)
    }
}

fn path_within(path: &Path, base: &Path) -> bool {
    path.starts_with(base)
}

fn guidance_for_cat_write(suggestion: &CatWriteSuggestion) -> String {
    format!(
        "Blocked cat heredoc that writes files directly. Use apply_patch to edit files so changes stay reviewable.\n\n{}: {}",
        suggestion.label,
        suggestion.original_value
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PythonWriteSuggestion {
    label: &'static str,
    original_value: String,
}

fn detect_python_write(argv: &[String]) -> Option<PythonWriteSuggestion> {
    if let Some((_, script)) = extract_shell_script_from_wrapper(argv)
        && script_contains_python_write(&script) {
            return Some(PythonWriteSuggestion {
                label: "original_script",
                original_value: script,
            });
        }

    detect_python_write_in_argv(argv)
}

fn detect_python_write_in_argv(argv: &[String]) -> Option<PythonWriteSuggestion> {
    if argv.is_empty() {
        return None;
    }

    if !is_python_command(&argv[0]) {
        return None;
    }

    if argv.len() >= 3 && argv[1] == "-c" {
        let code = &argv[2];
        if python_code_writes_files(code) {
            return Some(PythonWriteSuggestion {
                label: "python_inline_script",
                original_value: code.clone(),
            });
        }
    }

    None
}

fn script_contains_python_write(script: &str) -> bool {
    let lower = script.to_ascii_lowercase();
    if !(lower.contains("python ")
        || lower.contains("python3")
        || lower.contains("python\n"))
    {
        return false;
    }
    contains_python_write_keywords(&lower)
}

fn python_code_writes_files(code: &str) -> bool {
    contains_python_write_keywords(&code.to_ascii_lowercase())
}

fn contains_python_write_keywords(lower: &str) -> bool {
    const KEYWORDS: &[&str] = &["write_text(", "write_bytes(", ".write_text(", ".write_bytes("];
    KEYWORDS.iter().any(|needle| lower.contains(needle))
}

fn is_python_command(cmd: &str) -> bool {
    std::path::Path::new(cmd)
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            let lower = name.to_ascii_lowercase();
            matches!(lower.as_str(), "python" | "python3" | "python2")
        })
        .unwrap_or(false)
}

fn guidance_for_python_write(suggestion: &PythonWriteSuggestion) -> String {
    format!(
        "Blocked python command that writes files directly. Use apply_patch to edit files so changes stay reviewable.\n\n{}: {}",
        suggestion.label,
        suggestion.original_value
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RedundantCdSuggestion {
    label: &'static str,
    original_value: String,
    suggested: Vec<String>,
    target_arg: String,
    cwd: PathBuf,
}

fn detect_redundant_cd(argv: &[String], cwd: &Path) -> Option<RedundantCdSuggestion> {
    let normalized_cwd = normalize_path(cwd);
    if let Some((script_index, script)) = extract_shell_script_from_wrapper(argv)
        && let Some(suggestion) = detect_redundant_cd_in_shell(
            argv,
            script_index,
            &script,
            cwd,
            &normalized_cwd,
        ) {
            return Some(suggestion);
        }
    detect_redundant_cd_in_argv(argv, cwd, &normalized_cwd)
}

fn detect_redundant_cd_in_shell(
    argv: &[String],
    script_index: usize,
    script: &str,
    cwd: &Path,
    normalized_cwd: &Path,
) -> Option<RedundantCdSuggestion> {
    let trimmed = script.trim_start();
    let tokens = shlex_split(trimmed)?;
    if tokens.len() < 3 {
        return None;
    }
    if tokens.first().map(String::as_str) != Some("cd") {
        return None;
    }
    let target = tokens.get(1)?.clone();
    if !is_simple_cd_target(&target) {
        return None;
    }
    let resolved_target = resolve_cd_target(&target, cwd)?;
    if resolved_target != normalized_cwd {
        return None;
    }

    let mut idx = 2;
    let mut saw_connector = false;
    while idx < tokens.len() && is_connector(&tokens[idx]) {
        saw_connector = true;
        idx += 1;
    }
    if !saw_connector || idx >= tokens.len() {
        return None;
    }

    let remainder_tokens = tokens[idx..].to_vec();
    let suggested_script = shlex_try_join(remainder_tokens.iter().map(std::string::String::as_str))
        .unwrap_or_else(|_| remainder_tokens.join(" "));
    if suggested_script.trim().is_empty() {
        return None;
    }

    let mut suggested = argv.to_vec();
    suggested[script_index] = suggested_script;

    Some(RedundantCdSuggestion {
        label: "original_script",
        original_value: script.to_string(),
        suggested,
        target_arg: target,
        cwd: normalized_cwd.to_path_buf(),
    })
}

fn detect_redundant_cd_in_argv(
    argv: &[String],
    cwd: &Path,
    normalized_cwd: &Path,
) -> Option<RedundantCdSuggestion> {
    if argv.len() < 4 {
        return None;
    }
    if argv.first().map(String::as_str) != Some("cd") {
        return None;
    }
    let target = argv.get(1)?.clone();
    if !is_simple_cd_target(&target) {
        return None;
    }
    let resolved_target = resolve_cd_target(&target, cwd)?;
    if resolved_target != normalized_cwd {
        return None;
    }

    let mut idx = 2;
    let mut saw_connector = false;
    while idx < argv.len() && is_connector(&argv[idx]) {
        saw_connector = true;
        idx += 1;
    }
    if !saw_connector || idx >= argv.len() {
        return None;
    }

    let suggested = argv[idx..].to_vec();
    if suggested.is_empty() {
        return None;
    }

    Some(RedundantCdSuggestion {
        label: "original_argv",
        original_value: format!("{argv:?}"),
        suggested,
        target_arg: target,
        cwd: normalized_cwd.to_path_buf(),
    })
}

fn resolve_cd_target(target: &str, cwd: &Path) -> Option<PathBuf> {
    if target.is_empty() {
        return None;
    }
    let candidate = if Path::new(target).is_absolute() {
        PathBuf::from(target)
    } else {
        cwd.join(target)
    };
    Some(normalize_path(candidate.as_path()))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            Component::Prefix(prefix) => {
                normalized = PathBuf::from(prefix.as_os_str());
            }
            Component::RootDir => {
                normalized.push(component.as_os_str());
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

fn is_simple_cd_target(target: &str) -> bool {
    if target.is_empty() || target == "-" {
        return false;
    }
    !target.chars().any(|ch| matches!(ch, '$' | '`' | '*' | '?' | '[' | ']' | '{' | '}' | '(' | ')' | '|' | '>' | '<' | '!'))
}

fn is_connector(token: &str) -> bool {
    matches!(token, "&&" | ";" | "||")
}

fn guidance_for_redundant_cd(suggestion: &RedundantCdSuggestion) -> String {
    let suggested = serde_json::to_string(&suggestion.suggested)
        .unwrap_or_else(|_| "<failed to serialize suggested argv>".to_string());
    let target_display = shlex_try_join(std::iter::once(suggestion.target_arg.as_str()))
        .unwrap_or_else(|_| suggestion.target_arg.clone());
    format!(
        "Leading cd {target_display} is redundant because the command already runs in {}. Drop the prefix before retrying.\n\n{}: {}\nresend_exact_argv: {}",
        suggestion.cwd.display(),
        suggestion.label,
        suggestion.original_value,
        suggested
    )
}

#[cfg(test)]
mod command_guard_detection_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_shell_redundant_cd() {
        let cwd = PathBuf::from("/tmp/project");
        let argv = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd /tmp/project && ls".to_string(),
        ];

        let suggestion = detect_redundant_cd(&argv, &cwd).expect("should flag redundant cd");
        assert_eq!(suggestion.label, "original_script");
        assert_eq!(suggestion.suggested, vec!["bash".to_string(), "-lc".to_string(), "ls".to_string()]);
    }

    #[test]
    fn ignores_cd_to_different_directory() {
        let cwd = PathBuf::from("/tmp/project");
        let argv = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd /tmp/project/src && ls".to_string(),
        ];

        assert!(detect_redundant_cd(&argv, &cwd).is_none());
    }

    #[test]
    fn skips_dynamic_cd_targets() {
        let cwd = PathBuf::from("/tmp/project");
        let argv = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd $PWD && ls".to_string(),
        ];

        assert!(detect_redundant_cd(&argv, &cwd).is_none());
    }

    #[test]
    fn detects_cat_heredoc_write() {
        let argv = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cat <<'EOF' > code-rs/git-tooling/Cargo.toml\n[package]\nname = \"demo\"\nEOF".to_string(),
        ];

        let suggestion = detect_cat_write(&argv).expect("should flag cat write");
        assert_eq!(suggestion.label, "original_script");
        assert!(suggestion
            .original_value
            .contains("cat <<'EOF' > code-rs/git-tooling/Cargo.toml"));
    }

    #[test]
    fn allows_cat_heredoc_without_redirect() {
        let argv = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cat <<'EOF'\nhello\nEOF".to_string(),
        ];

        assert!(detect_cat_write(&argv).is_none());
    }

    #[test]
    fn allows_cat_redirect_to_fd() {
        let argv = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cat <<'EOF' >&2\nwarn\nEOF".to_string(),
        ];

        assert!(detect_cat_write(&argv).is_none());
    }

    #[test]
    fn detects_python_here_doc_write() {
        let argv = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "python3 - <<'PY'\nfrom pathlib import Path\nPath('docs.txt').write_text('hello')\nPY".to_string(),
        ];

        let suggestion = detect_python_write(&argv).expect("should flag python write");
        assert_eq!(suggestion.label, "original_script");
        assert!(suggestion.original_value.contains("write_text"));
    }

    #[test]
    fn detects_python_inline_write() {
        let argv = vec![
            "python3".to_string(),
            "-c".to_string(),
            "from pathlib import Path; Path('foo.txt').write_text('hi')".to_string(),
        ];

        let suggestion = detect_python_write(&argv).expect("should flag inline python write");
        assert_eq!(suggestion.label, "python_inline_script");
        assert!(suggestion.original_value.contains("write_text"));
    }

    #[test]
    fn allows_read_only_python() {
        let argv = vec![
            "python3".to_string(),
            "-c".to_string(),
            "print('hello world')".to_string(),
        ];

        assert!(detect_python_write(&argv).is_none());
    }
}


#[cfg(test)]
mod tests {
    use super::format_exec_output_with_limit;
    use super::super::truncation::TRUNCATION_MARKER;
    use crate::exec::{ExecToolCallOutput, StreamOutput};
    use serde_json::Value;
    use std::time::Duration;
    use tempfile::TempDir;

    fn make_exec_output(output: String) -> ExecToolCallOutput {
        ExecToolCallOutput {
            exit_code: 0,
            stdout: StreamOutput::new(String::new()),
            stderr: StreamOutput::new(String::new()),
            aggregated_output: StreamOutput::new(output),
            duration: Duration::from_secs(1),
            timed_out: false,
        }
    }

    #[test]
    fn format_exec_output_truncates_with_small_limit() {
        let dir = TempDir::new().expect("tempdir");
        let output = "line\n".repeat(200);
        let exec_output = make_exec_output(output);

        let payload =
            format_exec_output_with_limit(dir.path(), "sub", "call", &exec_output, 64);
        let parsed: Value = serde_json::from_str(&payload).expect("parse payload");
        let content = parsed
            .get("output")
            .and_then(Value::as_str)
            .expect("output string");

        assert!(content.contains(TRUNCATION_MARKER));
    }

    #[test]
    fn format_exec_output_keeps_output_when_under_limit() {
        let dir = TempDir::new().expect("tempdir");
        let output = "line\n".repeat(10);
        let exec_output = make_exec_output(output.clone());
        let payload = format_exec_output_with_limit(
            dir.path(),
            "sub",
            "call",
            &exec_output,
            output.len() + 32,
        );
        let parsed: Value = serde_json::from_str(&payload).expect("parse payload");
        let content = parsed
            .get("output")
            .and_then(Value::as_str)
            .expect("output string");

        assert!(!content.contains(TRUNCATION_MARKER));
        assert!(content.contains("line"));
    }
}
