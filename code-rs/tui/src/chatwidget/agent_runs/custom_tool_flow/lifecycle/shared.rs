use super::super::action_text::report_missing_batch;
use super::super::metadata::InvocationMetadata;
use super::super::*;

pub(super) fn missing_batch_for_action(
    chat: &mut ChatWidget<'_>,
    context: &str,
    call_id: &str,
    tool_name: &str,
    metadata: &InvocationMetadata,
) -> bool {
    if metadata.batch_id.is_some() {
        return false;
    }

    if action_requires_batch(metadata.action.as_deref()) {
        report_missing_batch(
            chat,
            context,
            Some(call_id),
            Some(tool_name),
            metadata.action.as_deref(),
        );
    }
    true
}

pub(super) fn apply_tracker_metadata(
    tracker: &mut AgentRunTracker,
    metadata: &InvocationMetadata,
    write_only_if_unset: bool,
) {
    let raw_label = metadata.label.as_ref().map(std::string::ToString::to_string);
    let clean_label_value = raw_label.as_ref().and_then(|value| clean_label(value));
    if let Some(ref cleaned) = clean_label_value
        && !looks_like_uuid(cleaned)
    {
        tracker.batch_label = Some(cleaned.clone());
    }

    tracker.merge_agent_ids(metadata.agent_ids.clone());
    tracker.merge_models(metadata.models.clone());

    let name_for_cell = clean_label_value
        .or(raw_label)
        .filter(|value| !looks_like_uuid(value));
    tracker.set_agent_name(name_for_cell, true);

    if !write_only_if_unset || tracker.write_enabled.is_none() {
        tracker.set_write_mode(metadata.resolved_write_flag());
    }

    if !metadata.plan.is_empty() {
        tracker.cell.set_plan(metadata.plan.clone());
    }
    tracker.set_context(metadata.context.clone());
    tracker.set_task(metadata.task.clone());
}

fn action_requires_batch(action: Option<&str>) -> bool {
    matches!(action, Some("create") | Some("wait") | Some("result") | Some("cancel"))
}
