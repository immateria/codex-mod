#![allow(clippy::unwrap_used, clippy::expect_used)]

use code_tui::test_helpers::{
    assistant_layout_builds,
    exec_layout_builds,
    history_layout_cache_misses,
    merged_exec_layout_builds,
    reasoning_layout_builds,
    reset_web_fetch_layout_builds,
    render_chat_widget_to_vt100,
    reset_assistant_layout_builds,
    reset_exec_layout_builds,
    reset_history_layout_cache_stats,
    reset_merged_exec_layout_builds,
    reset_reasoning_layout_builds,
    reset_syntax_highlight_calls,
    syntax_highlight_calls,
    web_fetch_layout_builds,
    ChatWidgetHarness,
};
use code_core::history::state::{
    ExecAction,
    ExecRecord,
    ExecStreamChunk,
    ExecStatus,
    HistoryId,
    InlineSpan,
    MergedExecRecord,
    ReasoningBlock,
    ReasoningSection,
    ReasoningState,
    TextEmphasis,
    TextTone,
};
use code_core::protocol::{Event, EventMsg, ExecCommandBeginEvent, ExecCommandEndEvent, OrderMeta};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn next_order_meta(request_ordinal: u64, seq: &mut u64) -> OrderMeta {
    let order = OrderMeta {
        request_ordinal,
        output_index: Some(0),
        sequence_number: Some(*seq),
    };
    *seq += 1;
    order
}

#[test]
fn scrollback_repeated_renders_do_not_rehighlight_or_relayout_assistant() {
    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    // Enough assistant messages to enable scrollback. Include code blocks so syntax highlighting
    // and code-card rendering are exercised.
    for idx in 0..80 {
        harness.push_assistant_message(format!(
            "Assistant {idx}: some prose.\n\n```rust\nfn main() {{ println!(\"{idx}\"); }}\n```\n"
        ));
    }

    reset_assistant_layout_builds();
    reset_syntax_highlight_calls();

    // Initial draw warms caches.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    let layout_after_first = assistant_layout_builds();
    let highlight_after_first = syntax_highlight_calls();
    assert!(
        layout_after_first > 0,
        "expected assistant layout to be computed on first render"
    );
    assert!(
        highlight_after_first > 0,
        "expected syntax highlighting to run on first render"
    );

    // Re-rendering without state changes must be cache-only.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    assert_eq!(
        assistant_layout_builds(),
        layout_after_first,
        "assistant layout should be cached for stable renders"
    );
    assert_eq!(
        syntax_highlight_calls(),
        highlight_after_first,
        "syntax highlighting should not rerun for stable renders"
    );

    // Scroll, then verify the second render at the same scroll offset doesn't redo work.
    for _ in 0..6 {
        harness.send_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        let layout_once = assistant_layout_builds();
        let highlight_once = syntax_highlight_calls();

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        assert_eq!(
            assistant_layout_builds(),
            layout_once,
            "assistant layout should not be recomputed on repeated renders at a stable scroll offset"
        );
        assert_eq!(
            syntax_highlight_calls(),
            highlight_once,
            "syntax highlighting should not rerun on repeated renders at a stable scroll offset"
        );
    }
}

#[test]
fn width_churn_does_not_rehighlight_assistant() {
    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    harness.push_assistant_message(
        "Resize churn should not re-run syntax highlighting.\n\n```rust\nfn main() { println!(\"hi\"); }\n```\n"
            .to_string(),
    );

    reset_syntax_highlight_calls();

    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    let highlight_after_first = syntax_highlight_calls();
    assert!(
        highlight_after_first > 0,
        "expected syntax highlighting to run on first render"
    );

    for width in [80_u16, 100, 90, 120, 60, 85] {
        let _ = render_chat_widget_to_vt100(&mut harness, width, 30);
    }

    assert_eq!(
        syntax_highlight_calls(),
        highlight_after_first,
        "syntax highlighting should be cached across width churn"
    );
}

#[test]
fn scrollback_repeated_renders_do_not_relayout_exec_cells() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let cwd = PathBuf::from("/tmp");

    // Seed enough completed exec commands that scrollback exists and layout can be cached.
    for idx in 0..80 {
        let call_id = format!("call_{idx}");
        harness.handle_event(Event {
            id: format!("exec-begin-{idx}"),
            event_seq: 0,
            msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id: call_id.clone(),
                command: vec!["echo".into(), idx.to_string()],
                cwd: cwd.clone(),
                parsed_cmd: Vec::new(),
            }),
            order: Some(next_order_meta(1, &mut seq)),
        });
        harness.handle_event(Event {
            id: format!("exec-end-{idx}"),
            event_seq: 1,
            msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id,
                stdout: format!("line {idx}: {pad}\n", pad = "x".repeat(140)),
                stderr: String::new(),
                exit_code: 0,
                duration: Duration::from_millis(50),
            }),
            order: Some(next_order_meta(1, &mut seq)),
        });
    }

    reset_exec_layout_builds();

    // Initial draw warms caches.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    let layout_after_first = exec_layout_builds();
    assert!(
        layout_after_first > 0,
        "expected exec layout to be computed on first render"
    );

    // Re-rendering without state changes must be cache-only.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    assert_eq!(
        exec_layout_builds(),
        layout_after_first,
        "exec layout should be cached for stable renders"
    );

    // Scroll, then verify the second render at the same scroll offset doesn't redo work.
    for _ in 0..6 {
        harness.send_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        let layout_once = exec_layout_builds();

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        assert_eq!(
            exec_layout_builds(),
            layout_once,
            "exec layout should not be recomputed on repeated renders at a stable scroll offset"
        );
    }
}

