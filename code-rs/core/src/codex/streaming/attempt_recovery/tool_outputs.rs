use super::*;

use std::collections::HashSet;

pub(in crate::codex::streaming) fn reconcile_pending_tool_outputs(
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
        ResponseItem::FunctionCall { call_id: existing, .. } if existing == call_id => {
            Some(item.clone())
        }
        ResponseItem::LocalShellCall { call_id: call_id_field, id, .. } => {
            let effective = call_id_field.as_deref().or(id.as_deref());
            if effective == Some(call_id) {
                Some(item.clone())
            } else {
                None
            }
        }
        ResponseItem::CustomToolCall { call_id: existing, .. } if existing == call_id => {
            Some(item.clone())
        }
        _ => None,
    })
}

pub(in crate::codex::streaming) fn missing_tool_outputs_to_insert(
    items: &[ResponseItem],
) -> Vec<(usize, ResponseItem)> {
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
            .position(|item| {
                matches!(item, ResponseItem::CustomToolCall { call_id, .. } if call_id == "c1")
            })
            .expect("expected CustomToolCall to be injected");
        let output_pos = attempt_input
            .iter()
            .position(|item| {
                matches!(item, ResponseItem::CustomToolCallOutput { call_id, .. } if call_id == "c1")
            })
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
