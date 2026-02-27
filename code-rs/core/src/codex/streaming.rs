use super::*;
use super::session::{
    QueuedUserInput,
    State,
    TurnScratchpad,
    WaitInterruptReason,
    account_usage_context,
    format_retry_eta,
    is_connectivity_error,
    spawn_usage_task,
};
use super::agent_tool_call::{
    agent_completion_wake_messages,
    enqueue_agent_completion_wake,
    get_last_assistant_message_from_turn,
    send_agent_status_update,
};
use crate::auth;
use crate::auth_accounts;
use crate::account_switching::RateLimitSwitchState;
use crate::collaboration_mode_instructions::{
    render_collaboration_mode_instructions,
};
use crate::openai_tools::OpenAiTool;
use crate::openai_tools::ResponsesApiTool;
use crate::openai_tools::SEARCH_TOOL_BM25_TOOL_NAME;
use crate::protocol::McpListToolsResponseEvent;
use crate::tools::scheduler::PendingToolCall;
use code_app_server_protocol::AuthMode as AppAuthMode;

mod attempt_recovery;
mod agent;
mod env_context;
mod mcp_convert;
mod submission;
mod turn;
#[cfg(test)]
mod tests;

pub(super) use agent::AgentTask;
pub(super) use submission::submission_loop;

use attempt_recovery::{
    HTML_SANITIZER_GUARDRAILS_MESSAGE,
    SEARCH_TOOL_DEVELOPER_INSTRUCTIONS,
    inject_scratchpad_into_attempt_input,
    missing_tool_outputs_to_insert,
    reconcile_pending_tool_outputs,
    should_inject_html_sanitizer_guardrails,
    should_inject_search_tool_developer_instructions,
};
pub(super) use env_context::{
    TimelineReplayContext,
    debug_history,
    parse_env_delta_from_response,
    parse_env_snapshot_from_response,
    process_rollout_env_item,
};
use mcp_convert::{
    convert_mcp_resource_templates_by_server,
    convert_mcp_resources_by_server,
};

pub(super) fn add_pending_screenshot(
    sess: &Session,
    screenshot_path: PathBuf,
    url: String,
) {
    // Do not queue screenshots for next turn anymore; we inject fresh per-turn.
    tracing::info!("Captured screenshot; updating UI and using per-turn injection");

    // Also send an immediate event to update the TUI display
    let event = sess.make_event(
        "browser_screenshot",
        EventMsg::BrowserScreenshotUpdate(BrowserScreenshotUpdateEvent {
            screenshot_path,
            url,
        }),
    );

    // Send event asynchronously to avoid blocking
    let tx_event = sess.tx_event.clone();
    tokio::spawn(async move {
        if let Err(e) = tx_event.send(event).await {
            tracing::error!("Failed to send browser screenshot update event: {}", e);
        }
    });
}
