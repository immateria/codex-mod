    #[test]
    fn exec_child_gutter_click_jumps_to_parent_js_repl_cell() {
        let _guard = enter_test_runtime_guard();
        let mut harness = ChatWidgetHarness::new();

        // Seed a parent JS REPL begin, then enough filler content to force scroll,
        // then a child exec begin that references the parent via parent_call_id.
        let parent_call_id = "js-parent".to_string();
        harness.handle_event(Event {
            id: "js-begin".to_string(),
            event_seq: 0,
            msg: EventMsg::JsReplExecBegin(code_core::protocol::JsReplExecBeginEvent {
                call_id: parent_call_id.clone(),
                code: "console.log('hi')".to_string(),
                runtime_kind: "node".to_string(),
                runtime_version: "20.11.0".to_string(),
                cwd: std::env::temp_dir(),
                timeout_ms: 15_000,
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(1),
            }),
        });

        for i in 0..30 {
            harness.push_user_prompt(format!("filler {i}"));
        }

        harness.handle_event(Event {
            id: "exec-begin".to_string(),
            event_seq: 0,
            msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: "sh-child".to_string(),
                command: vec!["bash".into(), "-lc".into(), "echo child".into()],
                cwd: std::env::temp_dir(),
                parsed_cmd: Vec::new(),
                parent_call_id: Some(parent_call_id.clone()),
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(2),
            }),
        });

        // Render once to populate clickable regions. Use a short viewport so we can
        // verify jumping changes the rendered content.
        {
            use crate::test_backend::VT100Backend;
            use ratatui::Terminal;

            let chat = harness.chat();
            let mut terminal = Terminal::new(VT100Backend::new(80, 10)).expect("terminal");
            terminal
                .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
                .expect("draw");
        }

        let (x, y) = harness.with_chat(|chat| {
            let regions = chat.clickable_regions.borrow();
            let region = regions
                .iter()
                .find(|region| {
                    matches!(
                        &region.action,
                        ClickableAction::JumpToCallId(call_id) if call_id == &parent_call_id
                    )
                })
                .expect("expected a history gutter click region for js parent");
            let x = region.rect.x.saturating_add(region.rect.width.saturating_div(2));
            (x, region.rect.y)
        });

        harness.with_chat(|chat| chat.handle_click((x, y)));

        let output = {
            use crate::test_backend::VT100Backend;
            use ratatui::Terminal;

            let chat = harness.chat();
            let mut terminal = Terminal::new(VT100Backend::new(80, 10)).expect("terminal");
            terminal
                .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
                .expect("draw");
            terminal.backend().to_string()
        };

        assert!(
            output.contains("js node 20.11.0"),
            "expected to jump to the parent JS cell, got:\n{output}",
        );
    }

    #[test]
    fn js_repl_spawned_child_keyboard_jump() {
        let _guard = enter_test_runtime_guard();
        let mut harness = ChatWidgetHarness::new();

        let parent_call_id = "js-parent".to_string();
        harness.handle_event(Event {
            id: "js-begin".to_string(),
            event_seq: 0,
            msg: EventMsg::JsReplExecBegin(code_core::protocol::JsReplExecBeginEvent {
                call_id: parent_call_id.clone(),
                code: "console.log('hi')".to_string(),
                runtime_kind: "node".to_string(),
                runtime_version: "20.11.0".to_string(),
                cwd: std::env::temp_dir(),
                timeout_ms: 15_000,
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(1),
            }),
        });

        for i in 0..30 {
            harness.push_user_prompt(format!("filler {i}"));
        }

        harness.handle_event(Event {
            id: "exec-begin".to_string(),
            event_seq: 0,
            msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: "sh-child".to_string(),
                command: vec!["bash".into(), "-lc".into(), "echo child".into()],
                cwd: std::env::temp_dir(),
                parsed_cmd: Vec::new(),
                parent_call_id: Some(parent_call_id),
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(2),
            }),
        });

        // Render once so prefix sums / max scroll are computed before we jump.
        {
            use crate::test_backend::VT100Backend;
            use ratatui::Terminal;

            let chat = harness.chat();
            let mut terminal = Terminal::new(VT100Backend::new(80, 10)).expect("terminal");
            terminal
                .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
                .expect("draw");
        }

        harness.with_chat(|chat| {
            use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
            chat.handle_key_event(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        });

        let output_parent = {
            use crate::test_backend::VT100Backend;
            use ratatui::Terminal;

            let chat = harness.chat();
            let mut terminal = Terminal::new(VT100Backend::new(80, 10)).expect("terminal");
            terminal
                .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
                .expect("draw");
            terminal.backend().to_string()
        };

        assert!(
            output_parent.contains("js node 20.11.0"),
            "expected to jump to the parent JS cell, got:\n{output_parent}",
        );

        harness.with_chat(|chat| {
            use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
            // Many terminals report `}` with the SHIFT modifier (since it's often typed as Shift+]).
            chat.handle_key_event(KeyEvent::new(KeyCode::Char('}'), KeyModifiers::SHIFT));
        });

        let output_child = {
            use crate::test_backend::VT100Backend;
            use ratatui::Terminal;

            let chat = harness.chat();
            let mut terminal = Terminal::new(VT100Backend::new(80, 10)).expect("terminal");
            terminal
                .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
                .expect("draw");
            terminal.backend().to_string()
        };

        assert!(
            output_child.contains("echo child"),
            "expected to jump to the child exec cell, got:\n{output_child}",
        );
    }

    #[test]
    fn history_exec_output_fold_targets_visible_exec_cell_when_scrolled() {
        let _guard = enter_test_runtime_guard();
        let mut harness = ChatWidgetHarness::new();
        let cwd = std::env::temp_dir();

        harness.with_chat(reset_history);

        let mk_stdout = |prefix: &str| -> String {
            let mut out = String::new();
            for i in 0..50 {
                out.push_str(&format!("{prefix}-{i}\n"));
            }
            out
        };

        harness.handle_event(Event {
            id: "exec-a-begin".to_string(),
            event_seq: 0,
            msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: "exec-a".to_string(),
                command: vec!["bash".into(), "-lc".into(), "echo a".into()],
                cwd: cwd.clone(),
                parsed_cmd: Vec::new(),
                parent_call_id: None,
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(1),
            }),
        });
        harness.handle_event(Event {
            id: "exec-a-end".to_string(),
            event_seq: 1,
            msg: EventMsg::ExecCommandEnd(code_core::protocol::ExecCommandEndEvent {
                call_id: "exec-a".to_string(),
                stdout: mk_stdout("a"),
                stderr: String::new(),
                exit_code: 0,
                duration: std::time::Duration::from_millis(10),
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(2),
            }),
        });

        for i in 0..30 {
            harness.push_user_prompt(format!("filler {i}"));
        }

        harness.handle_event(Event {
            id: "exec-b-begin".to_string(),
            event_seq: 0,
            msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: "exec-b".to_string(),
                command: vec!["bash".into(), "-lc".into(), "echo b".into()],
                cwd,
                parsed_cmd: Vec::new(),
                parent_call_id: None,
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(3),
            }),
        });
        harness.handle_event(Event {
            id: "exec-b-end".to_string(),
            event_seq: 1,
            msg: EventMsg::ExecCommandEnd(code_core::protocol::ExecCommandEndEvent {
                call_id: "exec-b".to_string(),
                stdout: mk_stdout("b"),
                stderr: String::new(),
                exit_code: 0,
                duration: std::time::Duration::from_millis(10),
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(4),
            }),
        });

        // Render once so prefix sums / scroll bounds are computed before we scroll.
        {
            use crate::test_backend::VT100Backend;
            use ratatui::Terminal;

            let chat = harness.chat();
            let mut terminal = Terminal::new(VT100Backend::new(80, 10)).expect("terminal");
            terminal
                .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
                .expect("draw");
        }

        let (a_before, b_before) = harness.with_chat(|chat| {
            let a = chat
                .history_cells
                .iter()
                .find_map(|cell| (cell.call_id() == Some("exec-a")).then_some(cell.desired_height(80)));
            let b = chat
                .history_cells
                .iter()
                .find_map(|cell| (cell.call_id() == Some("exec-b")).then_some(cell.desired_height(80)));
            (a.expect("expected exec-a cell"), b.expect("expected exec-b cell"))
        });

        // Scroll to top so exec-b is offscreen, then fold output. The shortcut
        // should operate on the bottom-most visible exec cell (exec-a), not the
        // newest exec cell in history.
        harness.with_chat(|chat| {
            use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
            chat.handle_key_event(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
            chat.handle_key_event(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
        });

        let (a_after, b_after) = harness.with_chat(|chat| {
            let a = chat
                .history_cells
                .iter()
                .find_map(|cell| (cell.call_id() == Some("exec-a")).then_some(cell.desired_height(80)));
            let b = chat
                .history_cells
                .iter()
                .find_map(|cell| (cell.call_id() == Some("exec-b")).then_some(cell.desired_height(80)));
            (a.expect("expected exec-a cell"), b.expect("expected exec-b cell"))
        });

        assert_ne!(
            a_after, a_before,
            "expected exec-a output fold to toggle when scrolled to top"
        );
        assert_eq!(
            b_after, b_before,
            "expected exec-b output fold to remain unchanged when scrolled to top"
        );
    }

    #[test]
    fn history_exec_output_fold_toggles_tool_details_on_latest_tool_cell() {
        let _guard = enter_test_runtime_guard();
        let mut harness = ChatWidgetHarness::new();

        harness.with_chat(reset_history);

        let idx = harness.with_chat(|chat| {
            use crate::history_cell::ToolCallCell;
            use code_core::history::state::{
                ArgumentValue,
                HistoryId,
                ToolArgument,
                ToolCallState,
                ToolResultPreview,
                ToolStatus,
            };

            let state = ToolCallState {
                id: HistoryId::ZERO,
                call_id: Some("tool-1".to_string()),
                status: ToolStatus::Success,
                title: "Tool...".to_string(),
                duration: None,
                arguments: vec![
                    ToolArgument {
                        name: "invocation".to_string(),
                        value: ArgumentValue::Text("some_tool({\"x\":1})".to_string()),
                    },
                    ToolArgument {
                        name: "path".to_string(),
                        value: ArgumentValue::Text("/tmp/example.txt".to_string()),
                    },
                    ToolArgument {
                        name: "mode".to_string(),
                        value: ArgumentValue::Text("fast".to_string()),
                    },
                    ToolArgument {
                        name: "flag".to_string(),
                        value: ArgumentValue::Text("true".to_string()),
                    },
                ],
                result_preview: Some(ToolResultPreview {
                    lines: (0..10).map(|n| format!("line {n}")).collect(),
                    truncated: false,
                }),
                error_message: None,
            };

            let cell = ToolCallCell::new(state);
            let key = chat.next_req_key_top();
            chat.history_insert_with_key_global(Box::new(cell), key)
        });

        let (before_height, before_collapsed) = harness.with_chat(|chat| {
            let cell = chat.history_cells[idx]
                .as_any()
                .downcast_ref::<crate::history_cell::ToolCallCell>()
                .expect("expected tool cell");
            (cell.desired_height(80), cell.details_collapsed())
        });
        assert!(
            before_collapsed,
            "expected successful tool cells to start with collapsed details"
        );

        harness.with_chat(|chat| {
            use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
            chat.handle_key_event(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
        });

        let (after_height, after_collapsed) = harness.with_chat(|chat| {
            let cell = chat.history_cells[idx]
                .as_any()
                .downcast_ref::<crate::history_cell::ToolCallCell>()
                .expect("expected tool cell");
            (cell.desired_height(80), cell.details_collapsed())
        });

        assert_ne!(
            after_height, before_height,
            "expected fold hotkey to toggle tool details"
        );
        assert_ne!(
            after_collapsed, before_collapsed,
            "expected tool details collapsed state to toggle"
        );
    }

    #[test]
    fn history_js_repl_code_fold_targets_visible_js_cell_when_scrolled() {
        let _guard = enter_test_runtime_guard();
        let mut harness = ChatWidgetHarness::new();
        let cwd = std::env::temp_dir();

        harness.with_chat(reset_history);

        harness.handle_event(Event {
            id: "js-a-begin".to_string(),
            event_seq: 0,
            msg: EventMsg::JsReplExecBegin(code_core::protocol::JsReplExecBeginEvent {
                call_id: "js-a".to_string(),
                code: "console.log('a')".to_string(),
                runtime_kind: "node".to_string(),
                runtime_version: "20.11.0".to_string(),
                cwd: cwd.clone(),
                timeout_ms: 15_000,
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(1),
            }),
        });

        for i in 0..30 {
            harness.push_user_prompt(format!("filler {i}"));
        }

        harness.handle_event(Event {
            id: "js-b-begin".to_string(),
            event_seq: 0,
            msg: EventMsg::JsReplExecBegin(code_core::protocol::JsReplExecBeginEvent {
                call_id: "js-b".to_string(),
                code: "console.log('b')".to_string(),
                runtime_kind: "node".to_string(),
                runtime_version: "20.11.0".to_string(),
                cwd,
                timeout_ms: 15_000,
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(2),
            }),
        });

        // Render once so prefix sums / scroll bounds are computed before we scroll.
        {
            use crate::test_backend::VT100Backend;
            use ratatui::Terminal;

            let chat = harness.chat();
            let mut terminal = Terminal::new(VT100Backend::new(80, 10)).expect("terminal");
            terminal
                .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
                .expect("draw");
        }

        let (a_before, b_before) = harness.with_chat(|chat| {
            use crate::history_cell::JsReplCell;

            let a = chat.history_cells.iter().find_map(|cell| {
                cell.as_any().downcast_ref::<JsReplCell>().and_then(|js| {
                    (js.call_id() == Some("js-a")).then_some(js.code_collapsed.get())
                })
            });
            let b = chat.history_cells.iter().find_map(|cell| {
                cell.as_any().downcast_ref::<JsReplCell>().and_then(|js| {
                    (js.call_id() == Some("js-b")).then_some(js.code_collapsed.get())
                })
            });
            (a.expect("expected js-a cell"), b.expect("expected js-b cell"))
        });

        harness.with_chat(|chat| {
            use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
            chat.handle_key_event(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
            chat.handle_key_event(KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE));
        });

        let (a_after, b_after) = harness.with_chat(|chat| {
            use crate::history_cell::JsReplCell;

            let a = chat.history_cells.iter().find_map(|cell| {
                cell.as_any().downcast_ref::<JsReplCell>().and_then(|js| {
                    (js.call_id() == Some("js-a")).then_some(js.code_collapsed.get())
                })
            });
            let b = chat.history_cells.iter().find_map(|cell| {
                cell.as_any().downcast_ref::<JsReplCell>().and_then(|js| {
                    (js.call_id() == Some("js-b")).then_some(js.code_collapsed.get())
                })
            });
            (a.expect("expected js-a cell"), b.expect("expected js-b cell"))
        });

        assert_ne!(
            a_after, a_before,
            "expected js-a code fold to toggle when scrolled to top"
        );
        assert_eq!(
            b_after, b_before,
            "expected js-b code fold to remain unchanged when scrolled to top"
        );
    }

    #[cfg(feature = "managed-network-proxy")]
    #[test]
    fn statusline_network_segment_click_on_bottom_opens_network_settings() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        use code_core::config_types::StatusLineLane;
        use crate::bottom_pane::settings_pages::status_line::StatusLineItem;

        chat.setup_status_line(
            Vec::new(),
            vec![StatusLineItem::NetworkMediation],
            StatusLineLane::Top,
        );
    });

    // Render once to populate clickable regions (bottom statusline adds regions).
    {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
    }

    let (x, y) = harness.with_chat(|chat| {
        let regions = chat.clickable_regions.borrow();
        let region = regions
            .iter()
            .filter(|region| region.action == ClickableAction::ShowNetworkSettings)
            .max_by_key(|region| region.rect.y)
            .expect("expected a bottom statusline region for network settings");
        let x = region.rect.x.saturating_add(region.rect.width.saturating_div(2));
        (x, region.rect.y)
    });

    harness.with_chat(|chat| chat.handle_click((x, y)));

    let output = {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
        terminal.backend().to_string()
    };

    assert!(
        output.contains("Coverage: exec, exec_command, web_fetch"),
        "expected Network settings view after click, got:\n{output}",
    );
    }
    
    
    
