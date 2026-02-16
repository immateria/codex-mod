mod handler;
mod metrics;
mod summary;

pub(super) fn handle_status_update(
    chat: &mut super::ChatWidget<'_>,
    event: &code_core::protocol::AgentStatusUpdateEvent,
) {
    handler::handle_status_update(chat, event);
}
