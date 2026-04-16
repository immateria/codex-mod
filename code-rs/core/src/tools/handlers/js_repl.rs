use crate::codex::Session;
use crate::protocol::EventMsg;
use crate::protocol::ExecCommandEndEvent;
use crate::protocol::JsReplExecBeginEvent;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::unsupported_tool_call_output;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::tools::handlers::{tool_error, tool_output};
use async_trait::async_trait;
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
    #[serde(default)]
    runtime: Option<crate::config::JsReplRuntimeKindToml>,
}

fn join_outputs(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_owned()
    } else if stderr.is_empty() {
        stdout.to_owned()
    } else {
        format!("{stdout}\n{stderr}")
    }
}

fn parse_freeform_args(input: &str) -> Result<crate::tools::js_repl::JsReplArgs, String> {
    if input.trim().is_empty() {
        return Err(
            "js_repl expects raw JavaScript tool input (non-empty). Provide JS source text, optionally with first-line `// codex-js-repl: ...`.".to_owned(),
        );
    }

    let mut args = crate::tools::js_repl::JsReplArgs {
        code: input.to_owned(),
        timeout_ms: None,
        runtime: None,
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
    let mut runtime: Option<crate::config::JsReplRuntimeKindToml> = None;
    let directive = pragma.trim();
    if !directive.is_empty() {
        for token in directive.split_whitespace() {
            let (key, value) = token.split_once('=').ok_or_else(|| {
                format!(
                    "js_repl pragma expects space-separated key=value pairs (supported keys: timeout_ms, runtime); got `{token}`"
                )
            })?;
            match key {
                "timeout_ms" => {
                    if timeout_ms.is_some() {
                        return Err("js_repl pragma specifies timeout_ms more than once".to_owned());
                    }
                    let parsed = value.parse::<u64>().map_err(|_| {
                        format!("js_repl pragma timeout_ms must be an integer; got `{value}`")
                    })?;
                    timeout_ms = Some(parsed);
                }
                "runtime" => {
                    if runtime.is_some() {
                        return Err("js_repl pragma specifies runtime more than once".to_owned());
                    }
                    let normalized = value.trim().to_ascii_lowercase();
                    runtime = crate::config::JsReplRuntimeKindToml::ALL
                        .iter()
                        .find(|k| k.label() == normalized)
                        .copied();
                    if runtime.is_none() {
                        let valid: Vec<_> = crate::config::JsReplRuntimeKindToml::ALL
                            .iter()
                            .map(|k| k.label())
                            .collect();
                        return Err(format!(
                            "js_repl pragma runtime must be one of {valid:?}; got `{value}`"
                        ));
                    }
                }
                _ => {
                    return Err(format!(
                        "js_repl pragma only supports timeout_ms and runtime; got `{key}`"
                    ));
                }
            }
        }
    }

    if rest.trim().is_empty() {
        return Err("js_repl pragma must be followed by JavaScript source on subsequent lines".to_owned());
    }

    reject_json_or_quoted_source(rest)?;
    rest.clone_into(&mut args.code);
    args.timeout_ms = timeout_ms;
    args.runtime = runtime;
    Ok(args)
}

fn reject_json_or_quoted_source(code: &str) -> Result<(), String> {
    let trimmed = code.trim();
    if trimmed.starts_with("```") {
        return Err(
            "js_repl expects raw JavaScript source, not markdown code fences. Resend plain JS only (optional first line `// codex-js-repl: ...`).".to_owned(),
        );
    }
    let Ok(value) = serde_json::from_str::<JsonValue>(trimmed) else {
        return Ok(());
    };
    match value {
        JsonValue::Object(_) | JsonValue::String(_) => Err(
            "js_repl is a freeform tool and expects raw JavaScript source. Resend plain JS only (optional first line `// codex-js-repl: ...`); do not send JSON (`{\"code\":...}`), quoted code, or markdown fences.".to_owned(),
        ),
        _ => Ok(()),
    }
}

async fn emit_js_repl_exec_begin(
    sess: &Session,
    ctx: &crate::codex::ToolCallCtx,
    code: &str,
    manager: &crate::tools::js_repl::JsReplManager,
    timeout_ms: u64,
) {
    sess.send_ordered_from_ctx(
        ctx,
        EventMsg::JsReplExecBegin(JsReplExecBeginEvent {
            call_id: ctx.call_id.clone(),
            code: code.to_owned(),
            runtime_kind: manager.runtime_kind_str().to_owned(),
            runtime_version: manager.runtime_version().to_owned(),
            cwd: sess.get_cwd().to_path_buf(),
            timeout_ms,
        }),
    )
    .await;
}

async fn emit_js_repl_exec_end(
    sess: &Session,
    ctx: &crate::codex::ToolCallCtx,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
    duration: std::time::Duration,
) {
    sess.send_ordered_from_ctx(
        ctx,
        EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: ctx.call_id.clone(),
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
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
        turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolInvocation {
            ctx,
            payload,
            attempt_req,
            ..
        } = inv;
        let outputs_custom = payload.outputs_custom();

        if !sess.js_repl_enabled() {
            return unsupported_tool_call_output(
                &ctx.call_id,
                outputs_custom,
                "js_repl is disabled (set `[tools].js_repl=true`)".to_owned(),
            );
        }

        let args = match payload {
            ToolPayload::Custom { input } => match parse_freeform_args(&input) {
                Ok(args) => args,
                Err(err) => {
                    return unsupported_tool_call_output(&ctx.call_id, true, err);
                }
            },
            ToolPayload::Function { arguments } => match serde_json::from_str::<JsReplFunctionArgs>(&arguments) {
                Ok(args) => crate::tools::js_repl::JsReplArgs {
                    code: args.code,
                    timeout_ms: args.timeout_ms,
                    runtime: args.runtime,
                },
                Err(err) => {
                    return unsupported_tool_call_output(
                        &ctx.call_id,
                        outputs_custom,
                        format!("invalid js_repl arguments: {err}"),
                    );
                }
            },
            other => {
                return unsupported_tool_call_output(
                    &ctx.call_id,
                    outputs_custom,
                    format!("js_repl received unsupported payload: {other:?}"),
                );
            }
        };

        let runtime_kind = args.runtime.unwrap_or_else(|| sess.js_repl_default_runtime());
        let manager = match sess.js_repl_manager_for_runtime(runtime_kind).await {
            Ok(manager) => manager,
            Err(err) => {
                return unsupported_tool_call_output(&ctx.call_id, outputs_custom, err);
            }
        };

        let started_at = Instant::now();
        let timeout_ms = args
            .timeout_ms
            .unwrap_or(crate::tools::js_repl::DEFAULT_TIMEOUT_MS)
            .min(crate::tools::js_repl::MAX_TIMEOUT_MS);
        emit_js_repl_exec_begin(sess, &ctx, &args.code, &manager, timeout_ms).await;

        match manager
            .execute(
                sess,
                turn_diff_tracker,
                &ctx,
                attempt_req,
                sess.get_cwd(),
                args,
            )
            .await
        {
            Ok(result) => {
                emit_js_repl_exec_end(
                    sess,
                    &ctx,
                    &result.output,
                    "",
                    0,
                    started_at.elapsed(),
                )
                .await;

                if outputs_custom {
                    ResponseInputItem::CustomToolCallOutput {
                        call_id: ctx.call_id,
                        output: FunctionCallOutputPayload::from_text(result.output),
                    }
                } else {
                    tool_output(ctx.call_id, result.output)
                }
            }
            Err(err) => {
                let combined = join_outputs(&err.output, &err.error);
                emit_js_repl_exec_end(
                    sess,
                    &ctx,
                    &err.output,
                    &err.error,
                    1,
                    started_at.elapsed(),
                )
                .await;

                if outputs_custom {
                    ResponseInputItem::CustomToolCallOutput {
                        call_id: ctx.call_id,
                        output: FunctionCallOutputPayload::from_text(combined),
                    }
                } else {
                    tool_error(ctx.call_id, combined)
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
                "js_repl is disabled (set `[tools].js_repl=true`)".to_owned(),
            );
        }

        let mut first_err: Option<String> = None;
        for &runtime in crate::config::JsReplRuntimeKindToml::ALL {
            if let Some(manager) = sess.js_repl_manager_if_started_for_runtime(runtime)
                && let Err(err) = manager.reset().await
                && first_err.is_none()
            {
                first_err = Some(err);
            }
        }
        if let Some(err) = first_err {
            return unsupported_tool_call_output(&inv.ctx.call_id, outputs_custom, err);
        }

        tool_output(inv.ctx.call_id, "js_repl kernel reset")
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
        assert_eq!(args.runtime, None);
    }

    #[test]
    fn parse_freeform_args_with_runtime() {
        let input = "// codex-js-repl: runtime=deno timeout_ms=15000\nconsole.log('ok');";
        let args = parse_freeform_args(input).expect("parse args");
        assert_eq!(args.code, "console.log('ok');");
        assert_eq!(args.timeout_ms, Some(15_000));
        assert_eq!(args.runtime, Some(crate::config::JsReplRuntimeKindToml::Deno));
    }

    #[test]
    fn parse_freeform_args_rejects_unknown_key() {
        let err =
            parse_freeform_args("// codex-js-repl: nope=1\nconsole.log('ok');").expect_err("err");
        assert_eq!(
            err,
            "js_repl pragma only supports timeout_ms and runtime; got `nope`"
        );
    }

    #[test]
    fn parse_freeform_args_with_node_runtime() {
        let input = "// codex-js-repl: runtime=node\nconsole.log('ok');";
        let args = parse_freeform_args(input).expect("parse args");
        assert_eq!(args.runtime, Some(crate::config::JsReplRuntimeKindToml::Node));
    }

    #[test]
    fn parse_freeform_args_rejects_unknown_runtime() {
        let err = parse_freeform_args("// codex-js-repl: runtime=bun\nconsole.log('ok');")
            .expect_err("err");
        assert!(
            err.contains("bun"),
            "error should mention the invalid runtime: {err}"
        );
    }

    #[test]
    fn parse_freeform_args_rejects_json_wrapped_code() {
        let err = parse_freeform_args(r#"{"code":"await doThing()"}"#).expect_err("err");
        assert!(
            err.contains("freeform"),
            "error should explain freeform format: {err}"
        );
    }

    #[test]
    fn parse_freeform_args_rejects_reset_key() {
        let err = parse_freeform_args("// codex-js-repl: reset=true\nconsole.log('ok');")
            .expect_err("err");
        assert!(
            err.contains("reset"),
            "error should mention the rejected key: {err}"
        );
    }

    #[test]
    fn parse_freeform_args_rejects_duplicate_runtime() {
        let err = parse_freeform_args(
            "// codex-js-repl: runtime=node runtime=deno\nconsole.log('ok');",
        )
        .expect_err("err");
        assert!(
            err.contains("more than once"),
            "error should explain duplicate: {err}"
        );
    }

    #[test]
    fn parse_freeform_args_runtime_case_insensitive() {
        let input = "// codex-js-repl: runtime=NODE\nconsole.log('ok');";
        let args = parse_freeform_args(input).expect("parse args");
        assert_eq!(args.runtime, Some(crate::config::JsReplRuntimeKindToml::Node));
    }
}
