#[cfg(feature = "browser-automation")]
mod helpers;
#[cfg(feature = "browser-automation")]
mod input;
#[cfg(feature = "browser-automation")]
mod inspect;
#[cfg(feature = "browser-automation")]
mod lifecycle;
#[cfg(feature = "browser-automation")]
mod page_ops;
#[cfg(feature = "browser-automation")]
mod screenshot;
#[cfg(feature = "browser-automation")]
mod selectors;
#[cfg(feature = "browser-automation")]
mod storage;

use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::tool_error;
#[cfg(not(feature = "browser-automation"))]
use crate::tools::handlers::tool_output;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
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
            return tool_error(inv.ctx.call_id, "browser expects function-call arguments");
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
            return tool_error(ctx.call_id.clone(), format!("Invalid browser arguments: {e}"));
        }
    };

    let Value::Object(mut object) = parsed_value else {
        return tool_error(ctx.call_id.clone(), "Invalid browser arguments: expected an object");
    };

    let action_value = object.remove("action");
    let Some(action) = action_value.and_then(|v| v.as_str().map(ToString::to_string)) else {
        return tool_error(ctx.call_id.clone(), "Invalid browser arguments: missing 'action'");
    };

    let payload_value = Value::Object(object.clone());
    let payload_string = if object.is_empty() {
        "{}".to_owned()
    } else {
        serde_json::to_string(&payload_value).unwrap_or_else(|_| "{}".to_owned())
    };

    let action_lower = action.to_lowercase();
    match action_lower.as_str() {
        "status" => {
            #[cfg(feature = "browser-automation")]
            {
                lifecycle::handle_browser_status(sess, ctx).await
            }
            #[cfg(not(feature = "browser-automation"))]
            {
                tool_output(ctx.call_id.clone(), "Browser automation is not available in this build.")
            }
        }
        #[cfg(feature = "browser-automation")]
        "open" => lifecycle::handle_browser_open(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "close" => lifecycle::handle_browser_close(sess, ctx).await,
        #[cfg(feature = "browser-automation")]
        "targets" => lifecycle::handle_browser_targets(sess, ctx).await,
        #[cfg(feature = "browser-automation")]
        "new_tab" => lifecycle::handle_browser_new_tab(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "switch_target" => lifecycle::handle_browser_switch_target(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "activate_target" => lifecycle::handle_browser_activate_target(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "close_target" => lifecycle::handle_browser_close_target(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "click" => input::handle_browser_click(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "click_selector" => selectors::handle_browser_click_selector(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "move" => input::handle_browser_move(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "type" => input::handle_browser_type(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "type_selector" => selectors::handle_browser_type_selector(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "key" => input::handle_browser_key(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "javascript" => page_ops::handle_browser_javascript(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "scroll" => input::handle_browser_scroll(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "scroll_into_view" => selectors::handle_browser_scroll_into_view(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "wait_for" => selectors::handle_browser_wait_for(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "history" => input::handle_browser_history(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "inspect" => inspect::handle_browser_inspect(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "console" => page_ops::handle_browser_console(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "inspect_selector" => inspect::handle_browser_inspect_selector(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "screenshot" => screenshot::handle_browser_screenshot(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "cookies_get" => storage::handle_browser_cookies_get(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "cookies_set" => storage::handle_browser_cookies_set(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "storage_get" => storage::handle_browser_storage_get(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "storage_set" => storage::handle_browser_storage_set(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "cdp" => page_ops::handle_browser_cdp(sess, ctx, payload_string).await,
        #[cfg(feature = "browser-automation")]
        "cleanup" => lifecycle::handle_browser_cleanup(sess, ctx).await,
        "fetch" => super::web_fetch::handle_web_fetch(sess, ctx, payload_string).await,
        _ => tool_error(ctx.call_id.clone(), {
            #[cfg(feature = "browser-automation")]
            {
                format!("Unknown browser action: {action}")
            }
            #[cfg(not(feature = "browser-automation"))]
            {
                format!(
                    "Browser automation is not available in this build (action: {action})."
                )
            }
        }),
    }
}
