use crate::codex::Session;
use crate::protocol::EventMsg;
use crate::protocol::ExecCommandBeginEvent;
use crate::protocol::ExecCommandEndEvent;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::unsupported_tool_call_output;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::time::Instant;

pub(crate) struct JsReplToolHandler;
pub(crate) struct JsReplResetToolHandler;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JsReplFunctionArgs {
    code: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

fn join_outputs(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else {
        format!("{stdout}\n{stderr}")
    }
}

fn parse_freeform_args(input: &str) -> Result<crate::tools::js_repl::JsReplArgs, String> {
    if input.trim().is_empty() {
        return Err(
            "js_repl expects raw JavaScript tool input (non-empty). Provide JS source text, optionally with first-line `// codex-js-repl: ...`."
                .to_string(),
        );
    }

    let mut args = crate::tools::js_repl::JsReplArgs {
        code: input.to_string(),
        timeout_ms: None,
    };

    let mut lines = input.splitn(2, '\n');
    let first_line = lines.next().unwrap_or_default();
    let rest = lines.next().unwrap_or_default();
    let trimmed = first_line.trim_start();
    let Some(pragma) = trimmed.strip_prefix(crate::tools::js_repl::JS_REPL_PRAGMA_PREFIX) else {
        reject_json_or_quoted_source(&args.code)?;
        return Ok(args);
    };

    let mut timeout_ms: Option<u64> = None;
    let directive = pragma.trim();
    if !directive.is_empty() {
        for token in directive.split_whitespace() {
            let (key, value) = token.split_once('=').ok_or_else(|| {
                format!(
                    "js_repl pragma expects space-separated key=value pairs (supported keys: timeout_ms); got `{token}`"
                )
            })?;
            match key {
                "timeout_ms" => {
                    if timeout_ms.is_some() {
                        return Err("js_repl pragma specifies timeout_ms more than once".to_string());
                    }
                    let parsed = value.parse::<u64>().map_err(|_| {
                        format!("js_repl pragma timeout_ms must be an integer; got `{value}`")
                    })?;
                    timeout_ms = Some(parsed);
                }
                _ => {
                    return Err(format!("js_repl pragma only supports timeout_ms; got `{key}`"));
                }
            }
        }
    }

    if rest.trim().is_empty() {
        return Err("js_repl pragma must be followed by JavaScript source on subsequent lines".to_string());
    }

    reject_json_or_quoted_source(rest)?;
    args.code = rest.to_string();
    args.timeout_ms = timeout_ms;
    Ok(args)
}

fn reject_json_or_quoted_source(code: &str) -> Result<(), String> {
    let trimmed = code.trim();
    if trimmed.starts_with("```") {
        return Err(
            "js_repl expects raw JavaScript source, not markdown code fences. Resend plain JS only (optional first line `// codex-js-repl: ...`)."
                .to_string(),
        );
    }
    let Ok(value) = serde_json::from_str::<JsonValue>(trimmed) else {
        return Ok(());
    };
    match value {
        JsonValue::Object(_) | JsonValue::String(_) => Err(
            "js_repl is a freeform tool and expects raw JavaScript source. Resend plain JS only (optional first line `// codex-js-repl: ...`); do not send JSON (`{\"code\":...}`), quoted code, or markdown fences."
                .to_string(),
        ),
        _ => Ok(()),
    }
}

async fn emit_js_repl_exec_begin(sess: &Session, ctx: &crate::codex::ToolCallCtx) {
    let command = vec![crate::openai_tools::JS_REPL_TOOL_NAME.to_string()];
    sess.send_ordered_from_ctx(
        ctx,
        EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: ctx.call_id.clone(),
            command: command.clone(),
            cwd: sess.get_cwd().to_path_buf(),
            parsed_cmd: crate::parse_command::parse_command(&command),
        }),
    )
    .await;
}

async fn emit_js_repl_exec_end(
    sess: &Session,
    ctx: &crate::codex::ToolCallCtx,
    stdout: String,
    stderr: String,
    exit_code: i32,
    duration: std::time::Duration,
) {
    sess.send_ordered_from_ctx(
        ctx,
        EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: ctx.call_id.clone(),
            stdout,
            stderr,
            exit_code,
            duration,
        }),
    )
    .await;
}

