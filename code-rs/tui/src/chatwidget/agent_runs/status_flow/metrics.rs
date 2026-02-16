use super::summary::AgentPhase;
use super::super::AgentRunTracker;
use crate::history_cell::AgentDetail;
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

pub(super) fn resolve_agent_token_count(
    tracker: &mut AgentRunTracker,
    agent_id: &str,
    explicit: Option<u64>,
    details: &[AgentDetail],
) -> Option<u64> {
    if let Some(value) = explicit {
        tracker.agent_token_counts.insert(agent_id.to_string(), value);
        return Some(value);
    }

    let inferred = details.iter().rev().find_map(|detail| match detail {
        AgentDetail::Progress(text)
        | AgentDetail::Result(text)
        | AgentDetail::Error(text)
        | AgentDetail::Info(text) => extract_token_count_from_text(text),
    });

    if let Some(value) = inferred {
        tracker.agent_token_counts.insert(agent_id.to_string(), value);
        return Some(value);
    }

    tracker.agent_token_counts.get(agent_id).copied()
}

fn extract_token_count_from_text(text: &str) -> Option<u64> {
    let lower = text.to_ascii_lowercase();
    if !lower.contains("token") && !lower.contains("tok") {
        return None;
    }

    let mut candidate = None;
    let mut fragment = String::new();

    for ch in text.chars() {
        if ch.is_ascii_digit() || matches!(ch, '.' | ',' | '_' | 'k' | 'K' | 'm' | 'M') {
            fragment.push(ch);
        } else {
            if let Some(value) = parse_token_fragment(&fragment) {
                candidate = Some(value);
            }
            fragment.clear();
        }
    }

    if let Some(value) = parse_token_fragment(&fragment) {
        candidate = Some(value);
    }

    candidate
}

fn parse_token_fragment(fragment: &str) -> Option<u64> {
    let trimmed = fragment.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut multiplier = 1f64;
    let mut base = trimmed;
    if let Some(last) = trimmed.chars().last() {
        match last {
            'k' | 'K' => {
                multiplier = 1_000f64;
                base = trimmed[..trimmed.len().saturating_sub(1)].trim();
            }
            'm' | 'M' => {
                multiplier = 1_000_000f64;
                base = trimmed[..trimmed.len().saturating_sub(1)].trim();
            }
            _ => {}
        }
    }

    let normalized = base.replace([',', '_'], "");
    if normalized.is_empty() {
        return None;
    }

    if normalized.chars().all(|c| c.is_ascii_digit()) {
        let value: u64 = normalized.parse().ok()?;
        let computed = (value as f64 * multiplier).round();
        if computed > 0.0 {
            return Some(computed as u64);
        }
        return None;
    }

    if normalized.contains('.') {
        let value: f64 = normalized.parse().ok()?;
        let computed = (value * multiplier).round();
        if computed > 0.0 {
            return Some(computed as u64);
        }
        return None;
    }

    None
}
