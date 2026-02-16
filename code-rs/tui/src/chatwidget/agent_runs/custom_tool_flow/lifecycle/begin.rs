use super::shared::{apply_tracker_metadata, missing_batch_for_action};
use super::super::action_text::{
    begin_action_for,
    insert_agent_start_message,
    normalize_begin_action_text,
};
use super::super::mapping::{order_key_and_ordinal, update_mappings};
use super::super::metadata::InvocationMetadata;
use super::super::*;
use serde_json::Value;

pub(in super::super) fn handle_custom_tool_begin(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
    params: Option<Value>,
) -> bool {
    if !is_agent_tool(tool_name) {
        return false;
    }

    let metadata = InvocationMetadata::from(tool_name, params.as_ref());
    if missing_batch_for_action(chat, "custom_tool_begin", call_id, tool_name, &metadata) {
        return true;
    }

    let (order_key, ordinal) = order_key_and_ordinal(chat, order);
    let Some(batch) = metadata.batch_id.as_ref() else {
        return true;
    };

    let (mut tracker, resolved_key) = match chat
        .tools_state
        .agent_run_by_batch
        .get(batch)
        .cloned()
        .and_then(|key| chat.tools_state.agent_runs.remove(&key))
    {
        Some(existing) => (existing, agent_batch_key(batch)),
        None => (AgentRunTracker::new(order_key), agent_batch_key(batch)),
    };
    tracker.slot.set_order_key(order_key);
    tracker.batch_id.get_or_insert(batch.clone());

    apply_tracker_metadata(&mut tracker, &metadata, false);

    if matches!(metadata.action.as_deref(), Some("create")) && !tracker.anchor_inserted {
        insert_agent_start_message(chat, tracker.slot.order_key, &tracker);
        tracker.anchor_inserted = true;
    }

    let header_label = tracker.effective_label().or_else(|| tracker.batch_id.clone());
    tracker.cell.set_batch_label(header_label);

    if let Some(action) = begin_action_for(tool_name, &metadata)
        && let Some(message) = normalize_begin_action_text(&tracker, &metadata, action)
    {
        tracker.cell.record_action(message);
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
    let header_label = tracker.batch_label.clone().or_else(|| tracker.batch_id.clone());
    tracker.cell.set_batch_label(header_label);
    tool_cards::replace_tool_card::<AgentRunCell>(chat, &mut tracker.slot, &tracker.cell);
    chat.tools_state.agent_last_key = Some(key.clone());
    chat.tools_state.agent_runs.insert(key, tracker);
    prune_agent_runs(chat);

    true
}
