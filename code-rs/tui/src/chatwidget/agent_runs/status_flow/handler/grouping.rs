use super::super::super::*;
use code_core::protocol::{AgentInfo, AgentSourceKind, AgentStatusUpdateEvent};

pub(super) fn grouped_batches(event: &AgentStatusUpdateEvent) -> Vec<(String, Vec<AgentInfo>)> {
    if event.agents.is_empty() {
        return Vec::new();
    }

    let filtered_agents: Vec<AgentInfo> = event
        .agents
        .iter()
        .filter(|agent| {
            !matches!(agent.source_kind, Some(AgentSourceKind::AutoReview))
                && !agent
                    .batch_id
                    .as_deref()
                    .map(|batch| batch.eq_ignore_ascii_case("auto-review"))
                    .unwrap_or(false)
        })
        .cloned()
        .collect();

    if filtered_agents.is_empty() {
        return Vec::new();
    }

    let mut grouped: Vec<(String, Vec<AgentInfo>)> = Vec::new();
    for agent in filtered_agents {
        if let Some(batch_id) = agent.batch_id.clone() {
            if let Some((_, bucket)) = grouped.iter_mut().find(|(id, _)| id == &batch_id) {
                bucket.push(agent);
            } else {
                grouped.push((batch_id, vec![agent]));
            }
        }
    }

    grouped
}

pub(super) fn take_or_create_tracker(
    chat: &mut ChatWidget<'_>,
    batch_id: &str,
) -> (AgentRunTracker, String) {
    let (mut tracker, resolved_key) = match chat
        .tools_state
        .agent_run_by_batch
        .get(batch_id)
        .cloned()
        .and_then(|key| {
            chat.tools_state
                .agent_runs
                .remove(&key)
                .map(|tracker| (tracker, key))
        }) {
        Some((tracker, key)) => (tracker, key),
        None => {
            let order_key = chat.next_internal_key();
            tracing::warn!(batch_id, "status_update received with no existing tracker; creating placeholder");
            (AgentRunTracker::new(order_key), agent_batch_key(batch_id))
        }
    };
    let order_key = tracker
        .slot
        .last_inserted_order()
        .unwrap_or(tracker.slot.order_key);
    tracker.slot.set_order_key(order_key);
    tracker.batch_id.get_or_insert(batch_id.to_string());

    (tracker, resolved_key)
}

pub(super) fn apply_event_context(tracker: &mut AgentRunTracker, event: &AgentStatusUpdateEvent) {
    if tracker.context.is_none()
        && let Some(context) = event.context.clone()
    {
        tracker.set_context(Some(context));
    }

    if tracker.task.is_none()
        && let Some(task) = event.task.clone()
    {
        tracker.set_task(Some(task));
    }
}
