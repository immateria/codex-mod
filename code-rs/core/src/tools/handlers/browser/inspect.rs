use super::helpers::get_browser_manager_for_session;
use super::helpers::node_id_for_selector;
use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::events::execute_custom_tool;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde_json::Value;

pub(super) async fn handle_browser_inspect_selector(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    use serde_json::json;

    let params = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_inspect_selector".to_string(),
        params,
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

            let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
            let Ok(json_args) = args else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Invalid inspect_selector arguments".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let selector = json_args
                .get("selector")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if selector.is_empty() {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "selector must be non-empty".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            }

            let node_id = match node_id_for_selector(&browser_manager, selector).await {
                Ok(node_id) => node_id,
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

            // Best-effort scroll before highlight for clarity.
            let _ = browser_manager
                .execute_cdp("DOM.scrollIntoViewIfNeeded", json!({ "nodeId": node_id }))
                .await;

            let _ = browser_manager.execute_cdp("CSS.enable", json!({})).await;

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

            // Highlight inspected node, keep until next browser command.
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
            let _ = browser_manager
                .execute_cdp(
                    "Overlay.highlightNode",
                    json!({ "nodeId": node_id, "highlightConfig": highlight_config }),
                )
                .await;

            let mut out = String::new();
            out.push_str(&format!("Target: selector '{selector}'\n"));
            out.push_str(&format!("NodeId: {node_id}\n"));

            if let Some(arr) = attrs.get("attributes").and_then(|v| v.as_array()) {
                out.push_str("Attributes:\n");
                let mut it = arr.iter();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    out.push_str(&format!(
                        "  {}=\"{}\"\n",
                        k.as_str().unwrap_or(""),
                        v.as_str().unwrap_or("")
                    ));
                }
            }

            if let Some(html) = outer.get("outerHTML").and_then(|v| v.as_str()) {
                let one = html.replace('\n', " ");
                let snippet: String = one.chars().take(800).collect();
                out.push_str("\nOuterHTML (truncated):\n");
                out.push_str(&snippet);
                if one.len() > snippet.len() {
                    out.push('…');
                }
                out.push('\n');
            }

            if box_model.get("model").is_some() {
                out.push_str("\nBoxModel: available (content/padding/border/margin)\n");
            }

            if let Some(rules) = styles.get("matchedCSSRules").and_then(|v| v.as_array()) {
                out.push_str(&format!("Matched CSS rules: {}\n", rules.len()));
            }

            ResponseInputItem::FunctionCallOutput {
                call_id: call_id_clone.clone(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::Text(out),
                    success: Some(true),
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_inspect(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
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
                    // Determine target element: by id, by coords, or by cursor.
                    let id_attr = json
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(std::string::ToString::to_string);
                    let mut x = json.get("x").and_then(serde_json::Value::as_f64);
                    let mut y = json.get("y").and_then(serde_json::Value::as_f64);

                    if (x.is_none() || y.is_none()) && id_attr.is_none() {
                        // No coords provided; use current cursor.
                        if let Ok((cx, cy)) = browser_manager.get_cursor_position().await {
                            x = Some(cx);
                            y = Some(cy);
                        }
                    }

                    // Resolve nodeId.
                    let node_id_value = if let Some(id_attr) = id_attr.clone() {
                        // Use DOM.getDocument -> DOM.querySelector with selector `#id`.
                        let doc = browser_manager.execute_cdp("DOM.getDocument", json!({})).await;
                        let root_id = match doc {
                            Ok(v) => v
                                .get("root")
                                .and_then(|r| r.get("nodeId"))
                                .and_then(serde_json::Value::as_u64),
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
                        // Use DOM.getNodeForLocation.
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
                                // Prefer nodeId; if absent, push backendNodeId.
                                if let Some(n) = v.get("nodeId").cloned() {
                                    Some(n)
                                } else if let Some(backend) =
                                    v.get("backendNodeId").and_then(serde_json::Value::as_u64)
                                {
                                    let pushed = browser_manager
                                        .execute_cdp(
                                            "DOM.pushNodesByBackendIdsToFrontend",
                                            json!({ "backendNodeIds": [backend] }),
                                        )
                                        .await
                                        .ok();
                                    pushed
                                        .and_then(|pv| {
                                            pv.get("nodeIds")
                                                .and_then(|arr| arr.as_array().cloned())
                                        })
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
                                    body: FunctionCallOutputBody::Text(
                                        "Failed to resolve target node for inspection".to_string(),
                                    ),
                                    success: Some(false),
                                },
                            };
                        }
                    };

                    // Enable CSS domain to get matched rules.
                    let _ = browser_manager.execute_cdp("CSS.enable", json!({})).await;

                    // Gather details.
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

                    // Highlight the inspected node using Overlay domain (no screenshot capture here).
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
                    let _ = browser_manager
                        .execute_cdp(
                            "Overlay.highlightNode",
                            json!({ "nodeId": node_id, "highlightConfig": highlight_config }),
                        )
                        .await;
                    // Do not hide here; keep highlight until the next browser command.

                    // Format output.
                    let mut out = String::new();
                    if let (Some(ix), Some(iy)) = (x, y) {
                        out.push_str(&format!("Target: coordinates ({ix}, {iy})\n"));
                    }
                    if let Some(id_attr) = id_attr {
                        out.push_str(&format!("Target: id '#{id_attr}'\n"));
                    }
                    out.push_str(&format!("NodeId: {node_id}\n"));

                    // Attributes.
                    if let Some(arr) = attrs.get("attributes").and_then(|v| v.as_array()) {
                        out.push_str("Attributes:\n");
                        let mut it = arr.iter();
                        while let (Some(k), Some(v)) = (it.next(), it.next()) {
                            out.push_str(&format!(
                                "  {}=\"{}\"\n",
                                k.as_str().unwrap_or(""),
                                v.as_str().unwrap_or("")
                            ));
                        }
                    }

                    // Outer HTML.
                    if let Some(html) = outer.get("outerHTML").and_then(|v| v.as_str()) {
                        let one = html.replace('\n', " ");
                        let snippet: String = one.chars().take(800).collect();
                        out.push_str("\nOuterHTML (truncated):\n");
                        out.push_str(&snippet);
                        if one.len() > snippet.len() {
                            out.push('…');
                        }
                        out.push('\n');
                    }

                    // Box Model summary.
                    if box_model.get("model").is_some() {
                        out.push_str(
                            "\nBoxModel: available (content/padding/border/margin)\n",
                        );
                    }

                    // Matched styles summary.
                    if let Some(rules) = styles.get("matchedCSSRules").and_then(|v| v.as_array()) {
                        out.push_str(&format!("Matched CSS rules: {}\n", rules.len()));
                    }

                    // No inline screenshot capture; result reflects DOM details only.

                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(out),
                            success: Some(true),
                        },
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to parse browser_inspect arguments: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

