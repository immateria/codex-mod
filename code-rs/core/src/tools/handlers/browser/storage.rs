use super::helpers::get_browser_manager_for_session;
use super::helpers::unwrap_execute_javascript_value;
use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::events::execute_custom_tool;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde_json::Value;

pub(super) async fn handle_browser_cookies_get(
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
        "browser_cookies_get".to_string(),
        params,
        || async move {
            use serde_json::json;

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

            let _ = browser_manager.execute_cdp("Network.enable", json!({})).await;

            let urls: Option<Vec<String>> = serde_json::from_str::<Value>(&arguments_clone)
                .ok()
                .and_then(|v| v.get("urls").cloned())
                .and_then(|v| {
                    v.as_array().map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(std::string::ToString::to_string))
                            .collect::<Vec<_>>()
                    })
                })
                .filter(|v| !v.is_empty());

            let resp = if let Some(urls) = urls {
                browser_manager
                    .execute_cdp("Network.getCookies", json!({ "urls": urls }))
                    .await
            } else {
                browser_manager
                    .execute_cdp("Network.getAllCookies", json!({}))
                    .await
            };

            match resp {
                Ok(value) => {
                    let pretty =
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string());
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(pretty),
                            success: Some(true),
                        },
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "cookies_get failed: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_cookies_set(
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
        "browser_cookies_set".to_string(),
        params,
        || async move {
            use serde_json::json;

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

            let _ = browser_manager.execute_cdp("Network.enable", json!({})).await;

            let cookies_value = serde_json::from_str::<Value>(&arguments_clone)
                .ok()
                .and_then(|v| v.get("cookies").cloned());
            let Some(Value::Array(cookies)) = cookies_value else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "cookies_set requires a 'cookies' array".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let resp = browser_manager
                .execute_cdp("Network.setCookies", json!({ "cookies": cookies }))
                .await;
            match resp {
                Ok(value) => {
                    let payload = serde_json::json!({
                        "ok": true,
                        "result": value,
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
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "cookies_set failed: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_storage_get(
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
        "browser_storage_get".to_string(),
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
            let Ok(json) = args else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Invalid storage_get arguments".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let storage_kind = json
                .get("storage")
                .and_then(|v| v.as_str())
                .unwrap_or("local")
                .to_ascii_lowercase();
            let storage_expr = if storage_kind == "session" {
                "window.sessionStorage"
            } else {
                "window.localStorage"
            };

            let keys_json = json.get("keys").cloned().unwrap_or(serde_json::Value::Null);
            let keys_literal =
                serde_json::to_string(&keys_json).unwrap_or_else(|_| "null".to_string());

            let script = format!(
                r#"(function() {{
                    try {{
                        var st = {storage_expr};
                        var keys = {keys_literal};
                        var out = Object.create(null);
                        if (Array.isArray(keys) && keys.length) {{
                            for (var i = 0; i < keys.length; i++) {{
                                var k = String(keys[i]);
                                out[k] = st.getItem(k);
                            }}
                        }} else {{
                            for (var j = 0; j < st.length; j++) {{
                                var kk = st.key(j);
                                if (kk !== null) out[String(kk)] = st.getItem(String(kk));
                            }}
                        }}
                        return out;
                    }} catch (e) {{
                        return {{ __error: String(e) }};
                    }}
                }})()"#
            );

            match browser_manager.execute_javascript(&script).await {
                Ok(raw) => match unwrap_execute_javascript_value(raw) {
                    Ok(value) => {
                        let pretty = serde_json::to_string_pretty(&value)
                            .unwrap_or_else(|_| "{}".to_string());
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(pretty),
                                success: Some(true),
                            },
                        }
                    }
                    Err(err) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!(
                                "storage_get failed: {err}"
                            )),
                            success: Some(false),
                        },
                    },
                },
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "storage_get failed: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_storage_set(
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
        "browser_storage_set".to_string(),
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
            let Ok(json) = args else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Invalid storage_set arguments".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let storage_kind = json
                .get("storage")
                .and_then(|v| v.as_str())
                .unwrap_or("local")
                .to_ascii_lowercase();
            let storage_expr = if storage_kind == "session" {
                "window.sessionStorage"
            } else {
                "window.localStorage"
            };

            let clear = json
                .get("clear")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let items = json.get("items").cloned().unwrap_or(serde_json::Value::Null);
            let items_literal =
                serde_json::to_string(&items).unwrap_or_else(|_| "null".to_string());

            let script = format!(
                r#"(function() {{
                    try {{
                        var st = {storage_expr};
                        var clear = {clear};
                        var items = {items_literal};
                        if (!items || typeof items !== 'object') return {{ ok: false, error: "items must be an object" }};
                        var setCount = 0;
                        var removeCount = 0;
                        if (clear) {{
                            try {{ st.clear(); }} catch (_) {{}}
                        }}
                        for (var k in items) {{
                            if (!Object.prototype.hasOwnProperty.call(items, k)) continue;
                            var v = items[k];
                            if (v === null) {{
                                try {{ st.removeItem(String(k)); removeCount++; }} catch (_) {{}}
                                continue;
                            }}
                            if (typeof v !== 'string') {{
                                try {{ v = JSON.stringify(v); }} catch (_) {{ v = String(v); }}
                            }}
                            try {{ st.setItem(String(k), String(v)); setCount++; }} catch (_) {{}}
                        }}
                        return {{ ok: true, cleared: clear, set: setCount, removed: removeCount }};
                    }} catch (e) {{
                        return {{ ok: false, error: String(e) }};
                    }}
                }})()"#
            );

            match browser_manager.execute_javascript(&script).await {
                Ok(raw) => match unwrap_execute_javascript_value(raw) {
                    Ok(value) => {
                        let pretty = serde_json::to_string_pretty(&value)
                            .unwrap_or_else(|_| "{}".to_string());
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(pretty),
                                success: Some(true),
                            },
                        }
                    }
                    Err(err) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!(
                                "storage_set failed: {err}"
                            )),
                            success: Some(false),
                        },
                    },
                },
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "storage_set failed: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

