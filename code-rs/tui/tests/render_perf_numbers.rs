#![allow(clippy::unwrap_used, clippy::expect_used)]

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
use code_tui::test_helpers::{render_chat_widget_to_vt100, ChatWidgetHarness};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn make_code_block(lines: usize) -> String {
    let mut out = String::new();
    out.push_str("```rust\n");
    for idx in 0..lines {
        out.push_str(&format!("let x_{idx} = {idx};\n"));
    }
    out.push_str("```\n");
    out
}

fn make_long_code_block(lines: usize, line_len: usize) -> String {
    let mut out = String::new();
    out.push_str("```rust\n");
    let payload = "x".repeat(line_len);
    for idx in 0..lines {
        out.push_str(&format!("let s{idx} = \"{payload}\";\n"));
    }
    out.push_str("```\n");
    out
}

fn next_order_meta(request_ordinal: u64, seq: &mut u64) -> OrderMeta {
    let order = OrderMeta {
        request_ordinal,
        output_index: Some(0),
        sequence_number: Some(*seq),
    };
    *seq += 1;
    order
}

fn seed_completed_exec_cells(harness: &mut ChatWidgetHarness, count: usize) {
    let mut seq = 0_u64;
    let cwd = PathBuf::from("/tmp");
    for idx in 0..count {
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
                stdout: format!("line {idx}: {pad}\n", pad = "x".repeat(180)),
                stderr: String::new(),
                exit_code: 0,
                duration: Duration::from_millis(50),
            }),
            order: Some(next_order_meta(1, &mut seq)),
        });
    }
}

fn seed_plain_assistant_cells(harness: &mut ChatWidgetHarness, count: usize) {
    for idx in 0..count {
        harness.push_assistant_markdown(format!(
            "Assistant {idx}: {pad}",
            pad = "y".repeat(240)
        ));
    }
}

fn seed_merged_exec_cells(harness: &mut ChatWidgetHarness, count: usize) {
    for idx in 0..count {
        let mut segments = Vec::with_capacity(3);
        for seg in 0..3_u64 {
            segments.push(ExecRecord {
                id: HistoryId((idx as u64).saturating_mul(10).saturating_add(seg)),
                call_id: Some(format!("merged_{idx}_{seg}")),
                command: vec!["echo".into(), format!("{idx}:{seg}")],
                parsed: Vec::new(),
                action: ExecAction::Run,
                status: ExecStatus::Success,
                stdout_chunks: vec![ExecStreamChunk {
                    offset: 0,
                    content: format!("line {idx}:{seg}: {pad}\n", pad = "x".repeat(180)),
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
            id: HistoryId(idx as u64),
            action: ExecAction::Run,
            segments,
        });
    }
}

fn seed_reasoning_cells(harness: &mut ChatWidgetHarness, count: usize) {
    for idx in 0..count {
        let spans = vec![InlineSpan {
            text: format!("Reasoning {idx}: {pad}", pad = "z".repeat(240)),
            tone: TextTone::Default,
            emphasis: TextEmphasis::default(),
            entity: None,
        }];
        let section = ReasoningSection {
            heading: Some(format!("Section {idx}")),
            summary: None,
            blocks: vec![ReasoningBlock::Paragraph(spans)],
        };
        harness.push_reasoning_state(
            ReasoningState {
                id: HistoryId(idx as u64),
                sections: vec![section],
                effort: None,
                in_progress: false,
            },
            false,
        );
    }
}

fn seed_web_fetch_cells(harness: &mut ChatWidgetHarness, count: usize) {
    for idx in 0..count {
        let mut result = String::new();
        for line_idx in 0..200_u64 {
            result.push_str(&format!(
                "line {idx}-{line_idx}: {pad}\n",
                pad = "w".repeat(180)
            ));
        }
        harness.push_web_fetch_tool_call(result);
    }
}

fn perf_numbers_enabled() -> bool {
    std::env::var_os("CODEX_TUI_PERF_NUMBERS").is_some()
}

#[test]
fn print_widget_render_cost_for_scrollback_scenario() {
    if !perf_numbers_enabled() {
        return;
    }
    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    // Seed enough content that scrollback exists and a few code cards are visible.
    let code = make_code_block(6);
    for idx in 0..120 {
        harness.push_assistant_message(format!("Assistant {idx}: short message.\n\n{code}"));
    }

    // Warm-up to populate caches.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);

    let before = harness.perf_stats_snapshot();
    let frames = 120u64;
    for _ in 0..frames {
        let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);
    }
    let after = harness.perf_stats_snapshot();

    let rendered_frames = after.frames.saturating_sub(before.frames);
    let ns_widget = after
        .ns_widget_render_total
        .saturating_sub(before.ns_widget_render_total);
    let ns_render = after
        .ns_render_loop
        .saturating_sub(before.ns_render_loop);

    let avg_widget_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_widget as f64) / (rendered_frames as f64) / 1_000_000.0
    };
    let avg_visible_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_render as f64) / (rendered_frames as f64) / 1_000_000.0
    };

    println!(
        "render_perf_numbers: frames={rendered_frames} avg_widget_render_ms={avg_widget_ms:.3} avg_visible_render_ms={avg_visible_ms:.3}"
    );
}

