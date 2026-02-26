use super::helpers::get_browser_manager_for_session;
use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::events::execute_custom_tool;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde_json::Value;

pub(super) async fn handle_browser_click(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
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
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

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

            // Optional absolute coordinates.
            let (mut target_x, mut target_y) = (None, None);
            if let Some(p) = params.as_ref() {
                if let Some(vx) = p.get("x").and_then(serde_json::Value::as_f64) {
                    target_x = Some(vx);
                }
                if let Some(vy) = p.get("y").and_then(serde_json::Value::as_f64) {
                    target_y = Some(vy);
                }
            }

            // If x or y provided, resolve missing coord from current position, then move.
            if target_x.is_some() || target_y.is_some() {
                match browser_manager.get_cursor_position().await {
                    Ok((cx, cy)) => {
                        let x = target_x.unwrap_or(cx);
                        let y = target_y.unwrap_or(cy);
                        if let Err(e) = browser_manager.move_mouse(x, y).await {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(format!(
                                        "Failed to move before click: {e}"
                                    )),
                                    success: Some(false),
                                },
                            };
                        }
                    }
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to get current cursor position: {e}"
                                )),
                                success: Some(false),
                            },
                        };
                    }
                }
            }

            // Perform the action at current (possibly moved) position.
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
                Ok((x, y, label)) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!("{label} at ({x}, {y})")),
                        success: Some(true),
                    },
                },
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to perform mouse action: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_move(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
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
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let _ = browser_manager
                .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                .await;

            let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
            match args {
                Ok(json) => {
                    // Check if we have relative movement (dx, dy) or absolute (x, y).
                    let has_dx = json.get("dx").is_some();
                    let has_dy = json.get("dy").is_some();
                    let has_x = json.get("x").is_some();
                    let has_y = json.get("y").is_some();

                    let result = if has_dx || has_dy {
                        // Relative movement.
                        let dx = json
                            .get("dx")
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(0.0);
                        let dy = json
                            .get("dy")
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(0.0);
                        browser_manager.move_mouse_relative(dx, dy).await
                    } else if has_x || has_y {
                        // Absolute movement.
                        let x = json
                            .get("x")
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(0.0);
                        let y = json
                            .get("y")
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(0.0);
                        browser_manager.move_mouse(x, y).await.map(|_| (x, y))
                    } else {
                        // No parameters provided, just return current position.
                        browser_manager.get_cursor_position().await
                    };

                    match result {
                        Ok((x, y)) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Moved mouse position to ({x}, {y})"
                                )),
                                success: Some(true),
                            },
                        },
                        Err(e) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to move mouse: {e}"
                                )),
                                success: Some(false),
                            },
                        },
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to parse browser_move arguments: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_type(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
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
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let _ = browser_manager
                .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                .await;

            let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
            match args {
                Ok(json) => {
                    let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    match browser_manager.type_text(text).await {
                        Ok(()) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!("Typed: {text}")),
                                success: Some(true),
                            },
                        },
                        Err(e) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to type text: {e}"
                                )),
                                success: Some(false),
                            },
                        },
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to parse browser_type arguments: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_key(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
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
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

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
                                body: FunctionCallOutputBody::Text(
                                    "Developer tools are disabled for this browser session. Use the browser.console tool to inspect logs instead."
                                        .to_string(),
                                ),
                                success: Some(false),
                            },
                        };
                    }

                    match browser_manager.press_key(key).await {
                        Ok(()) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!("Pressed key: {key}")),
                                success: Some(true),
                            },
                        },
                        Err(e) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to press key: {e}"
                                )),
                                success: Some(false),
                            },
                        },
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to parse browser_key arguments: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_scroll(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
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
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let _ = browser_manager
                .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                .await;

            let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
            match args {
                Ok(json) => {
                    let dx = json
                        .get("dx")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    let dy = json
                        .get("dy")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    match browser_manager.scroll_by(dx, dy).await {
                        Ok(()) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Scrolled by ({dx}, {dy})"
                                )),
                                success: Some(true),
                            },
                        },
                        Err(e) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to scroll: {e}"
                                )),
                                success: Some(false),
                            },
                        },
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to parse browser_scroll arguments: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_history(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
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
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
            match args {
                Ok(json) => {
                    let direction = json
                        .get("direction")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

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
                        Ok(()) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "History {direction} triggered"
                                )),
                                success: Some(true),
                            },
                        },
                        Err(e) => ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to navigate history: {e}"
                                )),
                                success: Some(false),
                            },
                        },
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to parse browser_history arguments: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

