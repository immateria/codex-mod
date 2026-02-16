use super::super::metadata::InvocationMetadata;
use super::super::*;

pub(super) fn begin_action_for(tool_name: &str, metadata: &InvocationMetadata) -> Option<String> {
    let label = metadata
        .label
        .clone()
        .or_else(|| metadata.agent_ids.clone().into_iter().next())
        .unwrap_or_else(|| "agent".to_string());
    let action = resolved_action(tool_name, metadata.action.as_deref());

    match action {
        "create" => Some(format!("Started agent run for {label}")),
        "wait" => Some("Waiting for agents".to_string()),
        "result" => Some(format!("Requested results for {label}")),
        "cancel" => Some(format!("Cancelling agent batch for {label}")),
        "status" => Some(format!("Checking agent status for {label}")),
        "list" => Some("Listing available agents".to_string()),
        _ => None,
    }
}

pub(super) fn end_action_for(
    tool_name: &str,
    metadata: &InvocationMetadata,
    duration: Duration,
    success: bool,
    message: Option<&str>,
) -> Option<String> {
    let elapsed = format_elapsed_short(duration);
    let action = resolved_action(tool_name, metadata.action.as_deref());

    match action {
        "create" => {
            if success {
                None
            } else {
                let detail = message.unwrap_or("unknown error");
                Some(format!("Agent run failed in {elapsed} — {detail}"))
            }
        }
        "wait" => {
            if success {
                None
            } else {
                let detail = message.unwrap_or("wait failed");
                Some(format!("Wait failed in {elapsed} — {detail}"))
            }
        }
        "result" => {
            if success {
                None
            } else {
                let detail = message.unwrap_or("result error");
                Some(format!("Result fetch failed in {elapsed} — {detail}"))
            }
        }
        "cancel" => Some(format!("Cancel request completed in {elapsed}")),
        "status" => Some(format!("Status check finished in {elapsed}")),
        "list" => Some("Listed agents".to_string()),
        _ => None,
    }
}

fn resolved_action<'a>(tool_name: &'a str, action: Option<&'a str>) -> &'a str {
    action.unwrap_or(match tool_name {
        "agent" | "agent_run" => "create",
        "agent_wait" => "wait",
        "agent_result" => "result",
        "agent_cancel" => "cancel",
        "agent_check" => "status",
        "agent_list" => "list",
        other => other,
    })
}
