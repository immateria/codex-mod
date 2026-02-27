use super::*;

pub(in crate::codex::streaming) fn inject_scratchpad_into_attempt_input(
    attempt_input: &mut Vec<ResponseItem>,
    sp: TurnScratchpad,
) {
    // Build a set of call ids we have already included to avoid duplicate call items.
    let mut seen_calls: std::collections::HashSet<String> = attempt_input
        .iter()
        .filter_map(|ri| match ri {
            ResponseItem::FunctionCall { call_id, .. } => Some(call_id.clone()),
            ResponseItem::CustomToolCall { call_id, .. } => Some(call_id.clone()),
            ResponseItem::LocalShellCall { call_id, id, .. } => call_id.clone().or_else(|| id.clone()),
            _ => None,
        })
        .collect();

    // Append finalized tool calls from the dropped attempt so retry payloads include
    // the same call ids as their tool outputs.
    for item in sp.items {
        let call_id = match &item {
            ResponseItem::FunctionCall { call_id, .. } => Some(call_id.as_str()),
            ResponseItem::CustomToolCall { call_id, .. } => Some(call_id.as_str()),
            ResponseItem::LocalShellCall { call_id, id, .. } => call_id.as_deref().or(id.as_deref()),
            _ => None,
        };

        let Some(call_id) = call_id else {
            continue;
        };

        if seen_calls.insert(call_id.to_string()) {
            attempt_input.push(item);
        }
    }

    // Append tool outputs produced during the dropped attempt.
    for resp in sp.responses {
        attempt_input.push(ResponseItem::from(resp));
    }

    // If we have partial deltas, include a short ephemeral hint so the model can resume.
    if !sp.partial_assistant_text.is_empty() || !sp.partial_reasoning_summary.is_empty() {
        use code_protocol::models::ContentItem;
        let mut hint = String::from(
            "[EPHEMERAL:RETRY_HINT]\nPrevious attempt aborted mid-stream. Continue without repeating.\n",
        );
        if !sp.partial_reasoning_summary.is_empty() {
            let s = &sp.partial_reasoning_summary;
            // Take the last 800 characters, respecting UTF-8 boundaries
            let start_idx = if s.chars().count() > 800 {
                s.char_indices()
                    .rev()
                    .nth(800 - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            } else {
                0
            };
            let tail = &s[start_idx..];
            hint.push_str(&format!("Last reasoning summary fragment:\n{tail}\n\n"));
        }
        if !sp.partial_assistant_text.is_empty() {
            let s = &sp.partial_assistant_text;
            // Take the last 800 characters, respecting UTF-8 boundaries
            let start_idx = if s.chars().count() > 800 {
                s.char_indices()
                    .rev()
                    .nth(800 - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            } else {
                0
            };
            let tail = &s[start_idx..];
            hint.push_str(&format!("Last assistant text fragment:\n{tail}\n"));
        }
        attempt_input.push(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: hint }],
            end_turn: None,
            phase: None,
        });
    }
}
