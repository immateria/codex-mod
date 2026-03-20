    use ratatui::backend::TestBackend;
    use ratatui::text::Line;
    use ratatui::Terminal;
    use std::collections::HashMap;
    use std::time::{Duration, Instant, SystemTime};
    
    use code_core::protocol::{ReviewFinding, ReviewCodeLocation, ReviewLineRange};
    
    struct CaptureCommitStubGuard;
    
    impl CaptureCommitStubGuard {
    fn install<F>(stub: F) -> Self
    where
        F: Fn(&'static str, Option<String>) -> Result<GhostCommit, GitToolingError>
            + Send
            + Sync
            + 'static,
    {
        let mut slot = match CAPTURE_AUTO_TURN_COMMIT_STUB.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        assert!(slot.is_none(), "capture stub already installed");
        *slot = Some(Box::new(stub));
        Self
    }
    }
    
    impl Drop for CaptureCommitStubGuard {
    fn drop(&mut self) {
        match CAPTURE_AUTO_TURN_COMMIT_STUB.lock() {
            Ok(mut slot) => *slot = None,
            Err(poisoned) => {
                let mut slot = poisoned.into_inner();
                *slot = None;
            }
        }
    }
    }
    
    struct GitDiffStubGuard;
    
    impl GitDiffStubGuard {
    fn install<F>(stub: F) -> Self
    where
        F: Fn(String, String) -> Result<Vec<String>, String> + Send + Sync + 'static,
    {
        let mut slot = match GIT_DIFF_NAME_ONLY_BETWEEN_STUB.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        assert!(slot.is_none(), "git diff stub already installed");
        *slot = Some(Box::new(stub));
        Self
    }
    }
    
    impl Drop for GitDiffStubGuard {
    fn drop(&mut self) {
        match GIT_DIFF_NAME_ONLY_BETWEEN_STUB.lock() {
            Ok(mut slot) => *slot = None,
            Err(poisoned) => {
                let mut slot = poisoned.into_inner();
                *slot = None;
            }
        }
    }
    }
    
fn reset_history(chat: &mut ChatWidget<'_>) {
    chat.history_cells.clear();
    chat.history_cell_ids.clear();
    chat.history_live_window = None;
    chat.history_frozen_width = 0;
    chat.history_frozen_count = 0;
    chat.history_virtualization_sync_pending.set(false);
    chat.history_state = HistoryState::new();
    chat.history_render.invalidate_all();
    chat.cell_order_seq.clear();
    chat.cell_order_dbg.clear();
    chat.ui_background_seq_counters.clear();
    chat.last_assigned_order = None;
    chat.last_seen_request_index = 0;
    chat.current_request_index = 0;
    chat.internal_seq = 0;
    chat.order_request_bias = 0;
    chat.resume_expected_next_request = None;
    chat.resume_provider_baseline = None;
    chat.synthetic_system_req = None;
    chat.layout.scroll_offset.set(0);
    chat.layout.last_max_scroll.set(0);
    chat.layout.last_history_viewport_height.set(0);
}
    
    fn insert_plain_cell(chat: &mut ChatWidget<'_>, lines: &[&str]) {
    use code_core::history::state::{
        InlineSpan,
        MessageLine,
        MessageLineKind,
        PlainMessageKind,
        PlainMessageRole,
        PlainMessageState,
        TextEmphasis,
        TextTone,
    };
    
    let state = PlainMessageState {
        id: HistoryId::ZERO,
        role: PlainMessageRole::System,
        kind: PlainMessageKind::Plain,
        header: None,
        lines: lines
            .iter()
            .map(|text| MessageLine {
                kind: MessageLineKind::Paragraph,
                spans: vec![InlineSpan {
                    text: (*text).to_string(),
                    tone: TextTone::Default,
                    emphasis: TextEmphasis::default(),
                    entity: None,
                }],
            })
            .collect(),
        metadata: None,
    };
    
    let key = chat.next_internal_key();
    let _ = chat.history_insert_plain_state_with_key(state, key, "test");
    }
    
    fn make_pending_fix_state(review: ReviewOutputEvent) -> AutoResolveState {
    AutoResolveState {
        target: ReviewTarget::UncommittedChanges,
        prompt: "prompt".to_string(),
        hint: "hint".to_string(),
        metadata: None,
        attempt: 0,
        max_attempts: AUTO_RESOLVE_MAX_REVIEW_ATTEMPTS,
        phase: AutoResolvePhase::PendingFix { review },
        last_review: None,
        last_fix_message: None,
        last_reviewed_commit: None,
        snapshot_epoch: None,
    }
    }
    
