use super::helpers::get_browser_manager_for_session;
use super::helpers::selector_rect_after_scroll;
use super::helpers::unwrap_execute_javascript_value;
use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::events::execute_custom_tool;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde_json::Value;

pub(super) async fn handle_browser_click_selector(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    let params = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_click_selector".to_string(),
        params.clone(),
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
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
            let Ok(json) = args else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Invalid click_selector arguments".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let selector = json.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let click_type = json
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("click")
                .to_lowercase();

            let (x, y, w, h) =
                match selector_rect_after_scroll(&browser_manager, selector, false).await {
                    Ok(rect) => rect,
                    Err(message) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(message),
                                success: Some(false),
                            },
                        };
                    }
                };

            if w <= 0.0 || h <= 0.0 {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Element has zero size; try browser.wait_for with visible=true first."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                };
            }

            let cx = x + (w / 2.0);
            let cy = y + (h / 2.0);
            if let Err(e) = browser_manager.move_mouse(cx, cy).await {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to move to selector: {e}"
                        )),
                        success: Some(false),
                    },
                };
            }

            let action_result = match click_type.as_str() {
                "mousedown" => match browser_manager.mouse_down_at_current().await {
                    Ok((mx, my)) => Ok((mx, my, "Mouse down".to_string())),
                    Err(e) => Err(e),
                },
                "mouseup" => match browser_manager.mouse_up_at_current().await {
                    Ok((mx, my)) => Ok((mx, my, "Mouse up".to_string())),
                    Err(e) => Err(e),
                },
                _ => match browser_manager.click_at_current().await {
                    Ok((mx, my)) => Ok((mx, my, "Clicked".to_string())),
                    Err(e) => Err(e),
                },
            };

            match action_result {
                Ok((mx, my, label)) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "{label} selector '{selector}' at ({mx}, {my})"
                        )),
                        success: Some(true),
                    },
                },
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to click selector: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_type_selector(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    let params = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_type_selector".to_string(),
        params.clone(),
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
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
            let Ok(json) = args else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Invalid type_selector arguments".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let selector = json.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if text.trim().is_empty() {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text("text must be non-empty".to_string()),
                        success: Some(false),
                    },
                };
            }

            let (x, y, w, h) =
                match selector_rect_after_scroll(&browser_manager, selector, true).await {
                    Ok(rect) => rect,
                    Err(message) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(message),
                                success: Some(false),
                            },
                        };
                    }
                };

            if w <= 0.0 || h <= 0.0 {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Element has zero size; try browser.wait_for with visible=true first."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                };
            }

            let cx = x + (w / 2.0);
            let cy = y + (h / 2.0);
            if let Err(e) = browser_manager.move_mouse(cx, cy).await {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to move to selector: {e}"
                        )),
                        success: Some(false),
                    },
                };
            }

            if let Err(e) = browser_manager.click_at_current().await {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to focus selector: {e}"
                        )),
                        success: Some(false),
                    },
                };
            }

            match browser_manager.type_text(text).await {
                Ok(()) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Typed into selector '{selector}': {text}"
                        )),
                        success: Some(true),
                    },
                },
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to type into selector: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_scroll_into_view(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    let params = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_scroll_into_view".to_string(),
        params.clone(),
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
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
            let Ok(json) = args else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Invalid scroll_into_view arguments".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let selector = json.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let (x, y, w, h) =
                match selector_rect_after_scroll(&browser_manager, selector, false).await {
                    Ok(rect) => rect,
                    Err(message) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(message),
                                success: Some(false),
                            },
                        };
                    }
                };

            let payload = serde_json::json!({
                "selector": selector,
                "rect": { "x": x, "y": y, "width": w, "height": h }
            });
            let pretty = serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| "{}".to_string());
            ResponseInputItem::FunctionCallOutput {
                call_id: call_id_clone.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(pretty),
                    success: Some(true),
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_wait_for(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    let params = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_wait_for".to_string(),
        params.clone(),
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
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
            let Ok(json) = args else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Invalid wait_for arguments".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let selector = json
                .get("selector")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(std::string::ToString::to_string);
            let visible = json
                .get("visible")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let ready_state = json
                .get("ready_state")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_ascii_lowercase);
            let poll_ms = json
                .get("poll_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(200)
                .clamp(50, 2000);
            let timeout_ms = json
                .get("timeout_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(5000)
                .clamp(100, 120_000);

            if selector.is_none() && ready_state.is_none() {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "wait_for requires selector and/or ready_state".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            }

            let selector_json = match selector.as_deref() {
                Some(sel) => serde_json::to_string(sel).unwrap_or_else(|_| "null".to_string()),
                None => "null".to_string(),
            };

            let ready_json = match ready_state.as_deref() {
                Some(target) => serde_json::to_string(target).unwrap_or_else(|_| "null".to_string()),
                None => "null".to_string(),
            };

            let start = tokio::time::Instant::now();
            let deadline = start + tokio::time::Duration::from_millis(timeout_ms);
            let mut last_state: Option<Value> = None;

            loop {
                let script = format!(
                    r#"(function() {{
                        try {{
                            var sel = {selector_json};
                            var readyTarget = {ready_json};
                            var out = {{
                                ok: true,
                                readyState: document.readyState,
                                selector: sel,
                                selectorFound: null,
                                visible: null,
                                rect: null
                            }};

                            if (readyTarget) {{
                                var rs = String(document.readyState || '');
                                if (readyTarget === 'interactive') {{
                                    out.ok = out.ok && (rs === 'interactive' || rs === 'complete');
                                }} else if (readyTarget === 'complete') {{
                                    out.ok = out.ok && (rs === 'complete');
                                }} else {{
                                    out.ok = false;
                                }}
                            }}

                            if (sel) {{
                                var el = document.querySelector(String(sel));
                                out.selectorFound = !!el;
                                if (!el) {{
                                    out.ok = false;
                                }} else {{
                                    var r = el.getBoundingClientRect();
                                    out.rect = {{ x: r.x, y: r.y, width: r.width, height: r.height }};
                                    var vis = (r.width > 0 && r.height > 0);
                                    out.visible = vis;
                                    if ({visible} && !vis) out.ok = false;
                                }}
                            }}

                            return out;
                        }} catch (e) {{
                            return {{ ok: false, error: String(e) }};
                        }}
                    }})()"#
                );

                let raw = match browser_manager.execute_javascript(&script).await {
                    Ok(v) => v,
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(format!(
                                    "wait_for JavaScript failed: {e}"
                                )),
                                success: Some(false),
                            },
                        };
                    }
                };

                match unwrap_execute_javascript_value(raw) {
                    Ok(value) => {
                        let ok = value
                            .get("ok")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        let _ = last_state.replace(value.clone());
                        if ok {
                            let payload = serde_json::json!({
                                "elapsed_ms": start.elapsed().as_millis(),
                                "state": value,
                            });
                            let pretty = serde_json::to_string_pretty(&payload)
                                .unwrap_or_else(|_| "{}".to_string());
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    body: FunctionCallOutputBody::Text(pretty),
                                    success: Some(true),
                                },
                            };
                        }
                    }
                    Err(err) => {
                        let _ =
                            last_state.replace(serde_json::json!({ "ok": false, "error": err }));
                    }
                }

                if tokio::time::Instant::now() >= deadline {
                    let payload = serde_json::json!({
                        "elapsed_ms": start.elapsed().as_millis(),
                        "timeout_ms": timeout_ms,
                        "last_state": last_state,
                    });
                    let pretty = serde_json::to_string_pretty(&payload)
                        .unwrap_or_else(|_| "{}".to_string());
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!(
                                "wait_for timed out.\n{pretty}"
                            )),
                            success: Some(false),
                        },
                    };
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(poll_ms)).await;
            }
        },
    )
    .await
}
