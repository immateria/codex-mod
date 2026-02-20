use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::execute_custom_tool;
use crate::tools::registry::ToolHandler;
use crate::turn_diff_tracker::TurnDiffTracker;
use async_trait::async_trait;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde_json::Value;
use std::sync::Arc;

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

async fn handle_browser_cleanup(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
    let call_id_clone = ctx.call_id.clone();
    let _sess_clone = sess;
    execute_custom_tool(
        sess,
        ctx,
        "browser_cleanup".to_string(),
        Some(serde_json::json!({})),
        || async move {
            if let Some(browser_manager) = get_browser_manager_for_session(_sess_clone).await {
                match browser_manager.cleanup().await {
                    Ok(_) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text("Browser cleanup completed".to_string()), success: Some(true) },
                    },
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(format!("Cleanup failed: {e}")), success: Some(false) },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text("Browser is not initialized. Use browser_open to start the browser.".to_string()), success: Some(false) },
                }
            }
        }
    ).await
}

pub(crate) async fn handle_browser_tool(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    use serde_json::Value;

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
                    body: FunctionCallOutputBody::Text("Invalid browser arguments: expected an object".to_string()),
                    success: Some(false),
                },
            };
        }
    };

    let action_value = object.remove("action");
    let action = match action_value.and_then(|v| v.as_str().map(std::string::ToString::to_string)) {
        Some(value) => value,
        None => {
            return ResponseInputItem::FunctionCallOutput {
                call_id: ctx.call_id.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text("Invalid browser arguments: missing 'action'".to_string()),
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
        "open" => handle_browser_open(sess, ctx, payload_string.clone()).await,
        "close" => handle_browser_close(sess, ctx).await,
        "status" => handle_browser_status(sess, ctx).await,
        "click" => handle_browser_click(sess, ctx, payload_string.clone()).await,
        "move" => handle_browser_move(sess, ctx, payload_string.clone()).await,
        "type" => handle_browser_type(sess, ctx, payload_string.clone()).await,
        "key" => handle_browser_key(sess, ctx, payload_string.clone()).await,
        "javascript" => handle_browser_javascript(sess, ctx, payload_string.clone()).await,
        "scroll" => handle_browser_scroll(sess, ctx, payload_string.clone()).await,
        "history" => handle_browser_history(sess, ctx, payload_string.clone()).await,
        "inspect" => handle_browser_inspect(sess, ctx, payload_string.clone()).await,
        "console" => handle_browser_console(sess, ctx, payload_string.clone()).await,
        "cdp" => handle_browser_cdp(sess, ctx, payload_string.clone()).await,
        "cleanup" => handle_browser_cleanup(sess, ctx).await,
        "fetch" => super::web_fetch::handle_web_fetch(sess, ctx, payload_string.clone()).await,
        _ => ResponseInputItem::FunctionCallOutput {
            call_id: ctx.call_id.clone(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(format!("Unknown browser action: {action}")),
                success: Some(false),
            },
        },
    }
}

async fn handle_browser_open(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    // Parse arguments as JSON for the event
    let params = serde_json::from_str(&arguments).ok();

    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_open".to_string(),
        params,
        || async move {
            // Parse the URL from arguments
            let args: Result<Value, _> = serde_json::from_str(&arguments_clone);

            match args {
                Ok(json) => {
                    let url = json
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("about:blank");

                    if url.trim().to_ascii_lowercase().starts_with("devtools://") {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text("Developer tools are disabled for this browser session. Use the browser.console tool to inspect logs instead.".to_string()),
                                success: Some(false),
                            },
                        };
                    }

                    // Use the global browser manager (create if needed)
                    let browser_manager = {
                        let existing_global = code_browser::global::get_browser_manager().await;
                        if let Some(existing) = existing_global {
                            tracing::info!("Using existing global browser manager");
                            Some(existing)
                        } else {
                            tracing::info!("Creating new browser manager");
                            let new_manager =
                                code_browser::global::get_or_create_browser_manager().await;
                            Some(new_manager)
                        }
                    };

                    if let Some(browser_manager) = browser_manager {
                        // Ensure the browser manager is marked enabled so status reflects reality
                        browser_manager.set_enabled_sync(true);
                        // Clear any lingering node highlight from previous commands
                        let _ = browser_manager
                            .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                            .await;
                        // Navigate to the URL with detailed timing logs
                        let step_start = std::time::Instant::now();
                        tracing::info!("[browser_open] begin goto: {}", url);
                        match browser_manager.goto(url).await {
                            Ok(_) => {
                                tracing::info!(
                                    "[browser_open] goto success: {} in {:?}",
                                    url,
                                    step_start.elapsed()
                                );
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(format!("Browser opened to: {url}")),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => {
                                let error_string = e.to_string();
                                let error_lower = error_string.to_ascii_lowercase();
                                let url_lower = url.to_ascii_lowercase();
                                let is_local = url_lower.starts_with("http://localhost")
                                    || url_lower.starts_with("https://localhost")
                                    || url_lower.starts_with("http://127.")
                                    || url_lower.starts_with("https://127.")
                                    || url_lower.starts_with("http://[::1]")
                                    || url_lower.starts_with("https://[::1]")
                                    || url_lower.starts_with("http://0.0.0.0")
                                    || url_lower.starts_with("https://0.0.0.0");
                                let mut content =
                                    format!("Failed to navigate browser to {url}: {error_string}");
                                if error_lower.contains("oneshot error")
                                    || error_lower.contains("oneshot canceled")
                                    || error_lower.contains("oneshot cancelled")
                                {
                                    content.push_str(
                                        " The CDP navigation was cancelled before it completed.",
                                    );
                                    if is_local {
                                        content.push_str(
                                            " If this is a local server, make sure it is reachable from the browser process (binding to 0.0.0.0 or using the machine IP can help).",
                                        );
                                    } else {
                                        content.push_str(
                                            " Reopening the browser page and retrying can resolve transient target resets.",
                                        );
                                    }
                                }
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(content),
                                        success: Some(false),
                                    },
                                }
                            }
                        }
                    } else {
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text("Failed to initialize browser manager.".to_string()),
                                success: Some(false),
                            },
                        }
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!("Failed to parse browser_open arguments: {e}")),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

