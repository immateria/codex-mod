    fn plain_state_text(state: &PlainMessageState) -> String {
    state
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .collect::<Vec<_>>()
        .join("\n")
    }
    
    #[test]
    fn hook_started_and_completed_events_render_as_plain_history_cells() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    let started_run = code_protocol::protocol::HookRunSummary {
        id: "hook-1".to_string(),
        event_name: code_protocol::protocol::HookEventName::SessionStart,
        handler_type: code_protocol::protocol::HookHandlerType::Command,
        execution_mode: code_protocol::protocol::HookExecutionMode::Sync,
        scope: code_protocol::protocol::HookScope::Thread,
        source_path: PathBuf::from("/tmp/hooks.json"),
        display_order: 0,
        status: code_protocol::protocol::HookRunStatus::Running,
        status_message: Some("booting".to_string()),
        started_at: 0,
        completed_at: None,
        duration_ms: None,
        entries: Vec::new(),
    };
    let completed_run = code_protocol::protocol::HookRunSummary {
        status: code_protocol::protocol::HookRunStatus::Completed,
        status_message: Some("done".to_string()),
        completed_at: Some(0),
        duration_ms: Some(0),
        entries: vec![code_protocol::protocol::HookOutputEntry {
            kind: code_protocol::protocol::HookOutputEntryKind::Context,
            text: "hello from hook".to_string(),
        }],
        ..started_run.clone()
    };
    
    chat.handle_code_event(Event {
        id: "turn-1".to_string(),
        event_seq: 1,
        msg: EventMsg::HookStarted(code_protocol::protocol::HookStartedEvent {
            turn_id: Some("turn-1".to_string()),
            run: started_run,
        }),
        order: None,
    });
    chat.handle_code_event(Event {
        id: "turn-1".to_string(),
        event_seq: 2,
        msg: EventMsg::HookCompleted(code_protocol::protocol::HookCompletedEvent {
            turn_id: Some("turn-1".to_string()),
            run: completed_run,
        }),
        order: None,
    });
    
    let plain_states: Vec<&PlainMessageState> = chat
        .history_state
        .records
        .iter()
        .filter_map(|record| match record {
            HistoryRecord::PlainMessage(state) => Some(state),
            _ => None,
        })
        .collect();
    
    assert_eq!(plain_states.len(), 2, "expected two plain history records");
    
    let started_text = plain_state_text(plain_states[0]);
    assert!(
        started_text.contains("Running session start hook: booting"),
        "unexpected started hook render:\n{started_text}"
    );
    
    let completed_text = plain_state_text(plain_states[1]);
    assert!(
        completed_text.contains("Hook session start: completed"),
        "unexpected completed hook render:\n{completed_text}"
    );
    assert!(
        completed_text.contains("context: hello from hook"),
        "expected context entry in completed hook render:\n{completed_text}"
    );
    }