#[async_trait]
impl ToolHandler for JsReplToolHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let outputs_custom = inv.payload.outputs_custom();

        if !sess.js_repl_enabled() {
            return unsupported_tool_call_output(
                &inv.ctx.call_id,
                outputs_custom,
                "js_repl is disabled (set `[tools].js_repl=true`)".to_string(),
            );
        }

        let args = match inv.payload {
            ToolPayload::Custom { input } => match parse_freeform_args(&input) {
                Ok(args) => args,
                Err(err) => {
                    return unsupported_tool_call_output(&inv.ctx.call_id, true, err);
                }
            },
            ToolPayload::Function { arguments } => match serde_json::from_str::<JsReplFunctionArgs>(&arguments) {
                Ok(args) => crate::tools::js_repl::JsReplArgs {
                    code: args.code,
                    timeout_ms: args.timeout_ms,
                },
                Err(err) => {
                    return unsupported_tool_call_output(
                        &inv.ctx.call_id,
                        outputs_custom,
                        format!("invalid js_repl arguments: {err}"),
                    );
                }
            },
            other => {
                return unsupported_tool_call_output(
                    &inv.ctx.call_id,
                    outputs_custom,
                    format!("js_repl received unsupported payload: {other:?}"),
                );
            }
        };

        let started_at = Instant::now();
        emit_js_repl_exec_begin(sess, &inv.ctx).await;

        let manager = match sess.js_repl_manager().await {
            Ok(manager) => manager,
            Err(err) => {
                emit_js_repl_exec_end(
                    sess,
                    &inv.ctx,
                    String::new(),
                    err.clone(),
                    1,
                    started_at.elapsed(),
                )
                .await;
                return unsupported_tool_call_output(&inv.ctx.call_id, outputs_custom, err);
            }
        };

        match manager.execute(sess.get_cwd(), args).await {
            Ok(result) => {
                emit_js_repl_exec_end(
                    sess,
                    &inv.ctx,
                    result.output.clone(),
                    String::new(),
                    0,
                    started_at.elapsed(),
                )
                .await;

                if outputs_custom {
                    ResponseInputItem::CustomToolCallOutput {
                        call_id: inv.ctx.call_id,
                        output: result.output,
                    }
                } else {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: inv.ctx.call_id,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(result.output),
                            success: Some(true),
                        },
                    }
                }
            }
            Err(err) => {
                let combined = join_outputs(&err.output, &err.error);
                emit_js_repl_exec_end(
                    sess,
                    &inv.ctx,
                    err.output.clone(),
                    err.error.clone(),
                    1,
                    started_at.elapsed(),
                )
                .await;

                if outputs_custom {
                    ResponseInputItem::CustomToolCallOutput {
                        call_id: inv.ctx.call_id,
                        output: combined,
                    }
                } else {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: inv.ctx.call_id,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(combined),
                            success: Some(false),
                        },
                    }
                }
            }
        }
    }
}

#[async_trait]
impl ToolHandler for JsReplResetToolHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let outputs_custom = inv.payload.outputs_custom();

        if !sess.js_repl_enabled() {
            return unsupported_tool_call_output(
                &inv.ctx.call_id,
                outputs_custom,
                "js_repl is disabled (set `[tools].js_repl=true`)".to_string(),
            );
        }

        if let Some(manager) = sess.js_repl_manager_if_started()
            && let Err(err) = manager.reset().await
        {
            return unsupported_tool_call_output(&inv.ctx.call_id, outputs_custom, err);
        }

        ResponseInputItem::FunctionCallOutput {
            call_id: inv.ctx.call_id,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text("js_repl kernel reset".to_string()),
                success: Some(true),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_freeform_args;
    use pretty_assertions::assert_eq;

    #[test]
    fn parse_freeform_args_without_pragma() {
        let args = parse_freeform_args("console.log('ok');").expect("parse args");
        assert_eq!(args.code, "console.log('ok');");
        assert_eq!(args.timeout_ms, None);
    }

    #[test]
    fn parse_freeform_args_with_pragma() {
        let input = "// codex-js-repl: timeout_ms=15000\nconsole.log('ok');";
        let args = parse_freeform_args(input).expect("parse args");
        assert_eq!(args.code, "console.log('ok');");
        assert_eq!(args.timeout_ms, Some(15_000));
    }

    #[test]
    fn parse_freeform_args_rejects_unknown_key() {
        let err =
            parse_freeform_args("// codex-js-repl: nope=1\nconsole.log('ok');").expect_err("err");
        assert_eq!(
            err,
            "js_repl pragma only supports timeout_ms; got `nope`"
        );
    }
}