#[test]
fn print_widget_render_cost_for_width_churn_code_blocks() {
    if !perf_numbers_enabled() {
        return;
    }

    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    // Force frequent re-layout and re-highlighting by changing terminal width
    // every frame. This captures the "resize churn" cost rather than the
    // steady-state cached render cost.
    let code = make_code_block(20);
    let mut msg = String::new();
    for idx in 0..8 {
        msg.push_str(&format!("Code block {idx}\n\n{code}\n"));
    }
    harness.push_assistant_message(msg);

    // Warm-up to populate global caches (syntax set, themes, etc.).
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);

    let before = harness.perf_stats_snapshot();
    let frames = 80u64;
    for idx in 0..frames {
        // Use unique widths so HistoryRenderState doesn't re-use per-width caches.
        let width = 80u16.saturating_add(idx as u16);
        let _ = render_chat_widget_to_vt100(&mut harness, width, 40);
    }
    let after = harness.perf_stats_snapshot();

    let rendered_frames = after.frames.saturating_sub(before.frames);
    let ns_widget = after
        .ns_widget_render_total
        .saturating_sub(before.ns_widget_render_total);
    let ns_render = after
        .ns_render_loop
        .saturating_sub(before.ns_render_loop);

    let avg_widget_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_widget as f64) / (rendered_frames as f64) / 1_000_000.0
    };
    let avg_visible_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_render as f64) / (rendered_frames as f64) / 1_000_000.0
    };

    println!(
        "render_perf_numbers_width_churn_code_blocks: frames={rendered_frames} avg_widget_render_ms={avg_widget_ms:.3} avg_visible_render_ms={avg_visible_ms:.3}"
    );
}

#[test]
fn print_widget_render_cost_for_width_churn_long_code_lines() {
    if !perf_numbers_enabled() {
        return;
    }

    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    let code = make_long_code_block(4, 1600);
    harness.push_assistant_message(format!("Long code lines\n\n{code}"));

    // Warm-up to populate caches.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);

    let before = harness.perf_stats_snapshot();
    let frames = 50u64;
    for idx in 0..frames {
        let width = 80u16.saturating_add(idx as u16);
        let _ = render_chat_widget_to_vt100(&mut harness, width, 40);
    }
    let after = harness.perf_stats_snapshot();

    let rendered_frames = after.frames.saturating_sub(before.frames);
    let ns_widget = after
        .ns_widget_render_total
        .saturating_sub(before.ns_widget_render_total);
    let ns_render = after
        .ns_render_loop
        .saturating_sub(before.ns_render_loop);

    let avg_widget_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_widget as f64) / (rendered_frames as f64) / 1_000_000.0
    };
    let avg_visible_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_render as f64) / (rendered_frames as f64) / 1_000_000.0
    };

    println!(
        "render_perf_numbers_width_churn_long_code_lines: frames={rendered_frames} avg_widget_render_ms={avg_widget_ms:.3} avg_visible_render_ms={avg_visible_ms:.3}"
    );
}

#[test]
fn print_widget_render_cost_for_exec_scrollback_scenario() {
    if !perf_numbers_enabled() {
        return;
    }
    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    seed_completed_exec_cells(&mut harness, 120);

    // Warm-up to populate caches.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);

    let before = harness.perf_stats_snapshot();
    let frames = 120u64;
    for _ in 0..frames {
        let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);
    }
    let after = harness.perf_stats_snapshot();

    let rendered_frames = after.frames.saturating_sub(before.frames);
    let ns_widget = after
        .ns_widget_render_total
        .saturating_sub(before.ns_widget_render_total);
    let ns_render = after
        .ns_render_loop
        .saturating_sub(before.ns_render_loop);

    let avg_widget_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_widget as f64) / (rendered_frames as f64) / 1_000_000.0
    };
    let avg_visible_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_render as f64) / (rendered_frames as f64) / 1_000_000.0
    };

    println!(
        "render_perf_numbers_exec: frames={rendered_frames} avg_widget_render_ms={avg_widget_ms:.3} avg_visible_render_ms={avg_visible_ms:.3}"
    );
}