#[test]
fn scrollback_repeated_renders_do_not_relayout_merged_exec_cells() {
    let mut harness = ChatWidgetHarness::new();

    for idx in 0..80_u64 {
        let mut segments = Vec::with_capacity(3);
        for seg in 0..3_u64 {
            segments.push(ExecRecord {
                id: HistoryId(idx.saturating_mul(10).saturating_add(seg)),
                call_id: Some(format!("merged_{idx}_{seg}")),
                command: vec!["echo".into(), format!("{idx}:{seg}")],
                parsed: Vec::new(),
                action: ExecAction::Run,
                status: ExecStatus::Success,
                stdout_chunks: vec![ExecStreamChunk {
                    offset: 0,
                    content: format!("line {idx}:{seg}: {pad}\n", pad = "x".repeat(140)),
                }],
                stderr_chunks: Vec::new(),
                exit_code: Some(0),
                wait_total: None,
                wait_active: false,
                wait_notes: Vec::new(),
                started_at: SystemTime::UNIX_EPOCH,
                completed_at: Some(SystemTime::UNIX_EPOCH),
                working_dir: None,
                env: Vec::new(),
                tags: Vec::new(),
            });
        }

        harness.push_merged_exec_state(MergedExecRecord {
            id: HistoryId(idx),
            action: ExecAction::Run,
            segments,
        });
    }

    reset_merged_exec_layout_builds();

    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    let layout_after_first = merged_exec_layout_builds();
    assert!(
        layout_after_first > 0,
        "expected merged exec layout to be computed on first render"
    );

    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    assert_eq!(
        merged_exec_layout_builds(),
        layout_after_first,
        "merged exec layout should be cached for stable renders"
    );

    for _ in 0..6 {
        harness.send_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        let layout_once = merged_exec_layout_builds();

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        assert_eq!(
            merged_exec_layout_builds(),
            layout_once,
            "merged exec layout should not be recomputed on repeated renders at a stable scroll offset"
        );
    }
}

#[test]
fn scrollback_repeated_renders_do_not_relayout_reasoning_cells() {
    let mut harness = ChatWidgetHarness::new();

    for idx in 0..80_u64 {
        let spans = vec![InlineSpan {
            text: format!("Reasoning {idx}: {pad}", pad = "z".repeat(220)),
            tone: TextTone::Default,
            emphasis: TextEmphasis::default(),
            entity: None,
        }];
        let section = ReasoningSection {
            heading: Some(format!("Section {idx}")),
            summary: None,
            blocks: vec![ReasoningBlock::Paragraph(spans)],
        };
        let state = ReasoningState {
            id: HistoryId(idx),
            sections: vec![section],
            effort: None,
            in_progress: false,
        };
        harness.push_reasoning_state(state, false);
    }

    reset_reasoning_layout_builds();

    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    let layout_after_first = reasoning_layout_builds();
    assert!(
        layout_after_first > 0,
        "expected reasoning layout to be computed on first render"
    );

    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    assert_eq!(
        reasoning_layout_builds(),
        layout_after_first,
        "reasoning layout should be cached for stable renders"
    );

    for _ in 0..6 {
        harness.send_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        let layout_once = reasoning_layout_builds();

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        assert_eq!(
            reasoning_layout_builds(),
            layout_once,
            "reasoning layout should not be recomputed on repeated renders at a stable scroll offset"
        );
    }
}

#[test]
fn scrollback_repeated_renders_do_not_relayout_web_fetch_cells() {
    let mut harness = ChatWidgetHarness::new();

    for idx in 0..80_u64 {
        let mut result = String::new();
        for line_idx in 0..200_u64 {
            result.push_str(&format!(
                "line {idx}-{line_idx}: {pad}\n",
                pad = "w".repeat(180)
            ));
        }
        harness.push_web_fetch_tool_call(result);
    }

    reset_web_fetch_layout_builds();

    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    let layout_after_first = web_fetch_layout_builds();
    assert!(
        layout_after_first > 0,
        "expected web_fetch layout to be computed on first render"
    );

    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    assert_eq!(
        web_fetch_layout_builds(),
        layout_after_first,
        "web_fetch layout should be cached for stable renders"
    );

    for _ in 0..6 {
        harness.send_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        let layout_once = web_fetch_layout_builds();

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        assert_eq!(
            web_fetch_layout_builds(),
            layout_once,
            "web_fetch layout should not be recomputed on repeated renders at a stable scroll offset"
        );
    }
}

#[test]
fn stable_renders_do_not_miss_history_layout_cache() {
    let mut harness = ChatWidgetHarness::new();

    // Seed many plain message cells with stable history IDs so HistoryRenderState caching is used.
    for idx in 0..200_u64 {
        harness.push_assistant_markdown(format!(
            "Assistant {idx}: {pad}",
            pad = "y".repeat(220)
        ));
    }

    reset_history_layout_cache_stats();

    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    let misses_after_first = history_layout_cache_misses();
    assert!(
        misses_after_first > 0,
        "expected at least one history layout cache miss on the first render"
    );

    // Second render at the same scroll offset should be hit-only.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
    assert_eq!(
        history_layout_cache_misses(),
        misses_after_first,
        "history layout cache should not miss on stable re-renders"
    );

    // Scroll, then verify the second render at the same scroll offset doesn't miss.
    for _ in 0..6 {
        harness.send_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        let misses_once = history_layout_cache_misses();

        let _ = render_chat_widget_to_vt100(&mut harness, 120, 30);
        assert_eq!(
            history_layout_cache_misses(),
            misses_once,
            "history layout cache should not miss on repeated renders at a stable scroll offset"
        );
    }
}
