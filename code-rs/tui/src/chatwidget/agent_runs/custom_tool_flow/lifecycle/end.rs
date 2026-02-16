use super::shared::{apply_tracker_metadata, missing_batch_for_action};
use super::super::action_text::{end_action_for, normalize_end_action_text};
use super::super::mapping::update_mappings;
use super::super::metadata::InvocationMetadata;
use super::super::*;
use serde_json::Value;

pub(in super::super) fn handle_custom_tool_end(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
    params: Option<Value>,
    duration: Duration,
    result: &Result<String, String>,
) -> bool {
    if !is_agent_tool(tool_name) {
        return false;
    }

    let metadata = InvocationMetadata::from(tool_name, params.as_ref());
    if missing_batch_for_action(chat, "custom_tool_end", call_id, tool_name, &metadata) {
        return true;
    }

    let order_key = order
        .map(|meta| chat.provider_order_key_from_order_meta(meta))
        .unwrap_or_else(|| chat.next_internal_key());
    let ordinal = order.map(|m| m.request_ordinal);
    let Some(batch) = metadata.batch_id.as_ref() else {
        return false;
    };

    let (mut tracker, resolved_key) = match chat
        .tools_state
        .agent_run_by_batch
        .get(batch)
        .cloned()
        .and_then(|key| chat.tools_state.agent_runs.remove(&key))
    {
        Some(existing) => (existing, agent_batch_key(batch)),
        None => return false,
    };

    tracker.slot.set_order_key(order_key);
    tracker.batch_id.get_or_insert(batch.clone());
    apply_tracker_metadata(&mut tracker, &metadata, true);

    tracker.cell.set_duration(Some(duration));
    match result {
        Ok(text) => {
            let lines = lines_from(text);
            if !lines.is_empty() {
                tracker.cell.set_latest_result(lines);
            }
            tracker.cell.set_status_label("Completed");
            tracker.cell.mark_completed();
            if let Some(action) =
                end_action_for(tool_name, &metadata, duration, true, Some(text.as_str()))
                && let Some(message) = normalize_end_action_text(&tracker, &metadata, action)
            {
                tracker.cell.record_action(message);
            }
        }
        Err(err) => {
            tracker.cell.set_latest_result(vec![err.clone()]);
            tracker.cell.mark_failed();
            if let Some(action) =
                end_action_for(tool_name, &metadata, duration, false, Some(err.as_str()))
                && let Some(message) = normalize_end_action_text(&tracker, &metadata, action)
            {
                tracker.cell.record_action(message);
            }
        }
    }

    let key = update_mappings(
        chat,
        resolved_key,
        order,
        Some(call_id),
        ordinal,
        tool_name,
        &mut tracker,
    );
    tool_cards::assign_tool_card_key(&mut tracker.slot, &mut tracker.cell, Some(key.clone()));
    tool_cards::replace_tool_card::<AgentRunCell>(chat, &mut tracker.slot, &tracker.cell);
    chat.tools_state.agent_last_key = Some(key.clone());
    chat.tools_state.agent_runs.insert(key, tracker);
    prune_agent_runs(chat);

    true
}
