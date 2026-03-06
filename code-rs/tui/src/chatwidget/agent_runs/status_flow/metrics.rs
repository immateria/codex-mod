use super::summary::AgentPhase;
use super::super::AgentRunTracker;
use std::time::{Duration, Instant};

pub(super) fn compute_agent_elapsed(
    tracker: &mut AgentRunTracker,
    agent_id: &str,
    elapsed_ms: Option<u64>,
    phase: AgentPhase,
) -> Option<Duration> {
    if let Some(ms) = elapsed_ms {
        let duration = Duration::from_millis(ms);
        tracker.agent_elapsed.insert(agent_id.to_string(), duration);
        if matches!(
            phase,
            AgentPhase::Completed | AgentPhase::Failed | AgentPhase::Cancelled
        ) {
            tracker.agent_started_at.remove(agent_id);
        }
        return Some(duration);
    }

    let start_entry = tracker
        .agent_started_at
        .entry(agent_id.to_string())
        .or_insert_with(Instant::now);
    let duration = start_entry.elapsed();

    let entry = tracker
        .agent_elapsed
        .entry(agent_id.to_string())
        .or_insert(duration);
    if duration > *entry {
        *entry = duration;
    }

    if matches!(
        phase,
        AgentPhase::Completed | AgentPhase::Failed | AgentPhase::Cancelled
    ) {
        tracker.agent_started_at.remove(agent_id);
    }

    tracker.agent_elapsed.get(agent_id).copied()
}
