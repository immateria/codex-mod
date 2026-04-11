use crate::bridge_client::get_effective_subscription;
use crate::bridge_client::persist_workspace_subscription;
use crate::bridge_client::send_bridge_control;
use crate::bridge_client::set_session_subscription;
use crate::bridge_client::set_workspace_subscription;
use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use crate::tools::handlers::tool_error;
use crate::tools::handlers::tool_output;
use code_protocol::models::ResponseInputItem;
use serde::Deserialize;
use std::path::Path;

pub(crate) struct BridgeToolHandler;

#[async_trait]
impl ToolHandler for BridgeToolHandler {
    async fn handle(
        &self,
        sess: &Session,
        _turn_diff_tracker: &mut TurnDiffTracker,
        inv: ToolInvocation,
    ) -> ResponseInputItem {
        let ToolPayload::Function { arguments } = inv.payload else {
            return tool_error(inv.ctx.call_id, "code_bridge expects function-call arguments");
        };

        handle_code_bridge(sess, &inv.ctx, arguments).await
    }
}

#[derive(Deserialize)]
struct BridgeControlArgs {
    action: String,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

fn normalise_level(level: &str) -> Option<String> {
    let l = level.trim().to_lowercase();
    match l.as_str() {
        "errors" | "error" => Some("errors".to_owned()),
        "warn" | "warning" => Some("warn".to_owned()),
        "info" => Some("info".to_owned()),
        "trace" | "debug" => Some("trace".to_owned()),
        _ => None,
    }
}

fn full_capabilities() -> Vec<String> {
    vec![
        "console".to_owned(),
        "error".to_owned(),
        "pageview".to_owned(),
        "screenshot".to_owned(),
        "control".to_owned(),
    ]
}

async fn handle_code_bridge(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    handle_code_bridge_with_cwd(sess.get_cwd(), ctx, arguments).await
}

async fn handle_code_bridge_with_cwd(
    cwd: &Path,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    let parsed: Result<BridgeControlArgs, _> = serde_json::from_str(&arguments);
    let args = match parsed {
        Ok(a) => a,
        Err(e) => {
            return tool_error(ctx.call_id.clone(), format!("invalid arguments: {e}"));
        }
    };

    let action = args.action.to_lowercase();

    match action.as_str() {
        "subscribe" => {
            let Some(level) = args.level.as_ref().and_then(|l| normalise_level(l)) else {
                return tool_error(ctx.call_id.clone(), "invalid or missing level (use errors|warn|info|trace)");
            };

            let mut sub = get_effective_subscription();
            sub.levels = vec![level];
            sub.capabilities = full_capabilities();
            sub.llm_filter = "off".to_owned();

            set_session_subscription(Some(sub.clone()));
            if let Err(e) = persist_workspace_subscription(cwd, Some(sub.clone())) {
                return tool_error(ctx.call_id.clone(), format!("persist failed: {e}"));
            }
            set_workspace_subscription(Some(sub));

            tool_output(ctx.call_id.clone(), "ok")
        }
        "screenshot" => {
            send_bridge_control("screenshot", serde_json::json!({}));
            tool_output(ctx.call_id.clone(), "requested screenshot")
        }
        "javascript" => {
            let Some(code) = args.code.as_ref().map(|c| c.trim()).filter(|c| !c.is_empty()) else {
                return tool_error(ctx.call_id.clone(), "missing code for javascript action");
            };
            send_bridge_control("javascript", serde_json::json!({ "code": code }));
            tool_output(ctx.call_id.clone(), "sent javascript")
        }
        // Keep legacy actions for backward compatibility with older prompts/tools
        "show" | "set" | "clear" => tool_error(ctx.call_id.clone(), "deprecated action; use subscribe, screenshot, or javascript"),
        _ => tool_error(ctx.call_id.clone(), format!("unsupported action: {action}")),
    }
}

#[cfg(test)]
mod bridge_tool_tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    fn call_tool_with_cwd(cwd: &Path, args: &str) -> ResponseInputItem {
        let ctx = ToolCallCtx::new("sub".into(), "call".into(), None, None);
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { handle_code_bridge_with_cwd(cwd, &ctx, args.to_string()).await })
    }

    #[test]
    fn bridge_tool_show_set_clear() {
        let tmp = TempDir::new().unwrap();
        let cwd = tmp.path();

        // set (session-only) is now deprecated; ensure we emit a helpful failure
        let out = call_tool_with_cwd(
            cwd,
            r#"{"action":"set","levels":["trace"],"capabilities":["console"],"llm_filter":"off"}"#,
        );
        match out {
            ResponseInputItem::FunctionCallOutput { output, .. } => {
                assert_eq!(output.success, Some(false));
                assert!(output
                    .body
                    .to_text()
                    .unwrap_or_default()
                    .contains("deprecated action"));
            }
            _ => panic!("unexpected output"),
        }

        // show is also deprecated; we should return the same guidance
        let out = call_tool_with_cwd(cwd, r#"{"action":"show"}"#);
        match out {
            ResponseInputItem::FunctionCallOutput { output, .. } => {
                assert_eq!(output.success, Some(false));
                assert!(output
                    .body
                    .to_text()
                    .unwrap_or_default()
                    .contains("deprecated action"));
            }
            _ => panic!("unexpected output"),
        }

        // clear
        let out = call_tool_with_cwd(cwd, r#"{"action":"clear","persist":true}"#);
        match out {
            ResponseInputItem::FunctionCallOutput { output, .. } => {
                assert_eq!(output.success, Some(false));
                assert!(output
                    .body
                    .to_text()
                    .unwrap_or_default()
                    .contains("deprecated action"));
            }
            _ => panic!("unexpected output"),
        }
    }
}
