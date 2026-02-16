use super::{AgentDetail, ChatWidget, OrderKey, StepProgress};
use std::collections::HashSet;
use std::time::Duration;

const AGENT_TOOL_NAMES: &[&str] = &[
    "agent",
    "agent_run",
    "agent_result",
    "agent_wait",
    "agent_cancel",
    "agent_check",
    "agent_list",
];

// Memory guards for agent run outputs. These mirror the exec-stream caps to
// keep long Auto Drive sessions from retaining unbounded sub-agent chatter.
const AGENT_DETAIL_MAX_BYTES: usize = 512 * 1024; // total per-agent detail payload
const AGENT_DETAIL_MAX_LINES: usize = 200; // total detail entries per agent
const AGENT_DETAIL_LINE_MAX_BYTES: usize = 4 * 1024; // per-line tail we keep
const MAX_AGENT_RUNS: usize = 48; // global limit on tracked agent runs

pub(super) fn is_agent_tool(tool_name: &str) -> bool {
    AGENT_TOOL_NAMES
        .iter()
        .copied()
        .any(|name| name.eq_ignore_ascii_case(tool_name))
}

pub(super) fn is_primary_run_tool(tool_name: &str) -> bool {
    matches!(tool_name, "agent" | "agent_run")
}

pub(super) fn format_elapsed_short(duration: Duration) -> String {
    let secs = duration.as_secs();
    let minutes = secs / 60;
    let seconds = secs % 60;
    if minutes > 0 {
        format!("{minutes}m{seconds:02}s")
    } else {
        format!("{seconds:02}s")
    }
}

pub(super) fn agent_batch_key(batch_id: &str) -> String {
    format!("batch:{batch_id}")
}

pub(super) fn short_batch_id(batch_id: &str) -> String {
    let trimmed: String = batch_id.chars().filter(|c| *c != '-').collect();
    if trimmed.len() <= 8 {
        if trimmed.is_empty() {
            batch_id.to_string()
        } else {
            trimmed
        }
    } else {
        trimmed[..8].to_string()
    }
}

pub(super) fn looks_like_uuid(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    parts.iter()
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_hexdigit()))
}

pub(super) fn clean_label(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn parse_progress(progress: &str) -> Option<StepProgress> {
    for token in progress.split_whitespace() {
        if let Some((done, total)) = token.split_once('/') {
            let completed = done.trim().parse::<u32>().ok()?;
            let total = total.trim().parse::<u32>().ok()?;
            if total > 0 {
                return Some(StepProgress {
                    completed: completed.min(total),
                    total,
                });
            }
        }
    }
    None
}

pub(super) fn lines_from(input: &str) -> Vec<String> {
    input
        .lines()
        .map(std::string::ToString::to_string)
        .collect()
}

pub(super) fn dedup(mut values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
    values
}

fn order_tuple(order_key: OrderKey) -> (u64, i32, u64) {
    (order_key.req, order_key.out, order_key.seq)
}

pub(super) fn prune_agent_runs(chat: &mut ChatWidget<'_>) {
    while chat.tools_state.agent_runs.len() > MAX_AGENT_RUNS {
        let oldest_key = chat
            .tools_state
            .agent_runs
            .iter()
            .min_by_key(|(_, tracker)| order_tuple(tracker.slot.order_key))
            .map(|(key, _)| key.clone());

        let Some(key) = oldest_key else {
            break;
        };
        drop_agent_run(chat, key.as_str());
    }
}

fn drop_agent_run(chat: &mut ChatWidget<'_>, key: &str) {
    if let Some(tracker) = chat.tools_state.agent_runs.remove(key) {
        chat.tools_state.agent_run_by_call.retain(|_, v| v != key);
        chat.tools_state.agent_run_by_order.retain(|_, v| v != key);
        chat.tools_state.agent_run_by_batch.retain(|_, v| v != key);
        chat.tools_state.agent_run_by_agent.retain(|_, v| v != key);
        if chat.tools_state.agent_last_key.as_deref() == Some(key) {
            chat.tools_state.agent_last_key = None;
        }

        if let Some(id) = tracker.slot.history_id
            && let Some(idx) = chat.cell_index_for_history_id(id)
        {
            chat.history_remove_at(idx);
        }
    }
}

fn trim_to_tail(text: &str, max_bytes: usize) -> (String, bool) {
    let bytes = text.as_bytes();
    if bytes.len() <= max_bytes {
        return (text.to_string(), false);
    }

    // Keep the tail: most recent content is usually last.
    let start = bytes.len() - max_bytes;
    let mut start_idx = start;
    // Ensure we split on a UTF-8 boundary.
    while start_idx < bytes.len() && (bytes[start_idx] & 0b1100_0000) == 0b1000_0000 {
        start_idx += 1;
    }
    let trimmed = String::from_utf8_lossy(&bytes[start_idx..]).to_string();
    (format!("…{trimmed}"), true)
}

fn detail_text_len(detail: &AgentDetail) -> usize {
    match detail {
        AgentDetail::Progress(text)
        | AgentDetail::Result(text)
        | AgentDetail::Error(text)
        | AgentDetail::Info(text) => text.len(),
    }
}

pub(super) fn truncate_agent_details(details: &mut Vec<AgentDetail>) {
    // First, cap individual lines to avoid single gigantic entries.
    for detail in details.iter_mut() {
        match detail {
            AgentDetail::Progress(text)
            | AgentDetail::Result(text)
            | AgentDetail::Error(text)
            | AgentDetail::Info(text) => {
                let (trimmed, was_trimmed) = trim_to_tail(text, AGENT_DETAIL_LINE_MAX_BYTES);
                if was_trimmed {
                    *text = trimmed;
                }
            }
        }
    }

    // Then enforce total line + byte budgets, dropping oldest first.
    let mut total_bytes: usize = details.iter().map(detail_text_len).sum();

    while details.len() > AGENT_DETAIL_MAX_LINES || total_bytes > AGENT_DETAIL_MAX_BYTES {
        if details.is_empty() {
            break;
        }
        let removed = details.remove(0);
        total_bytes = total_bytes.saturating_sub(detail_text_len(&removed));
    }
}