/// Get the browser manager for the session (always uses global)
async fn get_browser_manager_for_session(
    _sess: &Session,
) -> Option<Arc<code_browser::BrowserManager>> {
    // Always use the global browser manager
    code_browser::global::get_browser_manager().await
}

async fn handle_browser_close(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
    let sess_clone = sess;
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_close".to_string(),
        None,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                // Clear any lingering highlight before closing
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                match browser_manager.stop().await {
                    Ok(_) => {
                        // Clear the browser manager from global
                        code_browser::global::clear_browser_manager().await;
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text("Browser closed. Screenshot capture disabled.".to_string()),
                                success: Some(true),
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to close browser: {e}")),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text("Browser is not currently open.".to_string()),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_status(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
    let sess_clone = sess;
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_status".to_string(),
        None,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let status = browser_manager.get_status().await;
                let status_msg = if status.enabled {
                    if let Some(url) = status.current_url {
                        format!("Browser status: Enabled, currently at {url}")
                    } else {
                        "Browser status: Enabled, no page loaded".to_string()
                    }
                } else {
                    "Browser status: Disabled".to_string()
                };

                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(status_msg),
                        success: Some(true),
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_click(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    let sess_clone = sess;
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_click".to_string(),
        params.clone(),
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;

            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                // Determine click type: default 'click', or 'mousedown'/'mouseup'
                let click_type = params
                    .as_ref()
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("click")
                    .to_lowercase();

                // Optional absolute coordinates
                let (mut target_x, mut target_y) = (None, None);
                if let Some(p) = params.as_ref() {
                    if let Some(vx) = p.get("x").and_then(serde_json::Value::as_f64) {
                        target_x = Some(vx);
                    }
                    if let Some(vy) = p.get("y").and_then(serde_json::Value::as_f64) {
                        target_y = Some(vy);
                    }
                }

                // If x or y provided, resolve missing coord from current position, then move
                if target_x.is_some() || target_y.is_some() {
                    // get current cursor for missing values
                    match browser_manager.get_cursor_position().await {
                        Ok((cx, cy)) => {
                            let x = target_x.unwrap_or(cx);
                            let y = target_y.unwrap_or(cy);
                            if let Err(e) = browser_manager.move_mouse(x, y).await {
                                return ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(format!("Failed to move before click: {e}")),
                                        success: Some(false),
                                    },
                                };
                            }
                        }
                        Err(e) => {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(format!("Failed to get current cursor position: {e}")),
                                    success: Some(false),
                                },
                            };
                        }
                    }
                }

                // Perform the action at current (possibly moved) position
                let action_result = match click_type.as_str() {
                    "mousedown" => match browser_manager.mouse_down_at_current().await {
                        Ok((x, y)) => Ok((x, y, "Mouse down".to_string())),
                        Err(e) => Err(e),
                    },
                    "mouseup" => match browser_manager.mouse_up_at_current().await {
                        Ok((x, y)) => Ok((x, y, "Mouse up".to_string())),
                        Err(e) => Err(e),
                    },
                    _ => match browser_manager.click_at_current().await {
                        Ok((x, y)) => Ok((x, y, "Clicked".to_string())),
                        Err(e) => Err(e),
                    },
                };

                match action_result {
                    Ok((x, y, label)) => {
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!("{label} at ({x}, {y})")),
                                success: Some(true),
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to perform mouse action: {e}")),
                            success: Some(false),
                        },
                    },
                }
    } else {
        ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(
                    "Browser is not initialized. Use browser_open to start the browser."
                        .to_string(),
                ),
                success: Some(false),
            },
        }
    }
        },
    )
    .await
}

