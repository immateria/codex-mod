use super::*;
use super::metadata::InvocationMetadata;

mod builders;
mod normalizers;
mod ui;

pub(super) fn begin_action_for(tool_name: &str, metadata: &InvocationMetadata) -> Option<String> {
    builders::begin_action_for(tool_name, metadata)
}

pub(super) fn end_action_for(
    tool_name: &str,
    metadata: &InvocationMetadata,
    duration: Duration,
    success: bool,
    message: Option<&str>,
) -> Option<String> {
    builders::end_action_for(tool_name, metadata, duration, success, message)
}

pub(super) fn normalize_begin_action_text(
    tracker: &AgentRunTracker,
    metadata: &InvocationMetadata,
    action: String,
) -> Option<String> {
    normalizers::normalize_begin_action_text(tracker, metadata, action)
}

pub(super) fn normalize_end_action_text(
    tracker: &AgentRunTracker,
    metadata: &InvocationMetadata,
    action: String,
) -> Option<String> {
    normalizers::normalize_end_action_text(tracker, metadata, action)
}

pub(super) fn insert_agent_start_message(
    chat: &mut ChatWidget<'_>,
    order_key: OrderKey,
    tracker: &AgentRunTracker,
) {
    ui::insert_agent_start_message(chat, order_key, tracker)
}

pub(super) fn report_missing_batch(
    chat: &mut ChatWidget<'_>,
    context: &str,
    call_id: Option<&str>,
    tool_name: Option<&str>,
    extra: Option<&str>,
) {
    ui::report_missing_batch(chat, context, call_id, tool_name, extra)
}
