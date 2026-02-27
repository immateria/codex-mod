use super::*;

use std::collections::HashSet;

pub(super) const HTML_SANITIZER_GUARDRAILS_MESSAGE: &str =
    "TB2 HTML/XSS guardrails:\n- Do NOT use DOTALL/full-document regex (e.g. `<script.*?>.*?</script>`); catastrophic backtracking risk.\n- Prefer linear-time scanning with quote/state tracking; if using regex, only on bounded substrings (single tags).\n- Perf smoke test: write malformed `/tmp/stress.html` and run `timeout 5s python3 /app/filter.py /tmp/stress.html` (or equivalent). If it times out, rewrite for linear-time behavior.";
pub(super) const SEARCH_TOOL_DEVELOPER_INSTRUCTIONS: &str =
    include_str!("../../../templates/search_tool/developer_instructions.md");

pub(super) fn should_inject_html_sanitizer_guardrails(input: &[ResponseItem]) -> bool {
    let mut user_messages_seen = 0u32;
    let mut text = String::new();
    for item in input.iter().rev() {
        if user_messages_seen >= 6 || text.len() >= 1_200 {
            break;
        }
        let ResponseItem::Message { role, content, .. } = item else {
            continue;
        };
        if role != "user" {
            continue;
        }
        user_messages_seen = user_messages_seen.saturating_add(1);
        for entry in content {
            let ContentItem::InputText { text: piece } = entry else {
                continue;
            };
            if piece.trim().is_empty() {
                continue;
            }
            text.push_str(piece);
            text.push('\n');
            if text.len() >= 1_200 {
                break;
            }
        }
    }

    if text.is_empty() {
        return false;
    }

    let lower = text.to_ascii_lowercase();
    let has_xss = lower.contains("xss");
    let has_sanitize = lower.contains("sanitize") || lower.contains("sanitiz");
    let has_filter_js_from_html =
        lower.contains("filter-js-from-html") || lower.contains("break-filter-js-from-html");
    let has_html = lower.contains("html");
    let has_script_tag =
        lower.contains("<script") || lower.contains("script tag") || lower.contains("script-tag");
    let has_filtering =
        lower.contains("filter") || lower.contains("strip") || lower.contains("remove");

    has_xss
        || has_sanitize
        || has_filter_js_from_html
        || (has_html && has_script_tag && has_filtering)
}

pub(super) fn should_inject_search_tool_developer_instructions(tools: &[OpenAiTool]) -> bool {
    tools.iter().any(|tool| {
        matches!(tool, OpenAiTool::Function(ResponsesApiTool { name, .. }) if name == SEARCH_TOOL_BM25_TOOL_NAME)
    })
}

