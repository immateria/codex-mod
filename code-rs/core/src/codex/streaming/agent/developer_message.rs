use super::*;

pub(super) fn merge_developer_message(existing: Option<String>, extra: &str) -> Option<String> {
    let extra_trimmed = extra.trim();
    if extra_trimmed.is_empty() {
        return existing;
    }

    match existing {
        Some(mut message) => {
            if !message.trim().is_empty() {
                message.push_str("\n\n");
            }
            message.push_str(extra_trimmed);
            Some(message)
        }
        None => Some(extra_trimmed.to_string()),
    }
}

pub(super) fn build_timeboxed_review_message(base: Option<String>) -> Option<String> {
    let mut message = merge_developer_message(base.clone(), AUTO_EXEC_TIMEBOXED_REVIEW_GUIDANCE);
    if base.as_deref() == Some(AUTO_EXEC_TIMEBOXED_CLI_GUIDANCE) {
        message = Some(AUTO_EXEC_TIMEBOXED_REVIEW_GUIDANCE.to_string());
    }
    message
}