async fn handle_browser_move(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_move".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;

            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        // Check if we have relative movement (dx, dy) or absolute (x, y)
                        let has_dx = json.get("dx").is_some();
                        let has_dy = json.get("dy").is_some();
                        let has_x = json.get("x").is_some();
                        let has_y = json.get("y").is_some();

                        let result = if has_dx || has_dy {
                            // Relative movement
                            let dx = json.get("dx").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                            let dy = json.get("dy").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                            browser_manager.move_mouse_relative(dx, dy).await
                        } else if has_x || has_y {
                            // Absolute movement
                            let x = json.get("x").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                            let y = json.get("y").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                            browser_manager.move_mouse(x, y).await.map(|_| (x, y))
                        } else {
                            // No parameters provided, just return current position
                            browser_manager.get_cursor_position().await
                        };

                        match result {
                            Ok((x, y)) => {
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(format!("Moved mouse position to ({x}, {y})")),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(format!("Failed to move mouse: {e}")),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to parse browser_move arguments: {e}")),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_type(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_type".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("");

                        match browser_manager.type_text(text).await {
                            Ok(_) => {
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(format!("Typed: {text}")),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(format!("Failed to type text: {e}")),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to parse browser_type arguments: {e}")),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_key(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_key".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let key = json.get("key").and_then(|v| v.as_str()).unwrap_or("");

                        let normalized = key
                            .split_whitespace()
                            .collect::<String>()
                            .to_ascii_lowercase();
                        if matches!(normalized.as_str(), "f12" | "ctrl+shift+i" | "control+shift+i") {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text("Developer tools are disabled for this browser session. Use the browser.console tool to inspect logs instead.".to_string()),
                                    success: Some(false),
                                },
                            };
                        }

                        match browser_manager.press_key(key).await {
                            Ok(_) => {
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(format!("Pressed key: {key}")),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(format!("Failed to press key: {e}")),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to parse browser_key arguments: {e}")),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_javascript(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_javascript".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let code = json.get("code").and_then(|v| v.as_str()).unwrap_or("");

                        match browser_manager.execute_javascript(code).await {
                            Ok(result) => {
                                // Log the JavaScript execution result
                                tracing::info!("JavaScript execution returned: {:?}", result);

                                // Format the result for the LLM
                                let formatted_result = if let Some(obj) = result.as_object() {
                                    // Check if it's our wrapped result format
                                    if let (Some(success), Some(value)) =
                                        (obj.get("success"), obj.get("value"))
                                    {
                                        let logs = obj.get("logs").and_then(|v| v.as_array());
                                        let mut output = String::new();

                                        if let Some(logs) = logs
                                            && !logs.is_empty() {
                                                output.push_str("Console logs:\n");
                                                for log in logs {
                                                    if let Some(log_str) = log.as_str() {
                                                        output
                                                            .push_str(&format!("  {log_str}\n"));
                                                    }
                                                }
                                                output.push('\n');
                                            }

                                        if success.as_bool().unwrap_or(false) {
                                            output.push_str("Result: ");
                                            output.push_str(
                                                &serde_json::to_string_pretty(value)
                                                    .unwrap_or_else(|_| "null".to_string()),
                                            );
                                        } else if let Some(error) = obj.get("error") {
                                            output.push_str("Error: ");
                                            output.push_str(&error.to_string());
                                        }

                                        output
                                    } else {
                                        // Fallback to raw JSON if not in expected format
                                        serde_json::to_string_pretty(&result)
                                            .unwrap_or_else(|_| "null".to_string())
                                    }
                                } else {
                                    // Not an object, return as-is
                                    serde_json::to_string_pretty(&result)
                                        .unwrap_or_else(|_| "null".to_string())
                                };

                                tracing::info!("Returning to LLM: {}", formatted_result);

                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(formatted_result),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => {
                                let error_string = e.to_string();
                                let mut content =
                                    format!("Failed to execute JavaScript: {error_string}");
                                if error_string.to_ascii_lowercase().contains("oneshot") {
                                    content.push_str(" (CDP request was cancelled or the page session was reset; reconnecting the browser and retrying usually helps.)");
                                }
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(content),
                                        success: Some(false),
                                    },
                                }
                            }
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to parse browser_javascript arguments: {e}")),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_scroll(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_scroll".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let dx = json.get("dx").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                        let dy = json.get("dy").and_then(serde_json::Value::as_f64).unwrap_or(0.0);

                        match browser_manager.scroll_by(dx, dy).await {
                    Ok(_) => {
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!("Scrolled by ({dx}, {dy})")),
                                success: Some(true),
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to scroll: {e}")),
                            success: Some(false),
                        },
                    },
                }
            }
            Err(e) => ResponseInputItem::FunctionCallOutput {
                call_id: call_id_clone,
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(format!("Failed to parse browser_scroll arguments: {e}")),
                    success: Some(false),
                },
            },
        }
    } else {
        ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text("Browser is not initialized. Use browser_open to start the browser.".to_string()),
                success: Some(false),
            },
        }
    }
        },
    )
    .await
}

