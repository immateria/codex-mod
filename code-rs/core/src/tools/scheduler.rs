use crate::codex::Session;
use crate::turn_diff_tracker::TurnDiffTracker;
use code_protocol::models::ResponseInputItem;
use code_protocol::models::ResponseItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolCallParallelism {
    Parallel,
    Exclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingToolCall {
    pub(crate) output_pos: usize,
    pub(crate) seq_hint: Option<u64>,
    pub(crate) output_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ToolCallBatch {
    Parallel(Vec<PendingToolCall>),
    Exclusive(PendingToolCall),
}

fn build_tool_call_batches(
    calls: impl IntoIterator<Item = (ToolCallParallelism, PendingToolCall)>,
) -> Vec<ToolCallBatch> {
    let mut batches: Vec<ToolCallBatch> = Vec::new();
    let mut pending_parallel: Vec<PendingToolCall> = Vec::new();

    for (parallelism, call) in calls {
        match parallelism {
            ToolCallParallelism::Parallel => pending_parallel.push(call),
            ToolCallParallelism::Exclusive => {
                if !pending_parallel.is_empty() {
                    batches.push(ToolCallBatch::Parallel(std::mem::take(
                        &mut pending_parallel,
                    )));
                }
                batches.push(ToolCallBatch::Exclusive(call));
            }
        }
    }

    if !pending_parallel.is_empty() {
        batches.push(ToolCallBatch::Parallel(pending_parallel));
    }

    batches
}

fn classify_tool_call_parallelism(
    sess: &Session,
    item: &ResponseItem,
) -> ToolCallParallelism {
    let ResponseItem::FunctionCall { name, .. } = item else {
        return ToolCallParallelism::Exclusive;
    };
    let tool_name = name.as_str();

    // Dynamic and MCP tool calls are always Exclusive for now (they can mutate
    // session-level state and rely on strict ordering / user interaction).
    if sess.is_dynamic_tool(tool_name) {
        return ToolCallParallelism::Exclusive;
    }
    if sess
        .mcp_connection_manager()
        .parse_tool_name(tool_name)
        .is_some()
    {
        return ToolCallParallelism::Exclusive;
    }

    if crate::tools::router::ToolRouter::global().is_parallel_safe_function_tool(tool_name) {
        ToolCallParallelism::Parallel
    } else {
        ToolCallParallelism::Exclusive
    }
}

pub(crate) async fn dispatch_pending_tool_calls<'a, F>(
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: &str,
    attempt_req: u64,
    pending_calls: &[PendingToolCall],
    item_for_pos: F,
) -> Vec<(usize, Option<ResponseInputItem>)>
where
    F: Fn(usize) -> Option<&'a ResponseItem> + Copy,
{
    let router = crate::tools::router::ToolRouter::global();

    let calls_with_parallelism = pending_calls.iter().filter_map(|call| {
        let item = item_for_pos(call.output_pos)?;
        let mut parallelism = classify_tool_call_parallelism(sess, item);

        // If the provider did not supply any ordering metadata, avoid parallel execution
        // so tool-call begin/end events remain deterministic for the TUI.
        if parallelism == ToolCallParallelism::Parallel
            && call.seq_hint.is_none()
            && call.output_index.is_none()
        {
            parallelism = ToolCallParallelism::Exclusive;
        }

        Some((parallelism, *call))
    });

    let mut results: Vec<(usize, Option<ResponseInputItem>)> = Vec::new();
    for batch in build_tool_call_batches(calls_with_parallelism) {
        match batch {
            ToolCallBatch::Parallel(calls) => {
                let mut futures = Vec::with_capacity(calls.len());
                for call in calls {
                    let Some(item) = item_for_pos(call.output_pos).cloned() else {
                        continue;
                    };
                    futures.push(async move {
                        let mut scratch = TurnDiffTracker::new();
                        let resp = router
                            .dispatch_response_item(
                                sess,
                                &mut scratch,
                                crate::tools::router::ToolDispatchMeta::new(
                                    sub_id,
                                    call.seq_hint,
                                    call.output_index,
                                    attempt_req,
                                ),
                                item,
                            )
                            .await;
                        (call.output_pos, resp)
                    });
                }

                let mut batch_results = futures::future::join_all(futures).await;
                batch_results.sort_by_key(|(pos, _)| *pos);
                results.extend(batch_results);
            }
            ToolCallBatch::Exclusive(call) => {
                let Some(item) = item_for_pos(call.output_pos).cloned() else {
                    continue;
                };
                let resp = router
                    .dispatch_response_item(
                        sess,
                        turn_diff_tracker,
                        crate::tools::router::ToolDispatchMeta::new(
                            sub_id,
                            call.seq_hint,
                            call.output_index,
                            attempt_req,
                        ),
                        item,
                    )
                    .await;
                results.push((call.output_pos, resp));
            }
        }
    }

    results
}

#[cfg(test)]
mod tool_call_batch_tests {
    use super::build_tool_call_batches;
    use super::PendingToolCall;
    use super::ToolCallBatch;
    use super::ToolCallParallelism;

    #[test]
    fn groups_parallel_calls_and_preserves_order() {
        let calls = vec![
            (
                ToolCallParallelism::Parallel,
                PendingToolCall {
                    output_pos: 0,
                    seq_hint: None,
                    output_index: None,
                },
            ),
            (
                ToolCallParallelism::Parallel,
                PendingToolCall {
                    output_pos: 1,
                    seq_hint: Some(10),
                    output_index: Some(2),
                },
            ),
            (
                ToolCallParallelism::Exclusive,
                PendingToolCall {
                    output_pos: 2,
                    seq_hint: Some(11),
                    output_index: Some(3),
                },
            ),
            (
                ToolCallParallelism::Parallel,
                PendingToolCall {
                    output_pos: 3,
                    seq_hint: Some(12),
                    output_index: Some(4),
                },
            ),
            (
                ToolCallParallelism::Exclusive,
                PendingToolCall {
                    output_pos: 4,
                    seq_hint: None,
                    output_index: None,
                },
            ),
            (
                ToolCallParallelism::Parallel,
                PendingToolCall {
                    output_pos: 5,
                    seq_hint: Some(13),
                    output_index: None,
                },
            ),
            (
                ToolCallParallelism::Parallel,
                PendingToolCall {
                    output_pos: 6,
                    seq_hint: None,
                    output_index: Some(5),
                },
            ),
        ];

        let batches = build_tool_call_batches(calls);
        assert_eq!(
            batches,
            vec![
                ToolCallBatch::Parallel(vec![
                    PendingToolCall {
                        output_pos: 0,
                        seq_hint: None,
                        output_index: None,
                    },
                    PendingToolCall {
                        output_pos: 1,
                        seq_hint: Some(10),
                        output_index: Some(2),
                    },
                ]),
                ToolCallBatch::Exclusive(PendingToolCall {
                    output_pos: 2,
                    seq_hint: Some(11),
                    output_index: Some(3),
                }),
                ToolCallBatch::Parallel(vec![PendingToolCall {
                    output_pos: 3,
                    seq_hint: Some(12),
                    output_index: Some(4),
                }]),
                ToolCallBatch::Exclusive(PendingToolCall {
                    output_pos: 4,
                    seq_hint: None,
                    output_index: None,
                }),
                ToolCallBatch::Parallel(vec![
                    PendingToolCall {
                        output_pos: 5,
                        seq_hint: Some(13),
                        output_index: None,
                    },
                    PendingToolCall {
                        output_pos: 6,
                        seq_hint: None,
                        output_index: Some(5),
                    },
                ]),
            ]
        );
    }
}

