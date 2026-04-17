use std::collections::BTreeMap;

use super::json_schema::JsonSchema;
use super::types::{OpenAiTool, ResponsesApiTool};

pub(super) fn create_browser_tool(browser_enabled: bool) -> OpenAiTool {
    #[cfg(not(feature = "browser-automation"))]
    let _browser_enabled = browser_enabled;

    #[cfg(feature = "browser-automation")]
    let mut actions = vec!["open", "status", "fetch"];
    #[cfg(not(feature = "browser-automation"))]
    let actions = ["status", "fetch"];

    #[cfg(feature = "browser-automation")]
    if browser_enabled {
        actions.extend([
            "close",
            "restart",
            "click",
            "click_selector",
            "move",
            "type",
            "type_selector",
            "key",
            "javascript",
            "scroll",
            "scroll_into_view",
            "wait_for",
            "history",
            "inspect",
            "inspect_selector",
            "console",
            "targets",
            "switch_target",
            "activate_target",
            "new_tab",
            "close_target",
            "screenshot",
            "cookies_get",
            "cookies_set",
            "storage_get",
            "storage_set",
            "cleanup",
            "cdp",
        ]);
    }

    let mut properties = BTreeMap::new();
    properties.insert(
        "action".to_owned(),
        JsonSchema::String {
            description: Some(
                "Required: choose one of the supported browser actions (e.g., 'open', 'click', 'fetch').".to_owned(),
            ),
            allowed_values: Some(actions.iter().map(ToString::to_string).collect()),
        },
    );

    properties.insert(
        "url".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=open/fetch/new_tab: URL to navigate to or retrieve (e.g., https://example.com).".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "type".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=click/click_selector: optional mouse event type ('click', 'mousedown', 'mouseup').".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "x".to_owned(),
        JsonSchema::Number {
            description: Some(
                "For actions=click/move/inspect: absolute X coordinate; use with 'y'.".to_owned(),
            ),
        },
    );
    properties.insert(
        "y".to_owned(),
        JsonSchema::Number {
            description: Some(
                "For actions=click/move/inspect: absolute Y coordinate; use with 'x'.".to_owned(),
            ),
        },
    );
    properties.insert(
        "dx".to_owned(),
        JsonSchema::Number {
            description: Some(
                "For action=move/scroll: relative X delta in CSS pixels (use with 'dy').".to_owned(),
            ),
        },
    );
    properties.insert(
        "dy".to_owned(),
        JsonSchema::Number {
            description: Some(
                "For action=move/scroll: relative Y delta in CSS pixels (use with 'dx').".to_owned(),
            ),
        },
    );
    properties.insert(
        "text".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=type/type_selector: text to send to the focused element.".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "key".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=key: key to press (e.g., Enter, Tab, Escape).".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "code".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=javascript: JavaScript source to execute in the browser context.".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "direction".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=history: history direction ('back' or 'forward').".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "id".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=inspect: optional element id (without '#') to inspect.".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "selector".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=click_selector/type_selector/inspect_selector/scroll_into_view/wait_for/screenshot: CSS selector (e.g., 'button.submit', '#main').".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "visible".to_owned(),
        JsonSchema::Boolean {
            description: Some(
                "For action=wait_for: when selector is provided, require element to be visible (non-zero bounding box).".to_owned(),
            ),
        },
    );
    properties.insert(
        "ready_state".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=wait_for: optional document.readyState target ('interactive' or 'complete').".to_owned(),
            ),
            allowed_values: Some(vec!["interactive".to_owned(), "complete".to_owned()]),
        },
    );
    properties.insert(
        "poll_ms".to_owned(),
        JsonSchema::Number {
            description: Some(
                "For action=wait_for: polling interval in milliseconds (default 200).".to_owned(),
            ),
        },
    );
    properties.insert(
        "segments_max".to_owned(),
        JsonSchema::Number {
            description: Some(
                "For action=screenshot (full_page): maximum segments to stitch (default 8).".to_owned(),
            ),
        },
    );
    properties.insert(
        "region".to_owned(),
        JsonSchema::Object {
            properties: BTreeMap::from([
                (
                    "x".to_owned(),
                    JsonSchema::Number {
                        description: Some("Region X in CSS pixels.".to_owned()),
                    },
                ),
                (
                    "y".to_owned(),
                    JsonSchema::Number {
                        description: Some("Region Y in CSS pixels.".to_owned()),
                    },
                ),
                (
                    "width".to_owned(),
                    JsonSchema::Number {
                        description: Some("Region width in CSS pixels.".to_owned()),
                    },
                ),
                (
                    "height".to_owned(),
                    JsonSchema::Number {
                        description: Some("Region height in CSS pixels.".to_owned()),
                    },
                ),
            ]),
            required: None,
            additional_properties: Some(false.into()),
        },
    );
    properties.insert(
        "lines".to_owned(),
        JsonSchema::Number {
            description: Some(
                "For action=console: optional number of recent console lines to return.".to_owned(),
            ),
        },
    );
    properties.insert(
        "target_id".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=switch_target/activate_target/close_target: CDP targetId of the tab to control (from action=targets).".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "index".to_owned(),
        JsonSchema::Number {
            description: Some(
                "For action=switch_target/activate_target/close_target: 1-based index from action=targets output.".to_owned(),
            ),
        },
    );
    properties.insert(
        "method".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=cdp: Chrome DevTools Protocol method name (e.g., 'Page.navigate').".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "params".to_owned(),
        JsonSchema::Object {
            properties: BTreeMap::new(),
            required: None,
            additional_properties: Some(true.into()),
        },
    );
    properties.insert(
        "target".to_owned(),
        JsonSchema::String {
            description: Some("For action=cdp: target session ('page' default or 'browser').".to_owned()),
            allowed_values: None,
        },
    );
    properties.insert(
        "timeout_ms".to_owned(),
        JsonSchema::Number {
            description: Some("For action=fetch/wait_for: optional timeout in milliseconds.".to_owned()),
        },
    );
    properties.insert(
        "mode".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=fetch: optional fetch mode ('auto', 'browser', or 'http'). For action=screenshot: 'viewport' (default) or 'full_page'. Use selector or region to capture a specific area.".to_owned(),
            ),
            allowed_values: None,
        },
    );
    properties.insert(
        "urls".to_owned(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some(
                "For action=cookies_get: optional list of URLs to filter cookies (uses Network.getCookies).".to_owned(),
            ),
        },
    );
    properties.insert(
        "cookies".to_owned(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::Object {
                properties: BTreeMap::new(),
                required: None,
                additional_properties: Some(true.into()),
            }),
            description: Some(
                "For action=cookies_set: list of cookie objects (passed through to Network.setCookie).".to_owned(),
            ),
        },
    );
    properties.insert(
        "storage".to_owned(),
        JsonSchema::String {
            description: Some(
                "For action=storage_get/storage_set: which storage to use ('local' or 'session').".to_owned(),
            ),
            allowed_values: Some(vec!["local".to_owned(), "session".to_owned()]),
        },
    );
    properties.insert(
        "keys".to_owned(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some(
                "For action=storage_get: optional list of keys to read (when omitted, returns all keys).".to_owned(),
            ),
        },
    );
    properties.insert(
        "items".to_owned(),
        JsonSchema::Object {
            properties: BTreeMap::new(),
            required: None,
            additional_properties: Some(true.into()),
        },
    );
    properties.insert(
        "clear".to_owned(),
        JsonSchema::Boolean {
            description: Some(
                "For action=storage_set: if true, clears storage before setting items.".to_owned(),
            ),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "browser".to_owned(),
        description: "Unified browser controller for navigation, interaction, console access, DevTools commands, and one-shot fetches. Use action=targets/switch_target to select a tab, then action=click/type/javascript/cdp for interactions.".to_owned(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["action".to_owned()]),
            additional_properties: Some(false.into()),
        },
    })
}
