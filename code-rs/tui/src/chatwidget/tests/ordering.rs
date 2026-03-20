    #[test]
    fn ordering_stream_delta_should_follow_existing_background_tail() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    chat.last_seen_request_index = 1;
    chat.push_background_tail("background".to_string());
    
    let stream_state = AssistantStreamState {
        id: HistoryId::ZERO,
        stream_id: "stream-1".into(),
        preview_markdown: "partial".into(),
        deltas: vec![AssistantStreamDelta {
            delta: "partial".into(),
            sequence: Some(0),
            received_at: SystemTime::now(),
        }],
        citations: vec![],
        metadata: None,
        in_progress: true,
        last_updated_at: SystemTime::now(),
        truncated_prefix_bytes: 0,
    };
    let stream_cell = history_cell::new_streaming_content(stream_state, &chat.config);
    
    chat.history_insert_with_key_global_tagged(
        Box::new(stream_cell),
        OrderKey {
            req: 1,
            out: 0,
            seq: 0,
        },
        "stream",
        None,
    );
    
    let kinds: Vec<HistoryCellType> = chat
        .history_cells
        .iter()
        .map(super::super::history_cell::HistoryCell::kind)
        .collect();
    
    assert_eq!(
        kinds,
        vec![HistoryCellType::BackgroundEvent, HistoryCellType::Assistant],
        "streaming assistant output should append after the existing background tail cell",
    );
    }
    
    #[test]
    fn ordering_tool_reasoning_explore_should_preserve_arrival_sequence() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    chat.last_seen_request_index = 1;
    
    let make_plain = |text: &str| PlainMessageState {
        id: HistoryId::ZERO,
        role: PlainMessageRole::System,
        kind: PlainMessageKind::Plain,
        header: None,
        lines: vec![MessageLine {
            kind: MessageLineKind::Paragraph,
            spans: vec![InlineSpan {
                text: text.to_string(),
                tone: TextTone::Default,
                emphasis: TextEmphasis::default(),
                entity: None,
            }],
        }],
        metadata: None,
    };
    
    // Reasoning arrives first with later output index.
    let reasoning_key = ChatWidget::raw_order_key_from_order_meta(&OrderMeta {
        request_ordinal: 1,
        output_index: Some(2),
        sequence_number: Some(0),
    });
    chat.history_insert_plain_state_with_key(make_plain("reasoning"), reasoning_key, "reasoning");
    
    // Explore summary follows immediately afterwards.
    let explore_key = ChatWidget::raw_order_key_from_order_meta(&OrderMeta {
        request_ordinal: 1,
        output_index: Some(3),
        sequence_number: Some(0),
    });
    chat.history_insert_plain_state_with_key(make_plain("explore"), explore_key, "explore");
    
    // Tool run summary arrives last but references an earlier output index.
    let tool_key = ChatWidget::raw_order_key_from_order_meta(&OrderMeta {
        request_ordinal: 1,
        output_index: Some(1),
        sequence_number: Some(0),
    });
    chat.history_insert_plain_state_with_key(make_plain("tool"), tool_key, "tool");
    
    let labels: Vec<String> = chat
        .history_cells
        .iter()
        .map(|cell| {
            cell.display_lines_trimmed()
                .first()
                .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect())
                .unwrap_or_default()
        })
        .collect();
    
    assert_eq!(
        labels,
        vec!["reasoning".to_string(), "explore".to_string(), "tool".to_string()],
        "later inserts with smaller output_index should not leapfrog visible reasoning/explore summaries",
    );
    }
    
    #[test]
    fn ordering_cross_request_pre_prompt_should_not_prepend_previous_turn() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    let make_plain = |text: &str| PlainMessageState {
        id: HistoryId::ZERO,
        role: PlainMessageRole::System,
        kind: PlainMessageKind::Plain,
        header: None,
        lines: vec![MessageLine {
            kind: MessageLineKind::Paragraph,
            spans: vec![InlineSpan {
                text: text.to_string(),
                tone: TextTone::Default,
                emphasis: TextEmphasis::default(),
                entity: None,
            }],
        }],
        metadata: None,
    };
    
    chat.history_insert_plain_state_with_key(
        make_plain("req1"),
        OrderKey {
            req: 1,
            out: 0,
            seq: 0,
        },
        "req1",
    );
    
    chat.last_seen_request_index = 1;
    chat.pending_user_prompts_for_next_turn = 0;
    
    let key = chat.system_order_key(SystemPlacement::PrePrompt, None);
    chat.history_insert_plain_state_with_key(make_plain("system"), key, "system");
    
    let labels: Vec<String> = chat
        .history_cells
        .iter()
        .map(|cell| {
            cell.display_lines_trimmed()
                .first()
                .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect())
                .unwrap_or_default()
        })
        .collect();
    
    assert_eq!(
        labels,
        vec!["req1".to_string(), "system".to_string()],
        "pre-prompt system notices for a new request should append after the prior turn rather than prepending it",
    );
    }
    
    #[test]
    fn resume_ordering_offsets_provider_ordinals() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    let make_plain = |id: u64,
                       text: &str,
                       role: PlainMessageRole,
                       kind: PlainMessageKind| -> PlainMessageState {
        PlainMessageState {
            id: HistoryId(id),
            role,
            kind,
            header: None,
            lines: vec![MessageLine {
                kind: MessageLineKind::Paragraph,
                spans: vec![InlineSpan {
                    text: text.to_string(),
                    tone: TextTone::Default,
                    emphasis: TextEmphasis::default(),
                    entity: None,
                }],
            }],
            metadata: None,
        }
    };
    
    let snapshot = HistorySnapshot {
        records: vec![
            HistoryRecord::PlainMessage(make_plain(
                1,
                "user-turn",
                PlainMessageRole::User,
                PlainMessageKind::User,
            )),
            HistoryRecord::PlainMessage(make_plain(
                2,
                "assistant-turn",
                PlainMessageRole::Assistant,
                PlainMessageKind::Assistant,
            )),
        ],
        next_id: 3,
        exec_call_lookup: HashMap::new(),
        tool_call_lookup: HashMap::new(),
        stream_lookup: HashMap::new(),
        order: vec![
            OrderKeySnapshot {
                req: 5,
                out: 0,
                seq: 0,
            },
            OrderKeySnapshot {
                req: 5,
                out: 1,
                seq: 0,
            },
        ],
        order_debug: Vec::new(),
    };
    
    chat.restore_history_snapshot(&snapshot);
    
    assert_eq!(
        chat.last_seen_request_index, 5,
        "restoring snapshot should set last_seen_request_index"
    );
    
    let order_meta = OrderMeta {
        request_ordinal: 0,
        output_index: Some(0),
        sequence_number: Some(0),
    };
    let key = chat.provider_order_key_from_order_meta(&order_meta);
    assert_eq!(
        key.req, 6,
        "resume should bias provider ordinals so new output slots after restored history"
    );
    
    let new_state = PlainMessageState {
        id: HistoryId::ZERO,
        role: PlainMessageRole::Assistant,
        kind: PlainMessageKind::Assistant,
        header: None,
        lines: vec![MessageLine {
            kind: MessageLineKind::Paragraph,
            spans: vec![InlineSpan {
                text: "new-assistant".to_string(),
                tone: TextTone::Default,
                emphasis: TextEmphasis::default(),
                entity: None,
            }],
        }],
        metadata: None,
    };
    
    let pos = chat.history_insert_plain_state_with_key(new_state, key, "resume-order");
    assert_eq!(pos, chat.history_cells.len().saturating_sub(1));
    
    let inserted_key = chat.cell_order_seq[pos];
    assert_eq!(inserted_key.req, 6);
    
    let inserted_text: String = chat.history_cells[pos]
        .display_lines_trimmed()
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect();
    assert!(
        inserted_text.contains("new-assistant"),
        "resume insertion should surface the new assistant answer at the tail"
    );
    }
