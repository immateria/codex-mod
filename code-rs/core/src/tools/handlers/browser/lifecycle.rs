use super::helpers::get_browser_manager_for_session;
use super::helpers::resolve_target_id_from_value;
use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::events::execute_custom_tool;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde_json::Value;

pub(super) async fn handle_browser_cleanup(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
    let call_id_clone = ctx.call_id.clone();
    let sess_clone = sess;
    execute_custom_tool(
        sess,
        ctx,
        "browser_cleanup".to_string(),
        Some(serde_json::json!({})),
        || async move {
            if let Some(browser_manager) = get_browser_manager_for_session(sess_clone).await {
                match browser_manager.cleanup().await {
                    Ok(()) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "Browser cleanup completed".to_string(),
                            ),
                            success: Some(true),
                        },
                    },
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!("Cleanup failed: {e}")),
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

pub(super) async fn handle_browser_open(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
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
                                body: FunctionCallOutputBody::Text(
                                    "Developer tools are disabled for this browser session. Use the browser.console tool to inspect logs instead."
                                        .to_string(),
                                ),
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
                                        body: FunctionCallOutputBody::Text(format!(
                                            "Browser opened to: {url}"
                                        )),
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
                                body: FunctionCallOutputBody::Text(
                                    "Failed to initialize browser manager.".to_string(),
                                ),
                                success: Some(false),
                            },
                        }
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to parse browser_open arguments: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_close(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
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
                    Ok(()) => {
                        // Clear the browser manager from global
                        code_browser::global::clear_browser_manager().await;
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                body: FunctionCallOutputBody::Text(
                                    "Browser closed. Screenshot capture disabled.".to_string(),
                                ),
                                success: Some(true),
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!(
                                "Failed to close browser: {e}"
                            )),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Browser is not currently open.".to_string(),
                        ),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_status(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
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

pub(super) async fn handle_browser_targets(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
    let sess_clone = sess;
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_targets".to_string(),
        None,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;

                match browser_manager.list_page_targets().await {
                    Ok(targets) => {
                        let active_target_id = targets
                            .iter()
                            .find(|t| t.active)
                            .map(|t| t.target_id.clone());
                        let payload = serde_json::json!({
                            "active_target_id": active_target_id,
                            "targets": targets,
                            "hint": "Use browser action=switch_target with a target_id or index."
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
                                "Failed to list browser targets: {e}"
                            )),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
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

pub(super) async fn handle_browser_new_tab(
    sess: &Session,
    ctx: &ToolCallCtx,
    arguments: String,
) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_new_tab".to_string(),
        params,
        || async move {
            let browser_manager = match code_browser::global::get_browser_manager().await {
                Some(manager) => manager,
                None => code_browser::global::get_or_create_browser_manager().await,
            };
            browser_manager.set_enabled_sync(true);

            let url = serde_json::from_str::<Value>(&arguments_clone)
                .ok()
                .and_then(|json| {
                    json.get("url")
                        .and_then(serde_json::Value::as_str)
                        .map(std::string::ToString::to_string)
                })
                .unwrap_or_else(|| "about:blank".to_string());

            match browser_manager.new_tab(&url).await {
                Ok(target_id) => {
                    let payload = serde_json::json!({
                        "target_id": target_id,
                        "url": url,
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
                            "Failed to open new tab: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_switch_target(
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
        "browser_switch_target".to_string(),
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

            let Some(value) = params.as_ref() else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Missing target_id or index for action=switch_target".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let target_id = match resolve_target_id_from_value(&browser_manager, value).await {
                Ok(target_id) => target_id,
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

            match browser_manager.switch_to_target(&target_id).await {
                Ok(()) => {
                    let _ = browser_manager
                        .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                        .await;
                    let current_url = browser_manager
                        .get_current_url()
                        .await
                        .unwrap_or_else(|| "<unknown>".to_string());
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(format!(
                                "Switched to target {target_id} ({current_url})"
                            )),
                            success: Some(true),
                        },
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to switch browser target: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_activate_target(
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
        "browser_activate_target".to_string(),
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

            let Some(value) = params.as_ref() else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Missing target_id or index for action=activate_target".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let target_id = match resolve_target_id_from_value(&browser_manager, value).await {
                Ok(target_id) => target_id,
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

            match browser_manager.activate_target(&target_id).await {
                Ok(()) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Activated target {target_id}"
                        )),
                        success: Some(true),
                    },
                },
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to activate target: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

pub(super) async fn handle_browser_close_target(
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
        "browser_close_target".to_string(),
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

            let Some(value) = params.as_ref() else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(
                            "Missing target_id or index for action=close_target".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let target_id = match resolve_target_id_from_value(&browser_manager, value).await {
                Ok(target_id) => target_id,
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

            match browser_manager.close_target(&target_id).await {
                Ok(()) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Closed target {target_id}"
                        )),
                        success: Some(true),
                    },
                },
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        body: FunctionCallOutputBody::Text(format!(
                            "Failed to close target: {e}"
                        )),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