pub(super) fn inject_scratchpad_into_attempt_input(
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

#[cfg(test)]
mod search_tool_instructions_tests {
    use super::*;

    #[test]
    fn detects_search_tool_presence() {
        let tools = vec![OpenAiTool::Function(ResponsesApiTool {
            name: SEARCH_TOOL_BM25_TOOL_NAME.to_string(),
            description: "search".to_string(),
            strict: false,
            parameters: crate::openai_tools::JsonSchema::Object {
                properties: Default::default(),
                required: None,
                additional_properties: None,
            },
        })];
        assert!(should_inject_search_tool_developer_instructions(&tools));
    }

    #[test]
    fn ignores_non_search_tools() {
        let tools = vec![OpenAiTool::Function(ResponsesApiTool {
            name: "not_search_tool".to_string(),
            description: "other".to_string(),
            strict: false,
            parameters: crate::openai_tools::JsonSchema::Object {
                properties: Default::default(),
                required: None,
                additional_properties: None,
            },
        })];
        assert!(!should_inject_search_tool_developer_instructions(&tools));
    }
}

pub(super) fn reconcile_pending_tool_outputs(
    pending_outputs: &[ResponseItem],
    rebuilt_history: &[ResponseItem],
    previous_input_snapshot: &[ResponseItem],
) -> (Vec<ResponseItem>, Vec<ResponseItem>) {
    let mut call_ids = collect_tool_call_ids(rebuilt_history);
    let mut missing_calls = Vec::new();
    let mut filtered_outputs = Vec::new();

    for item in pending_outputs {
        match item {
            ResponseItem::FunctionCallOutput { call_id, .. }
            | ResponseItem::CustomToolCallOutput { call_id, .. } => {
                if call_ids.contains(call_id) {
                    filtered_outputs.push(item.clone());
                    continue;
                }

                if let Some(call_item) = find_call_item_by_id(previous_input_snapshot, call_id) {
                    call_ids.insert(call_id.clone());
                    missing_calls.push(call_item);
                    filtered_outputs.push(item.clone());
                } else {
                    warn!("Skipping tool output for missing call_id={call_id} after auto-compact");
                }
            }
            _ => {
                filtered_outputs.push(item.clone());
            }
        }
    }

    (missing_calls, filtered_outputs)
}

fn collect_tool_call_ids(items: &[ResponseItem]) -> HashSet<String> {
    let mut ids = HashSet::new();
    for item in items {
        match item {
            ResponseItem::FunctionCall { call_id, .. } => {
                ids.insert(call_id.clone());
            }
            ResponseItem::LocalShellCall { call_id, id, .. } => {
                if let Some(call_id) = call_id.as_ref().or(id.as_ref()) {
                    ids.insert(call_id.clone());
                }
            }
            ResponseItem::CustomToolCall { call_id, .. } => {
                ids.insert(call_id.clone());
            }
            _ => {}
        }
    }
    ids
}

fn find_call_item_by_id(items: &[ResponseItem], call_id: &str) -> Option<ResponseItem> {
    items.iter().rev().find_map(|item| match item {
        ResponseItem::FunctionCall { call_id: existing, .. } if existing == call_id => Some(item.clone()),
        ResponseItem::LocalShellCall { call_id: call_id_field, id, .. } => {
            let effective = call_id_field.as_deref().or(id.as_deref());
            if effective == Some(call_id) {
                Some(item.clone())
            } else {
                None
            }
        }
        ResponseItem::CustomToolCall { call_id: existing, .. } if existing == call_id => Some(item.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tool_call_id_tests {
    use super::*;

    fn legacy_local_shell_call(id: &str) -> ResponseItem {
        ResponseItem::LocalShellCall {
            id: Some(id.to_string()),
            call_id: None,
            status: code_protocol::models::LocalShellStatus::Completed,
            action: code_protocol::models::LocalShellAction::Exec(
                code_protocol::models::LocalShellExecAction {
                    command: vec!["echo".to_string(), "hi".to_string()],
                    timeout_ms: None,
                    working_directory: None,
                    env: None,
                    user: None,
                },
            ),
        }
    }

    #[test]
    fn collect_tool_call_ids_includes_local_shell_id() {
        let items = vec![legacy_local_shell_call("sh_1")];
        let ids = collect_tool_call_ids(&items);
        assert!(ids.contains("sh_1"));
    }

    #[test]
    fn find_call_item_by_id_matches_local_shell_id() {
        let items = vec![legacy_local_shell_call("sh_1")];
        let found = find_call_item_by_id(&items, "sh_1");
        assert!(matches!(
            found,
            Some(ResponseItem::LocalShellCall { id: Some(id), .. }) if id == "sh_1"
        ));
    }

    #[test]
    fn retry_scratchpad_injects_custom_tool_call_before_output() {
        let sp = TurnScratchpad {
            items: vec![ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id: "c1".to_string(),
                name: "apply_patch".to_string(),
                input: "*** Begin Patch\n*** End Patch".to_string(),
            }],
            responses: vec![ResponseInputItem::CustomToolCallOutput {
                call_id: "c1".to_string(),
                output: "ok".to_string(),
            }],
            partial_assistant_text: String::new(),
            partial_reasoning_summary: String::new(),
        };

        let mut attempt_input: Vec<ResponseItem> = Vec::new();
        inject_scratchpad_into_attempt_input(&mut attempt_input, sp);

        let call_pos = attempt_input
            .iter()
            .position(|item| matches!(item, ResponseItem::CustomToolCall { call_id, .. } if call_id == "c1"))
            .expect("expected CustomToolCall to be injected");
        let output_pos = attempt_input
            .iter()
            .position(|item| matches!(item, ResponseItem::CustomToolCallOutput { call_id, .. } if call_id == "c1"))
            .expect("expected CustomToolCallOutput to be injected");
        assert!(call_pos < output_pos, "tool call should precede output");
    }

    #[test]
    fn missing_tool_outputs_inserts_function_call_output_for_function_call() {
        let items = vec![ResponseItem::FunctionCall {
            id: None,
            name: "shell".to_string(),
            arguments: "{}".to_string(),
            call_id: "f1".to_string(),
        }];

        let missing = missing_tool_outputs_to_insert(&items);
        assert_eq!(missing.len(), 1);

        let mut input = items;
        for (idx, output_item) in missing.into_iter().rev() {
            input.insert(idx + 1, output_item);
        }

        assert!(matches!(
            input.get(1),
            Some(ResponseItem::FunctionCallOutput { call_id, output })
                if call_id == "f1" && matches!(&output.body, code_protocol::models::FunctionCallOutputBody::Text(text) if text == "aborted")
        ));
    }

    #[test]
    fn missing_tool_outputs_inserts_function_call_output_for_local_shell_legacy_id() {
        let items = vec![legacy_local_shell_call("sh_1")];
        let missing = missing_tool_outputs_to_insert(&items);
        assert_eq!(missing.len(), 1);

        let mut input = items;
        for (idx, output_item) in missing.into_iter().rev() {
            input.insert(idx + 1, output_item);
        }

        assert!(matches!(
            input.get(1),
            Some(ResponseItem::FunctionCallOutput { call_id, output })
                if call_id == "sh_1" && matches!(&output.body, code_protocol::models::FunctionCallOutputBody::Text(text) if text == "aborted")
        ));
    }

    #[test]
    fn missing_tool_outputs_inserts_custom_tool_call_output_for_custom_tool_call() {
        let items = vec![ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "c1".to_string(),
            name: "apply_patch".to_string(),
            input: "noop".to_string(),
        }];

        let missing = missing_tool_outputs_to_insert(&items);
        assert_eq!(missing.len(), 1);

        let mut input = items;
        for (idx, output_item) in missing.into_iter().rev() {
            input.insert(idx + 1, output_item);
        }

        assert!(matches!(
            input.get(1),
            Some(ResponseItem::CustomToolCallOutput { call_id, output })
                if call_id == "c1" && output == "aborted"
        ));
    }

    #[test]
    fn missing_tool_outputs_noops_when_outputs_exist() {
        let items = vec![
            ResponseItem::FunctionCall {
                id: None,
                name: "shell".to_string(),
                arguments: "{}".to_string(),
                call_id: "f1".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "f1".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
        ];

        let missing = missing_tool_outputs_to_insert(&items);
        assert!(missing.is_empty());
    }
}

pub(super) fn missing_tool_outputs_to_insert(items: &[ResponseItem]) -> Vec<(usize, ResponseItem)> {
    let mut function_outputs: HashSet<String> = HashSet::new();
    let mut custom_outputs: HashSet<String> = HashSet::new();

    for item in items {
        match item {
            ResponseItem::FunctionCallOutput { call_id, .. } => {
                function_outputs.insert(call_id.clone());
            }
            ResponseItem::CustomToolCallOutput { call_id, .. } => {
                custom_outputs.insert(call_id.clone());
            }
            _ => {}
        }
    }

    let mut missing_outputs_to_insert: Vec<(usize, ResponseItem)> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        match item {
            ResponseItem::FunctionCall { call_id, .. } => {
                if function_outputs.insert(call_id.clone()) {
                    missing_outputs_to_insert.push((
                        idx,
                        ResponseItem::FunctionCallOutput {
                            call_id: call_id.clone(),
                            output: FunctionCallOutputPayload::from_text("aborted".to_string()),
                        },
                    ));
                }
            }
            ResponseItem::CustomToolCall { call_id, .. } => {
                if custom_outputs.insert(call_id.clone()) {
                    missing_outputs_to_insert.push((
                        idx,
                        ResponseItem::CustomToolCallOutput {
                            call_id: call_id.clone(),
                            output: "aborted".to_string(),
                        },
                    ));
                }
            }
            ResponseItem::LocalShellCall { call_id, id, .. } => {
                let Some(effective_call_id) = call_id.as_ref().or(id.as_ref()) else {
                    continue;
                };

                if function_outputs.insert(effective_call_id.clone()) {
                    missing_outputs_to_insert.push((
                        idx,
                        ResponseItem::FunctionCallOutput {
                            call_id: effective_call_id.clone(),
                            output: FunctionCallOutputPayload::from_text("aborted".to_string()),
                        },
                    ));
                }
            }
            _ => {}
        }
    }

    missing_outputs_to_insert
}

