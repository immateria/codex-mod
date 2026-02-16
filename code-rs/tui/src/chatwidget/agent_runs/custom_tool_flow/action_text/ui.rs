use super::super::*;
use crate::history_cell::{
    HistoryCellType,
    plain_message_state_from_lines,
    plain_message_state_from_paragraphs,
};
use tracing::error;

pub(super) fn insert_agent_start_message(
    chat: &mut ChatWidget<'_>,
    order_key: OrderKey,
    tracker: &AgentRunTracker,
) {
    let line = agent_start_line(tracker);
    let state = plain_message_state_from_lines(vec![line], HistoryCellType::BackgroundEvent);
    let cell = PlainHistoryCell::from_state(state);
    let _ = chat.history_insert_with_key_global_tagged(Box::new(cell), order_key, "background", None);
}

pub(super) fn report_missing_batch(
    chat: &mut ChatWidget<'_>,
    context: &str,
    call_id: Option<&str>,
    tool_name: Option<&str>,
    extra: Option<&str>,
) {
    error!(
        %context,
        call_id,
        tool_name,
        extra,
        "missing batch_id for agent event"
    );

    let mut message = format!("⚠️ {context}: missing agent batch_id.");
    if let Some(tool) = tool_name
        && !tool.is_empty()
    {
        message.push_str(&format!(" tool={tool}"));
    }
    if let Some(cid) = call_id
        && !cid.is_empty()
    {
        message.push_str(&format!(" call_id={cid}"));
    }
    if let Some(detail) = extra
        && !detail.is_empty()
    {
        message.push_str(&format!(" {detail}"));
    }

    let state = plain_message_state_from_paragraphs(
        PlainMessageKind::Error,
        PlainMessageRole::Error,
        [message],
    );
    let key = chat.next_internal_key();
    let cell = PlainHistoryCell::from_state(state);
    let _ = chat.history_insert_with_key_global(Box::new(cell), key);
}

fn agent_start_line(tracker: &AgentRunTracker) -> Line<'static> {
    let title = tracker
        .effective_label()
        .or_else(|| tracker.batch_id.as_ref().map(|id| short_batch_id(id)))
        .unwrap_or_else(|| "agent batch".to_string());

    let mut agents: Vec<String> = tracker.models.iter().cloned().collect();
    if agents.is_empty() {
        agents = tracker
            .agent_ids
            .iter()
            .filter_map(|id| {
                let trimmed = id.trim();
                if trimmed.is_empty() || looks_like_uuid(trimmed) {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();
    }
    agents.sort_unstable();
    agents.dedup();

    let agent_segment = if agents.is_empty() {
        None
    } else {
        Some(format!(" with agents {}", agents.join(", ")))
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw("Started "));
    spans.push(Span::styled(title, Style::new().bold()));
    if let Some(segment) = agent_segment {
        spans.push(Span::raw(segment));
    }

    Line::from(spans)
}