#[test]
fn print_widget_render_cost_for_plain_history_scrollback_scenario() {
    if !perf_numbers_enabled() {
        return;
    }
    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    seed_plain_assistant_cells(&mut harness, 240);

    // Warm-up to populate caches.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);

    let before = harness.perf_stats_snapshot();
    let frames = 120u64;
    for _ in 0..frames {
        let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);
    }
    let after = harness.perf_stats_snapshot();

    let rendered_frames = after.frames.saturating_sub(before.frames);
    let ns_widget = after
        .ns_widget_render_total
        .saturating_sub(before.ns_widget_render_total);
    let ns_render = after
        .ns_render_loop
        .saturating_sub(before.ns_render_loop);

    let avg_widget_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_widget as f64) / (rendered_frames as f64) / 1_000_000.0
    };
    let avg_visible_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_render as f64) / (rendered_frames as f64) / 1_000_000.0
    };

    println!(
        "render_perf_numbers_plain: frames={rendered_frames} avg_widget_render_ms={avg_widget_ms:.3} avg_visible_render_ms={avg_visible_ms:.3}"
    );
}

#[test]
fn print_widget_render_cost_for_merged_exec_scrollback_scenario() {
    if !perf_numbers_enabled() {
        return;
    }
    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    seed_merged_exec_cells(&mut harness, 120);

    // Warm-up to populate caches.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);

    let before = harness.perf_stats_snapshot();
    let frames = 120u64;
    for _ in 0..frames {
        let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);
    }
    let after = harness.perf_stats_snapshot();

    let rendered_frames = after.frames.saturating_sub(before.frames);
    let ns_widget = after
        .ns_widget_render_total
        .saturating_sub(before.ns_widget_render_total);
    let ns_render = after
        .ns_render_loop
        .saturating_sub(before.ns_render_loop);

    let avg_widget_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_widget as f64) / (rendered_frames as f64) / 1_000_000.0
    };
    let avg_visible_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_render as f64) / (rendered_frames as f64) / 1_000_000.0
    };

    println!(
        "render_perf_numbers_merged_exec: frames={rendered_frames} avg_widget_render_ms={avg_widget_ms:.3} avg_visible_render_ms={avg_visible_ms:.3}"
    );
}

#[test]
fn print_widget_render_cost_for_reasoning_scrollback_scenario() {
    if !perf_numbers_enabled() {
        return;
    }
    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    seed_reasoning_cells(&mut harness, 120);

    // Warm-up to populate caches.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);

    let before = harness.perf_stats_snapshot();
    let frames = 120u64;
    for _ in 0..frames {
        let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);
    }
    let after = harness.perf_stats_snapshot();

    let rendered_frames = after.frames.saturating_sub(before.frames);
    let ns_widget = after
        .ns_widget_render_total
        .saturating_sub(before.ns_widget_render_total);
    let ns_render = after
        .ns_render_loop
        .saturating_sub(before.ns_render_loop);

    let avg_widget_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_widget as f64) / (rendered_frames as f64) / 1_000_000.0
    };
    let avg_visible_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_render as f64) / (rendered_frames as f64) / 1_000_000.0
    };

    println!(
        "render_perf_numbers_reasoning: frames={rendered_frames} avg_widget_render_ms={avg_widget_ms:.3} avg_visible_render_ms={avg_visible_ms:.3}"
    );
}

#[test]
fn print_widget_render_cost_for_web_fetch_scrollback_scenario() {
    if !perf_numbers_enabled() {
        return;
    }
    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    seed_web_fetch_cells(&mut harness, 120);

    // Warm-up to populate caches.
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);

    let before = harness.perf_stats_snapshot();
    let frames = 120u64;
    for _ in 0..frames {
        let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);
    }
    let after = harness.perf_stats_snapshot();

    let rendered_frames = after.frames.saturating_sub(before.frames);
    let ns_widget = after
        .ns_widget_render_total
        .saturating_sub(before.ns_widget_render_total);
    let ns_render = after
        .ns_render_loop
        .saturating_sub(before.ns_render_loop);

    let avg_widget_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_widget as f64) / (rendered_frames as f64) / 1_000_000.0
    };
    let avg_visible_ms = if rendered_frames == 0 {
        0.0
    } else {
        (ns_render as f64) / (rendered_frames as f64) / 1_000_000.0
    };

    println!(
        "render_perf_numbers_web_fetch: frames={rendered_frames} avg_widget_render_ms={avg_widget_ms:.3} avg_visible_render_ms={avg_visible_ms:.3}"
    );
}
