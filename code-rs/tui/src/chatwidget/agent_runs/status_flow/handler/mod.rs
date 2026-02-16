mod finalize;
mod grouping;
mod preview_build;

use super::super::*;
use code_core::protocol::{AgentInfo, AgentStatusUpdateEvent};

pub(super) fn handle_status_update(chat: &mut ChatWidget<'_>, event: &AgentStatusUpdateEvent) {
    for (batch_id, agents) in grouping::grouped_batches(event) {
        process_status_update_for_batch(chat, &batch_id, &agents, event);
    }
}

fn process_status_update_for_batch(
    chat: &mut ChatWidget<'_>,
    batch_id: &str,
    agents: &[AgentInfo],
    event: &AgentStatusUpdateEvent,
) {
    let (mut tracker, resolved_key) = grouping::take_or_create_tracker(chat, batch_id);
    grouping::apply_event_context(&mut tracker, event);
    let build = preview_build::build_previews(&mut tracker, agents);
    finalize::apply_and_store(chat, tracker, resolved_key, build);
}