async fn handle_browser_console(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_console".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                let lines = match args {
                    Ok(json) => json.get("lines").and_then(serde_json::Value::as_u64).map(|n| n as usize),
                    Err(_) => None,
                };

                match browser_manager.get_console_logs(lines).await {
                    Ok(logs) => {
                        // Format the logs for display
                        let formatted = if let Some(logs_array) = logs.as_array() {
                            if logs_array.is_empty() {
                                "No console logs captured.".to_string()
                            } else {
                                let mut output = String::new();
                                output.push_str("Console logs:\n");
                                for log in logs_array {
                                    if let Some(log_obj) = log.as_object() {
                                        let timestamp = log_obj.get("timestamp")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let level = log_obj.get("level")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("log");
                                        let message = log_obj.get("message")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");

                                        output.push_str(&format!("[{}] [{}] {}\n", timestamp, level.to_uppercase(), message));
                                    }
                                }
                                output
                            }
                        } else {
                            "No console logs captured.".to_string()
                        };

                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(formatted),
                                success: Some(true),
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to get console logs: {e}")),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text("Browser is not enabled. Use browser_open to enable it first.".to_string()),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_cdp(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_cdp".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let method = json
                            .get("method")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let params = json.get("params").cloned().unwrap_or_else(|| Value::Object(serde_json::Map::new()));
                        let target = json
                            .get("target")
                            .and_then(|v| v.as_str())
                            .unwrap_or("page");

                        if method.is_empty() {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text("Missing required field: method".to_string()),
                                    success: Some(false),
                                },
                            };
                        }

                        let exec_res = if target == "browser" {
                            browser_manager.execute_cdp_browser(&method, params).await
                        } else {
                            browser_manager.execute_cdp(&method, params).await
                        };

                        match exec_res {
                            Ok(result) => {
                                let pretty = serde_json::to_string_pretty(&result)
                                    .unwrap_or_else(|_| "<non-serializable result>".to_string());
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone,
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(pretty),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(format!("Failed to execute CDP command: {e}")),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to parse browser_cdp arguments: {e}")),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text("Browser is not initialized. Use browser_open to start the browser.".to_string()),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_inspect(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    use serde_json::json;
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_inspect".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        // Determine target element: by id, by coords, or by cursor
                        let id_attr = json.get("id").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
                        let mut x = json.get("x").and_then(serde_json::Value::as_f64);
                        let mut y = json.get("y").and_then(serde_json::Value::as_f64);

                        if (x.is_none() || y.is_none()) && id_attr.is_none() {
                            // No coords provided; use current cursor
                            if let Ok((cx, cy)) = browser_manager.get_cursor_position().await {
                                x = Some(cx);
                                y = Some(cy);
                            }
                        }

                        // Resolve nodeId
                        let node_id_value = if let Some(id_attr) = id_attr.clone() {
                            // Use DOM.getDocument -> DOM.querySelector with selector `#id`
                            let doc = browser_manager
                                .execute_cdp("DOM.getDocument", json!({}))
                                .await;
                            let root_id = match doc {
                                Ok(v) => v.get("root").and_then(|r| r.get("nodeId")).and_then(serde_json::Value::as_u64),
                                Err(_) => None,
                            };
                            if let Some(root_node_id) = root_id {
                                let sel = format!("#{id_attr}");
                                let q = browser_manager
                                    .execute_cdp(
                                        "DOM.querySelector",
                                        json!({"nodeId": root_node_id, "selector": sel}),
                                    )
                                    .await;
                                match q {
                                    Ok(v) => v.get("nodeId").cloned(),
                                    Err(_) => None,
                                }
                            } else {
                                None
                            }
                        } else if let (Some(x), Some(y)) = (x, y) {
                            // Use DOM.getNodeForLocation
                            let res = browser_manager
                                .execute_cdp(
                                    "DOM.getNodeForLocation",
                                    json!({
                                        "x": x,
                                        "y": y,
                                        "includeUserAgentShadowDOM": true
                                    }),
                                )
                                .await;
                            match res {
                                Ok(v) => {
                                    // Prefer nodeId; if absent, push backendNodeId
                                    if let Some(n) = v.get("nodeId").cloned() {
                                        Some(n)
                                    } else if let Some(backend) = v.get("backendNodeId").and_then(serde_json::Value::as_u64) {
                                        let pushed = browser_manager
                                            .execute_cdp(
                                                "DOM.pushNodesByBackendIdsToFrontend",
                                                json!({ "backendNodeIds": [backend] }),
                                            )
                                            .await
                                            .ok();
                                        pushed
                                            .and_then(|pv| pv.get("nodeIds").and_then(|arr| arr.as_array().cloned()))
                                            .and_then(|arr| arr.first().cloned())
                                    } else {
                                        None
                                    }
                                }
                                Err(_) => None,
                            }
                        } else {
                            None
                        };

                        let node_id = match node_id_value.and_then(|v| v.as_u64()) {
                            Some(id) => id,
                            None => {
                                return ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone,
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text("Failed to resolve target node for inspection".to_string()),
                                        success: Some(false),
                                    },
                                };
                            }
                        };

                        // Enable CSS domain to get matched rules
                        let _ = browser_manager.execute_cdp("CSS.enable", json!({})).await;

                        // Gather details
                        let attrs = browser_manager
                            .execute_cdp("DOM.getAttributes", json!({"nodeId": node_id}))
                            .await
                            .unwrap_or_else(|_| json!({}));
                        let outer = browser_manager
                            .execute_cdp("DOM.getOuterHTML", json!({"nodeId": node_id}))
                            .await
                            .unwrap_or_else(|_| json!({}));
                        let box_model = browser_manager
                            .execute_cdp("DOM.getBoxModel", json!({"nodeId": node_id}))
                            .await
                            .unwrap_or_else(|_| json!({}));
                        let styles = browser_manager
                            .execute_cdp("CSS.getMatchedStylesForNode", json!({"nodeId": node_id}))
                            .await
                            .unwrap_or_else(|_| json!({}));

                        // Highlight the inspected node using Overlay domain (no screenshot capture here)
                        let _ = browser_manager.execute_cdp("Overlay.enable", json!({})).await;
                        let highlight_config = json!({
                            "showInfo": true,
                            "showStyles": false,
                            "showRulers": false,
                            "contentColor": {"r": 111, "g": 168, "b": 220, "a": 0.20},
                            "paddingColor": {"r": 147, "g": 196, "b": 125, "a": 0.55},
                            "borderColor": {"r": 255, "g": 229, "b": 153, "a": 0.60},
                            "marginColor": {"r": 246, "g": 178, "b": 107, "a": 0.60}
                        });
                        let _ = browser_manager.execute_cdp(
                            "Overlay.highlightNode",
                            json!({ "nodeId": node_id, "highlightConfig": highlight_config })
                        ).await;
                        // Do not hide here; keep highlight until the next browser command.

                        // Format output
                        let mut out = String::new();
                        if let (Some(ix), Some(iy)) = (x, y) {
                            out.push_str(&format!("Target: coordinates ({ix}, {iy})\n"));
                        }
                        if let Some(id_attr) = id_attr {
                            out.push_str(&format!("Target: id '#{id_attr}'\n"));
                        }
                        out.push_str(&format!("NodeId: {node_id}\n"));

                        // Attributes
                        if let Some(arr) = attrs.get("attributes").and_then(|v| v.as_array()) {
                            out.push_str("Attributes:\n");
                            let mut it = arr.iter();
                            while let (Some(k), Some(v)) = (it.next(), it.next()) {
                                out.push_str(&format!("  {}=\"{}\"\n", k.as_str().unwrap_or(""), v.as_str().unwrap_or("")));
                            }
                        }

                        // Outer HTML
                        if let Some(html) = outer.get("outerHTML").and_then(|v| v.as_str()) {
                            let one = html.replace('\n', " ");
                            let snippet: String = one.chars().take(800).collect();
                            out.push_str("\nOuterHTML (truncated):\n");
                            out.push_str(&snippet);
                            if one.len() > snippet.len() { out.push('…'); }
                            out.push('\n');
                        }

                        // Box Model summary
                        if box_model.get("model").is_some() {
                            out.push_str("\nBoxModel: available (content/padding/border/margin)\n");
                        }

                        // Matched styles summary
                        if let Some(rules) = styles.get("matchedCSSRules").and_then(|v| v.as_array()) {
                            out.push_str(&format!("Matched CSS rules: {}\n", rules.len()));
                        }

                        // No inline screenshot capture; result reflects DOM details only.

                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload { body: FunctionCallOutputBody::Text(out), success: Some(true) },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to parse browser_inspect arguments: {e}")),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text("Browser is not initialized. Use browser_open to start the browser.".to_string()),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_history(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_history".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let direction =
                            json.get("direction").and_then(|v| v.as_str()).unwrap_or("");

                        if direction != "back" && direction != "forward" {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(format!(
                                        "Unsupported direction: {direction} (expected 'back' or 'forward')"
                                    )),
                                    success: Some(false),
                                },
                            };
                        }

                        let action_res = if direction == "back" {
                            browser_manager.history_back().await
                        } else {
                            browser_manager.history_forward().await
                        };

                        match action_res {
                            Ok(_) => {
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        body: FunctionCallOutputBody::Text(format!("History {direction} triggered")),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(format!("Failed to navigate history: {e}")),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Failed to parse browser_history arguments: {e}")),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}
