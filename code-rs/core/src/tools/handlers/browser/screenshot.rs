use super::helpers::get_browser_manager_for_session;
use super::helpers::selector_rect_after_scroll;
use crate::codex::Session;
use crate::codex::ToolCallCtx;
use crate::tools::events::execute_custom_tool;
use code_protocol::models::FunctionCallOutputBody;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde_json::Value;

pub(super) async fn handle_browser_screenshot(
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
        "browser_screenshot".to_string(),
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
                            "Invalid screenshot arguments".to_string(),
                        ),
                        success: Some(false),
                    },
                };
            };

            let mode = json
                .get("mode")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("viewport")
                .to_ascii_lowercase();

            let selector = json
                .get("selector")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(std::string::ToString::to_string);

            let region = json.get("region").cloned();

            let screenshot_mode = if let Some(selector) = selector.as_deref() {
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

                let (vw, vh) = browser_manager.get_viewport_size().await;
                let x0 = x.floor().max(0.0);
                let y0 = y.floor().max(0.0);
                let x1 = (x + w).ceil().min(vw as f64);
                let y1 = (y + h).ceil().min(vh as f64);
                if x1 <= x0 || y1 <= y0 {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "Selector region is empty; try a different selector.".to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                }

                let region = code_browser::page::ScreenshotRegion {
                    x: x0 as u32,
                    y: y0 as u32,
                    width: (x1 - x0) as u32,
                    height: (y1 - y0) as u32,
                };
                code_browser::page::ScreenshotMode::Region(region)
            } else if let Some(region_value) = region {
                let Some(obj) = region_value.as_object() else {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "region must be an object with x/y/width/height".to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                };

                let x = obj.get("x").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                let y = obj.get("y").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                let w = obj
                    .get("width")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let h = obj
                    .get("height")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                if w <= 0.0 || h <= 0.0 {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "region width/height must be > 0".to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                }

                let (vw, vh) = browser_manager.get_viewport_size().await;
                let x0 = x.floor().max(0.0);
                let y0 = y.floor().max(0.0);
                let x1 = (x + w).ceil().min(vw as f64);
                let y1 = (y + h).ceil().min(vh as f64);
                if x1 <= x0 || y1 <= y0 {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            body: FunctionCallOutputBody::Text(
                                "region is out of bounds".to_string(),
                            ),
                            success: Some(false),
                        },
                    };
                }

                let region = code_browser::page::ScreenshotRegion {
                    x: x0 as u32,
                    y: y0 as u32,
                    width: (x1 - x0) as u32,
                    height: (y1 - y0) as u32,
                };
                code_browser::page::ScreenshotMode::Region(region)
            } else if mode == "full_page" || mode == "fullpage" {
                let segments_max = json
                    .get("segments_max")
                    .and_then(serde_json::Value::as_u64)
                    .map(|v| v.clamp(1, 64) as usize);
                code_browser::page::ScreenshotMode::FullPage { segments_max }
            } else {
                code_browser::page::ScreenshotMode::Viewport
            };

            match browser_manager.capture_screenshot_mode(screenshot_mode).await {
                Ok((paths, url)) => {
                    let paths: Vec<String> =
                        paths.into_iter().map(|p| p.display().to_string()).collect();
                    let payload = serde_json::json!({
                        "url": url,
                        "paths": paths,
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
                        body: FunctionCallOutputBody::Text(format!("Screenshot failed: {e}")),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

