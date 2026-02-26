use crate::codex::Session;
use serde_json::Value;
use std::sync::Arc;

/// Get the browser manager for the session (always uses global).
pub(super) async fn get_browser_manager_for_session(
    _sess: &Session,
) -> Option<Arc<code_browser::BrowserManager>> {
    // Always use the global browser manager.
    code_browser::global::get_browser_manager().await
}

pub(super) async fn resolve_target_id_from_value(
    browser_manager: &code_browser::BrowserManager,
    value: &Value,
) -> Result<String, String> {
    if let Some(target_id) = value.get("target_id").and_then(serde_json::Value::as_str) {
        let trimmed = target_id.trim();
        if trimmed.is_empty() {
            return Err("target_id must be non-empty".to_string());
        }
        return Ok(trimmed.to_string());
    }

    if let Some(index) = value.get("index").and_then(serde_json::Value::as_u64) {
        if index == 0 {
            return Err("index must be >= 1".to_string());
        }
        let targets = browser_manager
            .list_page_targets()
            .await
            .map_err(|e| format!("Failed to list browser targets: {e}"))?;
        match targets.get((index - 1) as usize) {
            Some(target) => Ok(target.target_id.clone()),
            None => Err(format!(
                "index out of range (got {index}, available {len})",
                len = targets.len()
            )),
        }
    } else {
        Err("Missing target_id or index".to_string())
    }
}

pub(super) fn unwrap_execute_javascript_value(result: Value) -> Result<Value, String> {
    let Some(obj) = result.as_object() else {
        return Ok(result);
    };

    let success = obj
        .get("success")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if success {
        Ok(obj.get("value").cloned().unwrap_or(serde_json::Value::Null))
    } else {
        let err = obj
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("JavaScript execution failed");
        Err(err.to_string())
    }
}

pub(super) async fn selector_rect_after_scroll(
    browser_manager: &code_browser::BrowserManager,
    selector: &str,
    focus: bool,
) -> Result<(f64, f64, f64, f64), String> {
    let selector = selector.trim();
    if selector.is_empty() {
        return Err("selector must be non-empty".to_string());
    }

    let selector_json =
        serde_json::to_string(selector).map_err(|e| format!("Failed to encode selector: {e}"))?;
    let focus_js = if focus {
        "try{el.focus({preventScroll:true});}catch(_){try{el.focus();}catch(_){}}"
    } else {
        ""
    };

    let script = format!(
        r#"(function() {{
            try {{
                var sel = {selector_json};
                var el = document.querySelector(sel);
                if (!el) return {{ ok: false, error: "selector not found", selector: sel }};
                try {{
                    el.scrollIntoView({{ block: "center", inline: "center" }});
                }} catch (_) {{
                    try {{ el.scrollIntoView(); }} catch (_) {{}}
                }}
                {focus_js}
                var r = el.getBoundingClientRect();
                return {{
                    ok: true,
                    selector: sel,
                    rect: {{ x: r.x, y: r.y, width: r.width, height: r.height }},
                    scroll: {{ x: window.scrollX, y: window.scrollY }},
                }};
            }} catch (e) {{
                return {{ ok: false, error: String(e) }};
            }}
        }})()"#
    );

    let raw = browser_manager
        .execute_javascript(&script)
        .await
        .map_err(|e| format!("Failed to execute selector query: {e}"))?;
    let value = unwrap_execute_javascript_value(raw)?;

    if value
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        let rect = value.get("rect").ok_or_else(|| "Missing rect".to_string())?;
        let x = rect.get("x").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
        let y = rect.get("y").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
        let w = rect
            .get("width")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let h = rect
            .get("height")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        Ok((x, y, w, h))
    } else {
        let error = value
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("selector query failed");
        Err(error.to_string())
    }
}

pub(super) async fn node_id_for_selector(
    browser_manager: &code_browser::BrowserManager,
    selector: &str,
) -> Result<u64, String> {
    use serde_json::json;

    let selector = selector.trim();
    if selector.is_empty() {
        return Err("selector must be non-empty".to_string());
    }

    let doc = browser_manager
        .execute_cdp("DOM.getDocument", json!({}))
        .await
        .map_err(|e| format!("Failed to get document: {e}"))?;
    let root_node_id = doc
        .get("root")
        .and_then(|r| r.get("nodeId"))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "DOM.getDocument missing root.nodeId".to_string())?;

    let query = browser_manager
        .execute_cdp(
            "DOM.querySelector",
            json!({"nodeId": root_node_id, "selector": selector}),
        )
        .await
        .map_err(|e| format!("Failed to query selector: {e}"))?;
    let node_id = query
        .get("nodeId")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if node_id == 0 {
        return Err("selector not found".to_string());
    }
    Ok(node_id)
}

