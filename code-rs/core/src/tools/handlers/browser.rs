mod helpers;
mod input;
mod inspect;
mod lifecycle;
mod page_ops;
mod screenshot;
mod selectors;
mod storage;

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
use serde_json::Value;

pub(crate) struct BrowserToolHandler;

#[async_trait]
impl ToolHandler for BrowserToolHandler {
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
                        "browser expects function-call arguments".to_string(),
                    ),
                    success: Some(false),
                },
            };
        };

        handle_browser_tool(sess, &inv.ctx, arguments).await
    }
}

pub(crate) async fn handle_browser_tool(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    let parsed_value = match serde_json::from_str::<Value>(&arguments) {
        Ok(value) => value,
        Err(e) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!("Invalid browser arguments: {e}")),
                    success: Some(false),
                },
            };
        }
    };

    let mut object = match parsed_value {
        Value::Object(map) => map,
        _ => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "Invalid browser arguments: expected an object".to_string(),
                    ),
                    success: Some(false),
                },
            };
        }
    };

    let action_value = object.remove("action");
    let action = match action_value.and_then(|v| v.as_str().map(std::string::ToString::to_string))
    {
        Some(value) => value,
        None => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(
                        "Invalid browser arguments: missing 'action'".to_string(),
                    ),
                    success: Some(false),
                },
            };
        }
    };

    let payload_value = Value::Object(object.clone());
    let payload_string = if object.is_empty() {
        "{}".to_string()
    } else {
        serde_json::to_string(&payload_value).unwrap_or_else(|_| "{}".to_string())
    };

    let action_lower = action.to_lowercase();
    match action_lower.as_str() {
        "open" => lifecycle::handle_browser_open(sess, ctx, payload_string).await,
        "close" => lifecycle::handle_browser_close(sess, ctx).await,
        "status" => lifecycle::handle_browser_status(sess, ctx).await,
        "targets" => lifecycle::handle_browser_targets(sess, ctx).await,
        "new_tab" => lifecycle::handle_browser_new_tab(sess, ctx, payload_string).await,
        "switch_target" => {
            lifecycle::handle_browser_switch_target(sess, ctx, payload_string).await
        }
        "activate_target" => {
            lifecycle::handle_browser_activate_target(sess, ctx, payload_string).await
        }
        "close_target" => lifecycle::handle_browser_close_target(sess, ctx, payload_string).await,
        "click" => input::handle_browser_click(sess, ctx, payload_string).await,
        "click_selector" => selectors::handle_browser_click_selector(sess, ctx, payload_string).await,
        "move" => input::handle_browser_move(sess, ctx, payload_string).await,
        "type" => input::handle_browser_type(sess, ctx, payload_string).await,
        "type_selector" => selectors::handle_browser_type_selector(sess, ctx, payload_string).await,
        "key" => input::handle_browser_key(sess, ctx, payload_string).await,
        "javascript" => page_ops::handle_browser_javascript(sess, ctx, payload_string).await,
        "scroll" => input::handle_browser_scroll(sess, ctx, payload_string).await,
        "scroll_into_view" => {
            selectors::handle_browser_scroll_into_view(sess, ctx, payload_string).await
        }
        "wait_for" => selectors::handle_browser_wait_for(sess, ctx, payload_string).await,
        "history" => input::handle_browser_history(sess, ctx, payload_string).await,
        "inspect" => inspect::handle_browser_inspect(sess, ctx, payload_string).await,
        "console" => page_ops::handle_browser_console(sess, ctx, payload_string).await,
        "inspect_selector" => {
            inspect::handle_browser_inspect_selector(sess, ctx, payload_string).await
        }
        "screenshot" => screenshot::handle_browser_screenshot(sess, ctx, payload_string).await,
        "cookies_get" => storage::handle_browser_cookies_get(sess, ctx, payload_string).await,
        "cookies_set" => storage::handle_browser_cookies_set(sess, ctx, payload_string).await,
        "storage_get" => storage::handle_browser_storage_get(sess, ctx, payload_string).await,
        "storage_set" => storage::handle_browser_storage_set(sess, ctx, payload_string).await,
        "cdp" => page_ops::handle_browser_cdp(sess, ctx, payload_string).await,
        "cleanup" => lifecycle::handle_browser_cleanup(sess, ctx).await,
        "fetch" => super::web_fetch::handle_web_fetch(sess, ctx, payload_string).await,
        _ => ResponseInputItem::FunctionCallOutput {
            call_id: ctx.call_id.clone(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(format!("Unknown browser action: {action}")),
                success: Some(false),
            },
        },
    }
}

