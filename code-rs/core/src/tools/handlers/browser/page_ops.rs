use super::helpers::get_browser_manager_for_session;
use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::events::execute_custom_tool;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde_json::Value;

pub(super) async fn handle_browser_javascript(
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
        "browser_javascript".to_string(),
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
                    let code = json.get("code").and_then(|v| v.as_str()).unwrap_or("");
                    match browser_manager.execute_javascript(code).await {
                        Ok(result) => {
                            tracing::info!("JavaScript execution returned: {:?}", result);

                            let formatted_result = if let Some(obj) = result.as_object() {
                                // Check if it's our wrapped result format.
                                if let (Some(success), Some(value)) =
                                    (obj.get("success"), obj.get("value"))
                                {
                                    let logs = obj.get("logs").and_then(|v| v.as_array());
                                    let mut output = String::new();

                                    if let Some(logs) = logs
                                        && !logs.is_empty()
                                    {
                                        output.push_str("Console logs:\n");
                                        for log in logs {
                                            if let Some(log_str) = log.as_str() {
                                                output.push_str(&format!("  {log_str}\n"));
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
                                    // Fallback to raw JSON if not in expected format.
                                    serde_json::to_string_pretty(&result)
                                        .unwrap_or_else(|_| "null".to_string())
                                }
                            } else {
                                // Not an object, return as-is.
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
                            let mut content = format!("Failed to execute JavaScript: {error_string}");
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
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to parse browser_javascript arguments: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_console(
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
        "browser_console".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            let Some(browser_manager) = browser_manager else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not enabled. Use browser_open to enable it first."
                                .to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
            let lines = match args {
                Ok(json) => json
                    .get("lines")
                    .and_then(serde_json::Value::as_u64)
                    .map(|n| n as usize),
                Err(_) => None,
            };

            match browser_manager.get_console_logs(lines).await {
                Ok(logs) => {
                    let formatted = if let Some(logs_array) = logs.as_array() {
                        if logs_array.is_empty() {
                            "No console logs captured.".to_string()
                        } else {
                            let mut output = String::new();
                            output.push_str("Console logs:\n");
                            for log in logs_array {
                                if let Some(log_obj) = log.as_object() {
                                    let timestamp = log_obj
                                        .get("timestamp")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let level = log_obj
                                        .get("level")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("log");
                                    let message = log_obj
                                        .get("message")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");

                                    output.push_str(&format!(
                                        "[{}] [{}] {}\n",
                                        timestamp,
                                        level.to_uppercase(),
                                        message
                                    ));
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
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to get console logs: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_cdp(
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
        "browser_cdp".to_string(),
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
                    let method = json
                        .get("method")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let params = json
                        .get("params")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
                    let target = json.get("target").and_then(|v| v.as_str()).unwrap_or("page");

                    if method.is_empty() {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(
                                    "Missing required field: method".to_string(),
                                ),
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
                                body: FunctionCallOutputBody::Text(format!(
                                    "Failed to execute CDP command: {e}"
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
                            "Failed to parse browser_cdp arguments: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

