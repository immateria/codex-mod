use super::tool_cards::ToolCardSlot;
use super::{ChatWidget, OrderKey, tool_cards};
use crate::history::state::{PlainMessageKind, PlainMessageRole};
use crate::history_cell::{AgentDetail, AgentRunCell, PlainHistoryCell, StepProgress};
use code_core::protocol::OrderMeta;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use std::time::Duration;

mod custom_tool_flow;
mod helpers;
mod status_flow;
mod tracker;

use helpers::{
    agent_batch_key,
    clean_label,
    dedup,
    format_elapsed_short,
    is_primary_run_tool,
    lines_from,
    looks_like_uuid,
    parse_progress,
    prune_agent_runs,
    short_batch_id,
    truncate_agent_details,
};
use helpers::is_agent_tool as is_agent_tool_impl;
pub(super) use tracker::AgentRunTracker;

pub(super) fn is_agent_tool(tool_name: &str) -> bool {
    is_agent_tool_impl(tool_name)
}

pub(super) fn handle_custom_tool_begin(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
    params: Option<serde_json::Value>,
) -> bool {
    custom_tool_flow::handle_custom_tool_begin(chat, order, call_id, tool_name, params)
}

pub(super) fn handle_custom_tool_end(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
    params: Option<serde_json::Value>,
    duration: Duration,
    result: &Result<String, String>,
) -> bool {
    custom_tool_flow::handle_custom_tool_end(
        chat, order, call_id, tool_name, params, duration, result,
    )
}

pub(super) fn handle_status_update(
    chat: &mut ChatWidget<'_>,
    event: &code_core::protocol::AgentStatusUpdateEvent,
) {
    status_flow::handle_status_update(chat, event);
}
