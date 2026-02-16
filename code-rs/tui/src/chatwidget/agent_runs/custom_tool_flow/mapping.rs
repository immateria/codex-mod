use super::*;

pub(super) fn order_key_and_ordinal(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
) -> (OrderKey, Option<u64>) {
    match order {
        Some(meta) => (
            chat.provider_order_key_from_order_meta(meta),
            Some(meta.request_ordinal),
        ),
        None => (chat.next_internal_key(), None),
    }
}

pub(super) fn update_mappings(
    chat: &mut ChatWidget<'_>,
    mut key: String,
    order: Option<&OrderMeta>,
    call_id: Option<&str>,
    ordinal: Option<u64>,
    tool_name: &str,
    tracker: &mut AgentRunTracker,
) -> String {
    let original_key = key.clone();

    if let Some(batch) = tracker.batch_id.as_ref() {
        let batch_key = agent_batch_key(batch);
        if batch_key != key {
            key = batch_key;
        }
    } else if is_primary_run_tool(tool_name)
        && let Some(ord) = ordinal
    {
        let ord_key = format!("req:{ord}:agent-run");
        if ord_key != key {
            key = ord_key;
        }
    }

    if let Some(cid) = call_id {
        tracker.call_ids.insert(cid.to_string());
        chat.tools_state
            .agent_run_by_call
            .insert(cid.to_string(), key.clone());
    }
    if let Some(meta) = order {
        chat.tools_state
            .agent_run_by_order
            .insert(meta.request_ordinal, key.clone());
    }
    if let Some(batch) = tracker.batch_id.as_ref() {
        chat.tools_state
            .agent_run_by_batch
            .insert(batch.clone(), key.clone());
    }
    for agent_id in &tracker.agent_ids {
        chat.tools_state
            .agent_run_by_agent
            .insert(agent_id.clone(), key.clone());
    }

    if key != original_key {
        for stored in chat.tools_state.agent_run_by_order.values_mut() {
            if *stored == original_key {
                *stored = key.clone();
            }
        }
        if let Some(batch) = tracker.batch_id.as_ref()
            && let Some(stored) = chat.tools_state.agent_run_by_batch.get_mut(batch)
            && *stored == original_key
        {
            *stored = key.clone();
        }
        for agent_id in &tracker.agent_ids {
            if let Some(stored) = chat.tools_state.agent_run_by_agent.get_mut(agent_id)
                && *stored == original_key
            {
                *stored = key.clone();
            }
        }
        for cid in &tracker.call_ids {
            chat.tools_state
                .agent_run_by_call
                .insert(cid.clone(), key.clone());
        }
    }

    key
}
