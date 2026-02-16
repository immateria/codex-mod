use super::*;

mod action_text;
mod lifecycle;
mod mapping;
mod metadata;

pub(super) fn handle_custom_tool_begin(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
    params: Option<serde_json::Value>,
) -> bool {
    lifecycle::handle_custom_tool_begin(chat, order, call_id, tool_name, params)
}

pub(super) fn handle_custom_tool_end(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
    params: Option<serde_json::Value>,
    duration: std::time::Duration,
    result: &Result<String, String>,
) -> bool {
    lifecycle::handle_custom_tool_end(chat, order, call_id, tool_name, params, duration, result)
}

pub(super) fn update_mappings(
    chat: &mut ChatWidget<'_>,
    key: String,
    order: Option<&OrderMeta>,
    call_id: Option<&str>,
    ordinal: Option<u64>,
    tool_name: &str,
    tracker: &mut AgentRunTracker,
) -> String {
    mapping::update_mappings(chat, key, order, call_id, ordinal, tool_name, tracker)
}
