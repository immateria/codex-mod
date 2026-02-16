use super::super::metadata::InvocationMetadata;
use super::super::*;

pub(super) fn normalize_begin_action_text(
    tracker: &AgentRunTracker,
    metadata: &InvocationMetadata,
    action: String,
) -> Option<String> {
    if action.is_empty() {
        return None;
    }

    if action.starts_with("Waiting for agents") {
        return Some("Waiting for agents".to_string());
    }

    if action.starts_with("Requested results for ") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some("Requested results".to_string());
        }
        return Some(format!("Requested results for {}", names.join(", ")));
    }

    if action.starts_with("Cancelling agent batch for ") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some("Cancelling agent batch".to_string());
        }
        return Some(format!("Cancelling agents {}", names.join(", ")));
    }

    if action.starts_with("Checking agent status for ") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some("Checking agent status".to_string());
        }
        return Some(format!("Checking agent status for {}", names.join(", ")));
    }

    if action.starts_with("Started agent run for ") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            if let Some(label) = metadata
                .label
                .as_ref()
                .and_then(|value| clean_label(value))
                .filter(|value| !looks_like_uuid(value))
            {
                return Some(format!("Started agent run for {label}"));
            }
            return Some(action);
        }
        return Some(format!("Started agent run for {}", names.join(", ")));
    }

    Some(action)
}

pub(super) fn normalize_end_action_text(
    tracker: &AgentRunTracker,
    metadata: &InvocationMetadata,
    action: String,
) -> Option<String> {
    if action.is_empty() {
        return None;
    }

    if action.starts_with("Agent run failed") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some(action);
        }
        return Some(format!("{} — {}", names.join(", "), action));
    }

    if action.starts_with("Wait failed") {
        return Some(action);
    }

    if action.starts_with("Result fetch failed") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some(action);
        }
        let detail = action.trim_start_matches("Result fetch failed in ");
        return Some(format!(
            "Result fetch failed for {} in {}",
            names.join(", "),
            detail
        ));
    }

    Some(action)
}

fn friendly_agent_names(metadata: &InvocationMetadata, tracker: &AgentRunTracker) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for id in &metadata.agent_ids {
        if let Some(name) = tracker.cell.agent_name_for_id(id) {
            names.push(name);
        } else if !looks_like_uuid(id.as_str()) {
            names.push(id.clone());
        }
    }
    if names.is_empty()
        && let Some(label) = metadata
            .label
            .as_ref()
            .and_then(|value| clean_label(value))
            .filter(|value| !looks_like_uuid(value))
    {
        names.push(label);
    }
    names.sort_unstable();
    names.dedup();
    names
}
