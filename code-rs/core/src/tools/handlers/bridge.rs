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
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
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
            return ResponseInputItem::FunctionCallOutput {
                call_id: inv.ctx.call_id,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "code_bridge expects function-call arguments".to_string(),
                    ),
                    success: Some(false),
                },
            };
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
        "errors" | "error" => Some("errors".to_string()),
        "warn" | "warning" => Some("warn".to_string()),
        "info" => Some("info".to_string()),
        "trace" | "debug" => Some("trace".to_string()),
        _ => None,
    }
}

fn full_capabilities() -> Vec<String> {
    vec![
        "console".to_string(),
        "error".to_string(),
        "pageview".to_string(),
        "screenshot".to_string(),
        "control".to_string(),
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
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!("invalid arguments: {e}")),
                    success: Some(false),
                },
            };
        }
    };

    let action = args.action.to_lowercase();

    match action.as_str() {
        "subscribe" => {
            let level = match args.level.as_ref().and_then(|l| normalise_level(l)) {
                Some(lvl) => lvl,
                None => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: ctx.call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "invalid or missing level (use errors|warn|info|trace)".to_string(),
                            ),
                            success: Some(false),
                        },
                    }
                }
            };

            let mut sub = get_effective_subscription();
            sub.levels = vec![level];
            sub.capabilities = full_capabilities();
            sub.llm_filter = "off".to_string();

            set_session_subscription(Some(sub.clone()));
            if let Err(e) = persist_workspace_subscription(cwd, Some(sub.clone())) {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: ctx.call_id.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!("persist failed: {e}")),
                        success: Some(false),
                    },
                };
            }
            set_workspace_subscription(Some(sub));

            ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text("ok".to_string()),
                    success: Some(true),
                },
            }
        }
        "screenshot" => {
            send_bridge_control("screenshot", serde_json::json!({}));
            ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text("requested screenshot".to_string()),
                    success: Some(true),
                },
            }
        }
        "javascript" => {
            let code = match args.code.as_ref().map(|c| c.trim()).filter(|c| !c.is_empty()) {
                Some(c) => c,
                None => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: ctx.call_id.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "missing code for javascript action".to_string(),
                            ),
                            success: Some(false),
                        },
                    }
                }
            };
            send_bridge_control("javascript", serde_json::json!({ "code": code }));
            ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text("sent javascript".to_string()),
                    success: Some(true),
                },
            }
        }
        // Keep legacy actions for backward compatibility with older prompts/tools
        "show" | "set" | "clear" => ResponseInputItem::FunctionCallOutput {
            call_id: ctx.call_id.clone(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(
                    "deprecated action; use subscribe, screenshot, or javascript".to_string(),
                ),
                success: Some(false),
            },
        },
        _ => ResponseInputItem::FunctionCallOutput {
            call_id: ctx.call_id.clone(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(format!("unsupported action: {action}")),
                success: Some(false),
            },
        },
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
