use super::super::super::*;
use super::super::metrics::{compute_agent_elapsed, resolve_agent_token_count};
use super::super::summary::{StatusSummary, classify_status, phase_to_status_kind};
use crate::history_cell::AgentStatusPreview;
use code_core::protocol::AgentInfo;
use std::time::Instant;

pub(super) struct PreviewBuildResult {
    pub(super) previews: Vec<AgentStatusPreview>,
    pub(super) status_collect: StatusSummary,
    pub(super) summary_lines: Option<Vec<String>>,
}

pub(super) fn build_previews(
    tracker: &mut AgentRunTracker,
    agents: &[AgentInfo],
) -> PreviewBuildResult {
    let mut previews: Vec<AgentStatusPreview> = Vec::new();
    let mut status_collect = StatusSummary::default();
    let mut summary_lines: Option<Vec<String>> = None;

    for agent in agents {
        tracker.agent_ids.insert(agent.id.clone());
        if let Some(agent_batch) = agent.batch_id.as_ref() {
            tracker.batch_id.get_or_insert(agent_batch.clone());
        }
        if let Some(model) = agent.model.as_ref() {
            tracker.merge_models([model.to_string()]);
        }
        if tracker
            .batch_label
            .as_ref()
            .map(|label| label.trim().is_empty())
            .unwrap_or(true)
            && let Some(cleaned) =
                clean_label(agent.name.as_str()).filter(|name| !looks_like_uuid(name))
        {
            tracker.batch_label = Some(cleaned);
        }

        let phase = classify_status(&agent.status, agent.result.is_some(), agent.error.is_some());

        let mut details: Vec<AgentDetail> = Vec::new();

        if let Some(result) = agent.result.as_ref() {
            let mut lines = lines_from(result);
            if lines.is_empty() {
                lines.push(result.clone());
            }
            let mut collected: Vec<String> = Vec::new();
            for line in lines {
                if !line.trim().is_empty() {
                    collected.push(line.clone());
                    details.push(AgentDetail::Result(line));
                }
            }
            if !collected.is_empty() {
                summary_lines = Some(collected);
            }
        }

        if details.is_empty()
            && let Some(error_text) = agent.error.as_ref()
        {
            let mut lines = lines_from(error_text);
            if lines.is_empty() {
                lines.push(error_text.clone());
            }
            let mut collected: Vec<String> = Vec::new();
            for line in lines {
                if !line.trim().is_empty() {
                    collected.push(line.clone());
                    details.push(AgentDetail::Error(line));
                }
            }
            if !collected.is_empty() {
                summary_lines = Some(collected);
            }
        }

        let step_progress = agent.last_progress.as_deref().and_then(parse_progress);

        if details.is_empty()
            && let Some(progress) = agent.last_progress.as_ref()
        {
            let mut lines = lines_from(progress);
            if lines.is_empty() {
                lines.push(progress.clone());
            }
            for line in lines {
                if !line.trim().is_empty() {
                    details.push(AgentDetail::Progress(line));
                }
            }
        }

        if details.is_empty() {
            details.push(AgentDetail::Info(agent.status.clone()));
        }

        truncate_agent_details(&mut details);

        let last_update = details.last().map(|detail| match detail {
            AgentDetail::Progress(text)
            | AgentDetail::Result(text)
            | AgentDetail::Error(text)
            | AgentDetail::Info(text) => text.clone(),
        });

        let elapsed = compute_agent_elapsed(tracker, agent.id.as_str(), agent.elapsed_ms, phase);
        let elapsed_updated_at = elapsed.map(|_| Instant::now());
        let token_count = resolve_agent_token_count(
            tracker,
            agent.id.as_str(),
            agent.token_count,
            &details,
        );

        let preview = AgentStatusPreview {
            id: agent.id.clone(),
            name: agent.name.clone(),
            status: agent.status.clone(),
            model: agent.model.clone(),
            details,
            status_kind: phase_to_status_kind(phase),
            step_progress,
            elapsed,
            token_count,
            last_update,
            elapsed_updated_at,
        };
        previews.push(preview);

        status_collect.observe(phase);

        if let Some(clean_name) = clean_label(agent.name.as_str()).filter(|name| !looks_like_uuid(name)) {
            tracker.set_agent_name(Some(clean_name), false);
        }
    }

    PreviewBuildResult {
        previews,
        status_collect,
        summary_lines,
    }
}
