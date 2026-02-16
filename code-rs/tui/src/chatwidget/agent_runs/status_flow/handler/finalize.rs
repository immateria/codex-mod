use super::preview_build::PreviewBuildResult;
use super::super::super::custom_tool_flow::update_mappings;
use super::super::super::*;
use crate::history_cell::{AgentRunCell, AgentStatusKind};

pub(super) fn apply_and_store(
    chat: &mut ChatWidget<'_>,
    mut tracker: AgentRunTracker,
    resolved_key: String,
    build: PreviewBuildResult,
) {
    tracker.cell.set_agent_overview(build.previews.clone());
    let header_label = tracker.effective_label().or_else(|| tracker.batch_id.clone());
    tracker.cell.set_batch_label(header_label);
    build.status_collect.apply(&mut tracker.cell);

    if let Some(lines) = build.summary_lines {
        tracker.cell.set_latest_result(lines);
    } else {
        tracker.cell.set_latest_result(Vec::new());
    }

    let mut status_updates: Vec<String> = Vec::new();
    for preview in &build.previews {
        let current_kind = preview.status_kind;
        let previous_kind = tracker.agent_announced_status.get(preview.id.as_str()).copied();
        tracker
            .agent_announced_status
            .insert(preview.id.clone(), current_kind);

        let should_emit = matches!(
            current_kind,
            AgentStatusKind::Completed | AgentStatusKind::Failed | AgentStatusKind::Cancelled
        ) && previous_kind != Some(current_kind);

        if !should_emit {
            continue;
        }

        let label = tracker
            .cell
            .agent_name_for_id(preview.id.as_str())
            .or_else(|| clean_label(preview.name.as_str()).filter(|value| !looks_like_uuid(value)))
            .unwrap_or_else(|| {
                let trimmed = preview.name.trim();
                if trimmed.is_empty() || looks_like_uuid(trimmed) {
                    preview.id.clone()
                } else {
                    trimmed.to_string()
                }
            });

        match current_kind {
            AgentStatusKind::Completed => status_updates.push(format!("{label} completed")),
            AgentStatusKind::Failed => status_updates.push(format!("{label} failed")),
            AgentStatusKind::Cancelled => status_updates.push(format!("{label} cancelled")),
            _ => {}
        }
    }

    if !status_updates.is_empty() {
        let message = if status_updates.len() == 1 {
            status_updates.remove(0)
        } else {
            status_updates.join("; ")
        };
        tracker.cell.record_action(message);
    }

    let mut current_key = resolved_key;
    current_key = update_mappings(chat, current_key, None, None, None, "agent_status", &mut tracker);
    tool_cards::assign_tool_card_key(&mut tracker.slot, &mut tracker.cell, Some(current_key.clone()));
    tool_cards::replace_tool_card::<AgentRunCell>(chat, &mut tracker.slot, &tracker.cell);
    chat.tools_state.agent_last_key = Some(current_key.clone());
    chat.tools_state.agent_runs.insert(current_key, tracker);
    prune_agent_runs(chat);
}
