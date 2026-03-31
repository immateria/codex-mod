    #[test]
    fn parse_agent_review_result_json_clean() {
    let json = r#"{
        "findings": [],
        "overall_correctness": "ok",
        "overall_explanation": "looks clean",
        "overall_confidence_score": 0.9
    }"#;
    
    let (has_findings, findings, summary) = ChatWidget::parse_agent_review_result(Some(json));
    assert!(!has_findings);
    assert_eq!(findings, 0);
    assert_eq!(summary.as_deref(), Some("looks clean"));
    }
    
    #[test]
    fn parse_agent_review_result_json_with_findings() {
    let json = r#"{
        "findings": [
            {"title": "bug", "body": "fix", "confidence_score": 0.5, "priority": 1, "code_location": {"absolute_file_path": "foo", "line_range": {"start":1,"end":1}}}
        ],
        "overall_correctness": "incorrect",
        "overall_explanation": "needs work",
        "overall_confidence_score": 0.6
    }"#;
    
    let (has_findings, findings, summary) = ChatWidget::parse_agent_review_result(Some(json));
    assert!(has_findings);
    assert_eq!(findings, 1);
    let summary_text = summary.unwrap();
    assert!(summary_text.contains("needs work"));
    assert!(summary_text.contains("bug"));
    }
    
    #[test]
    fn mcp_summary_includes_tools_and_failures() {
    let mut harness = ChatWidgetHarness::new();
    harness.with_chat(|chat| {
        chat.mcp_tools_by_server.insert(
            "alpha".to_string(),
            vec!["fetch".to_string(), "search".to_string()],
        );
        chat.mcp_server_failures.insert(
            "beta".to_string(),
            McpServerFailure {
                phase: McpServerFailurePhase::ListTools,
                message: "timeout".to_string(),
            },
        );
    
        let ok_cfg = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "alpha-bin".to_string(),
                args: Vec::new(),
                env: None,
            },
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            disabled_tools: Vec::new(),
            scheduling: code_core::config_types::McpServerSchedulingToml::default(),
            tool_scheduling: BTreeMap::new(),
        };
        let fail_cfg = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "beta-bin".to_string(),
                args: Vec::new(),
                env: None,
            },
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            disabled_tools: Vec::new(),
            scheduling: code_core::config_types::McpServerSchedulingToml::default(),
            tool_scheduling: BTreeMap::new(),
        };
    
        let ok_summary = format!(
            "{} · {}",
            ChatWidget::format_mcp_summary(&ok_cfg),
            chat.format_mcp_tool_status("alpha", true)
        );
        let fail_summary = format!(
            "{} · {}",
            ChatWidget::format_mcp_summary(&fail_cfg),
            chat.format_mcp_tool_status("beta", true)
        );
    
        assert!(
            ok_summary.contains("Tools: fetch, search"),
            "expected tool list in summary, got: {ok_summary}"
        );
        assert!(
            fail_summary.contains("Failed to list tools: timeout"),
            "expected failure message in summary, got: {fail_summary}"
        );
    });
    }

    #[test]
    fn mcp_status_report_includes_auth_timeouts_and_disabled_tools() {
    let rows = vec![
        McpServerRow {
            name: "alpha".to_string(),
            enabled: true,
            transport: "npx -y alpha".to_string(),
            auth_status: McpAuthStatus::OAuth,
            startup_timeout: Some(Duration::from_secs(12)),
            tool_timeout: Some(Duration::from_secs(4)),
            scheduling: code_core::config_types::McpServerSchedulingToml::default(),
            tool_scheduling: BTreeMap::new(),
            tools: vec!["fetch".to_string(), "search".to_string()],
            disabled_tools: vec!["legacy".to_string()],
            resources: Vec::new(),
            resource_templates: Vec::new(),
            tool_definitions: BTreeMap::new(),
            failure: None,
            status: "Tools: fetch, search".to_string(),
        },
        McpServerRow {
            name: "beta".to_string(),
            enabled: false,
            transport: "HTTP https://beta.example/mcp".to_string(),
            auth_status: McpAuthStatus::Unsupported,
            startup_timeout: None,
            tool_timeout: None,
            scheduling: code_core::config_types::McpServerSchedulingToml::default(),
            tool_scheduling: BTreeMap::new(),
            tools: Vec::new(),
            disabled_tools: Vec::new(),
            resources: Vec::new(),
            resource_templates: Vec::new(),
            tool_definitions: BTreeMap::new(),
            failure: None,
            status: "Tools: disabled".to_string(),
        },
    ];

    let report = ChatWidget::format_mcp_status_report(&rows);
    assert!(report.contains("Enabled (1):"), "{report}");
    assert!(report.contains("Disabled (1):"), "{report}");
    assert!(report.contains("Auth: OAuth"), "{report}");
    assert!(report.contains("startup_timeout_sec: 12.000"), "{report}");
    assert!(report.contains("tool_timeout_sec: 4.000"), "{report}");
    assert!(report.contains("disabled_tools (1): legacy"), "{report}");
    assert!(report.contains("Auth: Unsupported"), "{report}");
    }
    
    #[test]
    fn parse_agent_review_result_json_multi_run() {
    let json = r#"{
        "findings": [],
        "overall_correctness": "correct",
        "overall_explanation": "clean",
        "overall_confidence_score": 0.9,
        "runs": [
            {
                "findings": [
                    {"title": "bug", "body": "fix", "confidence_score": 0.5, "priority": 1, "code_location": {"absolute_file_path": "foo", "line_range": {"start":1,"end":1}}}
                ],
                "overall_correctness": "incorrect",
                "overall_explanation": "needs work",
                "overall_confidence_score": 0.6
            },
            {
                "findings": [],
                "overall_correctness": "correct",
                "overall_explanation": "clean",
                "overall_confidence_score": 0.9
            }
        ]
    }"#;
    
    let (has_findings, findings, summary) = ChatWidget::parse_agent_review_result(Some(json));
    assert!(has_findings);
    assert_eq!(findings, 1);
    let summary_text = summary.unwrap();
    assert!(summary_text.contains("needs work"));
    assert!(summary_text.contains("Final pass reported no issues"));
    }
    
    #[test]
    fn parse_agent_review_result_skip_lock() {
    let text = "Another review is already running; skipping this /review.";
    let (has_findings, findings, summary) = ChatWidget::parse_agent_review_result(Some(text));
    
    assert!(!has_findings);
    assert_eq!(findings, 0);
    assert_eq!(summary.as_deref(), Some(text));
    }
    
    #[test]
    fn format_model_name_capitalizes_codex_mini() {
    let mut harness = ChatWidgetHarness::new();
    let formatted = harness.chat().format_model_name("gpt-5.1-codex-mini");
    assert_eq!(formatted, "GPT-5.1-Codex-Mini");
    }
    
    #[test]
    fn auto_review_triggers_when_enabled_and_diff_seen() {
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    let _guard = AutoReviewStubGuard::install(|| {});
    let _capture_guard = CaptureCommitStubGuard::install(|_, _| {
        Ok(GhostCommit::new("baseline".to_string(), None))
    });
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    chat.config.tui.auto_review_enabled = true;
    chat.turn_had_code_edits = true;
    chat.background_review = None;
    
    chat.maybe_trigger_auto_review();
    
    assert!(chat.background_review.is_some(), "background review should start");
    }
    
    #[test]
    fn auto_review_does_not_duplicate_while_running() {
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = calls.clone();
    let _guard = AutoReviewStubGuard::install(move || {
        calls_clone.fetch_add(1, Ordering::SeqCst);
    });
    let _capture_guard = CaptureCommitStubGuard::install(|_, _| {
        Ok(GhostCommit::new("baseline".to_string(), None))
    });
    
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    chat.config.tui.auto_review_enabled = true;
    chat.turn_had_code_edits = true;
    chat.background_review = None;
    
    chat.maybe_trigger_auto_review();
    // Already running; second trigger should no-op
    chat.turn_had_code_edits = true;
    chat.maybe_trigger_auto_review();
    
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    
    #[test]
    fn auto_review_skips_when_no_changes_since_reviewed_snapshot() {
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    let _rt = enter_test_runtime_guard();
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = calls.clone();
    let _guard = AutoReviewStubGuard::install(move || {
        calls_clone.fetch_add(1, Ordering::SeqCst);
    });
    
    let repo = tempdir().expect("temp repo");
    let repo_path = repo.path();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .current_dir(repo_path)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git command failed: {args:?}");
    };
    
    git(&["init"]);
    git(&["config", "user.email", "auto@review.test"]);
    git(&["config", "user.name", "Auto Review"]);
    std::fs::write(repo_path.join("README.md"), "hello")
        .expect("write README");
    git(&["add", "."]);
    git(&["commit", "-m", "init"]);
    
    let snapshot = create_ghost_commit(
        &CreateGhostCommitOptions::new(repo_path).message("auto review snapshot"),
    )
    .expect("ghost snapshot");
    
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    chat.config.cwd = repo_path.to_path_buf();
    chat.config.tui.auto_review_enabled = true;
    chat.turn_had_code_edits = true;
    chat.auto_review_reviewed_marker = Some(snapshot);
    
    chat.maybe_trigger_auto_review();
    
    assert_eq!(calls.load(Ordering::SeqCst), 0, "auto review should skip");
    assert!(chat.background_review.is_none());
    }
    
    #[test]
    fn task_started_defers_auto_review_baseline_capture() {
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    let _rt = enter_test_runtime_guard();
    let _capture_guard = CaptureCommitStubGuard::install(|_, _| {
        Ok(GhostCommit::new("baseline".to_string(), None))
    });
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    chat.config.tui.auto_review_enabled = true;
    
    chat.handle_code_event(Event {
        id: "turn-1".to_string(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: None,
    });
    
    assert!(
        chat.auto_review_baseline.is_none(),
        "baseline capture should not block TaskStarted"
    );
    }
    
    #[test]
    fn background_review_completion_resumes_auto_and_posts_summary() {
    let _rt = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.insert_final_answer_with_id(
        None,
        vec![ratatui::text::Line::from("Assistant reply")],
        "Assistant reply".to_string(),
    );
    
    chat.config.tui.auto_review_enabled = true;
    chat.auto_state.on_begin_review(false);
    
    chat.background_review = Some(BackgroundReviewState {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-branch".to_string(),
        agent_id: Some("agent-123".to_string()),
        snapshot: Some("ghost123".to_string()),
        base: None,
        last_seen: std::time::Instant::now(),
    });
    
    chat.on_background_review_finished(BackgroundReviewFinishedEvent {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-branch".to_string(),
        has_findings: true,
        findings: 2,
        summary: Some("Short summary".to_string()),
        error: None,
        agent_id: Some("agent-123".to_string()),
        snapshot: Some("ghost123".to_string()),
    });
    
    assert!(
        !chat.auto_state.awaiting_review(),
        "auto drive should resume after background review completes"
    );
    
    let footer_status = chat
        .bottom_pane
        .auto_review_status()
        .expect("footer should show auto review status");
    assert_eq!(footer_status.status, AutoReviewIndicatorStatus::Fixed);
    assert_eq!(footer_status.findings, Some(2));
    let notice_present = chat.history_cells.iter().any(|cell| {
        cell.display_lines_trimmed().iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("issue(s) found"))
        })
    });
    assert!(notice_present, "actionable auto review notice should be visible");
    assert!(chat.pending_agent_notes.is_empty(), "idle path should inject via hidden message, not queue notes");
    let developer_seen = chat
        .pending_dispatched_user_messages
        .iter()
        .any(|msg| msg.contains("[developer]"));
    assert!(developer_seen, "developer note should be sent in hidden message");
    }
    
    #[test]
    fn background_review_busy_path_enqueues_developer_note_with_merge_hint() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.config.tui.auto_review_enabled = true;
    chat.bottom_pane.set_task_running(true); // simulate busy state so note is queued
    
    chat.background_review = Some(BackgroundReviewState {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-branch".to_string(),
        agent_id: Some("agent-123".to_string()),
        snapshot: Some("ghost123".to_string()),
        base: None,
        last_seen: std::time::Instant::now(),
    });
    
    // Agent.result will be parsed; provide structured JSON with findings
    let review_json = r#"{
        "findings": [
            {"title": "bug", "body": "fix", "confidence_score": 0.5, "priority": 1, "code_location": {"absolute_file_path": "foo", "line_range": {"start":1,"end":1}}}
        ],
        "overall_correctness": "incorrect",
        "overall_explanation": "needs work",
        "overall_confidence_score": 0.6
    }"#;
    
    // Simulate agent status observation completion path
    chat.on_background_review_finished(BackgroundReviewFinishedEvent {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-branch".to_string(),
        has_findings: true,
        findings: 1,
        summary: Some(review_json.to_string()),
        error: None,
        agent_id: Some("agent-123".to_string()),
        snapshot: Some("ghost123".to_string()),
    });
    
    // Busy path still injects a developer note immediately so the user sees it in the transcript.
    assert!(chat.pending_agent_notes.is_empty());
    let developer_sent = chat
        .pending_dispatched_user_messages
        .iter()
        .any(|msg| msg.contains("[developer]") && msg.contains("Merge the worktree") && msg.contains("auto-review-branch"));
    assert!(developer_sent, "developer merge-hint note should be injected even while busy");
    }
    
    #[test]
    fn background_review_observe_idle_injects_note_from_agent_result() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.config.tui.auto_review_enabled = true;
    chat.background_review = Some(BackgroundReviewState {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-branch".to_string(),
        agent_id: None,
        snapshot: Some("ghost123".to_string()),
        base: None,
        last_seen: std::time::Instant::now(),
    });
    
    let agent = code_core::protocol::AgentInfo {
        id: "agent-1".to_string(),
        name: "Auto Review".to_string(),
        status: "completed".to_string(),
        batch_id: Some("auto-review-branch".to_string()),
        model: Some("code-review".to_string()),
        last_progress: None,
        result: Some(
            r#"{
                "findings":[{"title":"bug","body":"details","confidence_score":0.5,"priority":1,"code_location":{"absolute_file_path":"src/lib.rs","line_range":{"start":1,"end":1}}}],
                "overall_correctness":"incorrect",
                "overall_explanation":"needs work",
                "overall_confidence_score":0.6
            }"#
            .to_string(),
        ),
        error: None,
        elapsed_ms: None,
        token_count: None,
        last_activity_at: None,
        seconds_since_last_activity: None,
        source_kind: Some(AgentSourceKind::AutoReview),
    };
    
    chat.observe_auto_review_status(&[agent]);
    
    // Idle path: should send hidden developer note immediately (not queued)
    assert!(chat.pending_agent_notes.is_empty());
    let developer_sent = chat
        .pending_dispatched_user_messages
        .iter()
        .any(|msg| msg.contains("[developer]") && msg.contains("Merge the worktree") && msg.contains("auto-review-branch"));
    assert!(developer_sent, "developer merge-hint note should be injected when idle");
    }
    
    #[test]
    fn background_review_observe_busy_queues_note_from_agent_result() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.config.tui.auto_review_enabled = true;
    chat.bottom_pane.set_task_running(true);
    chat.background_review = Some(BackgroundReviewState {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-branch".to_string(),
        agent_id: None,
        snapshot: Some("ghost123".to_string()),
        base: None,
        last_seen: std::time::Instant::now(),
    });
    
    let agent = code_core::protocol::AgentInfo {
        id: "agent-1".to_string(),
        name: "Auto Review".to_string(),
        status: "completed".to_string(),
        batch_id: Some("auto-review-branch".to_string()),
        model: Some("code-review".to_string()),
        last_progress: None,
        result: Some(
            r#"{
                "findings":[{"title":"bug","body":"details","confidence_score":0.5,"priority":1,"code_location":{"absolute_file_path":"src/lib.rs","line_range":{"start":1,"end":1}}}],
                "overall_correctness":"incorrect",
                "overall_explanation":"needs work",
                "overall_confidence_score":0.6
            }"#
            .to_string(),
        ),
        error: None,
        elapsed_ms: None,
        token_count: None,
        last_activity_at: None,
        seconds_since_last_activity: None,
        source_kind: Some(AgentSourceKind::AutoReview),
    };
    
    chat.observe_auto_review_status(&[agent]);
    
    assert!(chat.pending_agent_notes.is_empty());
    let developer_sent = chat
        .pending_dispatched_user_messages
        .iter()
        .any(|msg| msg.contains("[developer]") && msg.contains("Merge the worktree") && msg.contains("auto-review-branch"));
    assert!(developer_sent, "developer merge-hint note should be injected when busy");
    }
    
    #[test]
    fn terminal_auto_review_without_worktree_state_does_not_surface_blank_path() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.config.tui.auto_review_enabled = true;
    chat.background_review = None;
    
    let agent = code_core::protocol::AgentInfo {
        id: "agent-blank".to_string(),
        name: "Auto Review".to_string(),
        status: "failed".to_string(),
        batch_id: None,
        model: Some("code-review".to_string()),
        last_progress: None,
        result: None,
        error: Some("fatal: not a git repository".to_string()),
        elapsed_ms: None,
        token_count: None,
        last_activity_at: None,
        seconds_since_last_activity: None,
        source_kind: Some(AgentSourceKind::AutoReview),
    };
    
    chat.observe_auto_review_status(&[agent]);
    
    let blank_path_message = chat
        .pending_dispatched_user_messages
        .iter()
        .any(|msg| msg.contains("Worktree path: \n") || msg.contains("Worktree path: \r\n"));
    assert!(!blank_path_message, "should not emit auto-review message with blank worktree path");
    assert!(chat.processed_auto_review_agents.contains("agent-blank"));
    }
    
    #[test]
    fn missing_agent_clis_start_disabled_in_overview() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();

    // Avoid mutating global PATH (which can break other tests when run in
    // parallel). Instead, add a deterministic "missing CLI" agent entry that
    // points at a non-existent file path so `command_exists(...)` is false
    // regardless of the host environment.
    chat.config.agents.push(code_core::config_types::AgentConfig {
        name: "missing-agent-cli".to_string(),
        command: "/nonexistent/missing-agent-cli".to_string(),
        args: Vec::new(),
        read_only: false,
        enabled: true,
        description: None,
        env: None,
        args_read_only: None,
        args_write: None,
        instructions: None,
    });
    
    let (rows, _commands) = chat.collect_agents_overview_rows();
    let missing = rows
        .iter()
        .find(|row| row.name == "missing-agent-cli")
        .expect("missing agent row present");
    assert!(!missing.installed);
    assert!(!missing.enabled);
    
    let code_slug = enabled_agent_model_specs()
        .into_iter()
        .find(|spec| spec.family == "code")
        .expect("code agent spec present")
        .slug;
    let code = rows
        .iter()
        .find(|row| row.name == code_slug)
        .expect("code row present");
    assert!(code.installed);
    assert!(code.enabled);
    }
    
    #[test]
    fn skipped_auto_review_with_findings_defers_to_next_turn() {
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    let _rt = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    let launches = Arc::new(AtomicUsize::new(0));
    let launches_clone = launches.clone();
    let _stub = AutoReviewStubGuard::install(move || {
        launches_clone.fetch_add(1, Ordering::SeqCst);
    });
    
    chat.config.tui.auto_review_enabled = true;
    chat.turn_sequence = 1;
    chat.turn_had_code_edits = true;
    let pending_base = GhostCommit::new("base-skip".to_string(), None);
    chat.auto_review_baseline = Some(pending_base.clone());
    
    chat.background_review = Some(BackgroundReviewState {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-running".to_string(),
        agent_id: Some("agent-running".to_string()),
        snapshot: Some("ghost-running".to_string()),
        base: Some(GhostCommit::new("running-base".to_string(), None)),
        last_seen: Instant::now(),
    });
    
    chat.maybe_trigger_auto_review();
    assert_eq!(launches.load(Ordering::SeqCst), 0, "should skip while review runs");
    let pending = chat
        .pending_auto_review_range
        .as_ref()
        .expect("pending range queued");
    assert_eq!(pending.base.id(), pending_base.id());
    assert_eq!(pending.defer_until_turn, None);
    
    chat.on_background_review_finished(BackgroundReviewFinishedEvent {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-running".to_string(),
        has_findings: true,
        findings: 2,
        summary: Some("found issues".to_string()),
        error: None,
        agent_id: Some("agent-running".to_string()),
        snapshot: Some("ghost-running".to_string()),
    });
    
    let pending_after_finish = chat
        .pending_auto_review_range
        .as_ref()
        .expect("pending kept after findings");
    assert_eq!(pending_after_finish.defer_until_turn, Some(chat.turn_sequence));
    assert_eq!(launches.load(Ordering::SeqCst), 0, "follow-up deferred to next turn");
    
    chat.turn_sequence = 2;
    chat.turn_had_code_edits = true;
    chat.auto_review_baseline = Some(GhostCommit::new("next-base".to_string(), None));
    
    chat.maybe_trigger_auto_review();
    assert_eq!(launches.load(Ordering::SeqCst), 1, "follow-up launched next turn");
    let running = chat
        .background_review
        .as_ref()
        .expect("follow-up review should be running");
    assert_eq!(
        running.base.as_ref().map(code_git_tooling::GhostCommit::id),
        Some(pending_base.id()),
        "follow-up should use first skipped base",
    );
    }
    
    #[test]
    fn skipped_auto_review_clean_runs_immediately() {
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    let _rt = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    let launches = Arc::new(AtomicUsize::new(0));
    let launches_clone = launches.clone();
    let _stub = AutoReviewStubGuard::install(move || {
        launches_clone.fetch_add(1, Ordering::SeqCst);
    });
    
    chat.config.tui.auto_review_enabled = true;
    chat.turn_sequence = 1;
    chat.turn_had_code_edits = true;
    let pending_base = GhostCommit::new("base-clean".to_string(), None);
    chat.auto_review_baseline = Some(pending_base.clone());
    
    chat.background_review = Some(BackgroundReviewState {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-running".to_string(),
        agent_id: Some("agent-running".to_string()),
        snapshot: Some("ghost-running".to_string()),
        base: Some(GhostCommit::new("running-base".to_string(), None)),
        last_seen: Instant::now(),
    });
    
    chat.maybe_trigger_auto_review();
    assert_eq!(launches.load(Ordering::SeqCst), 0);
    assert!(chat.pending_auto_review_range.is_some());
    
    chat.on_background_review_finished(BackgroundReviewFinishedEvent {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-running".to_string(),
        has_findings: false,
        findings: 0,
        summary: None,
        error: None,
        agent_id: Some("agent-running".to_string()),
        snapshot: Some("ghost-running".to_string()),
    });
    
    assert_eq!(launches.load(Ordering::SeqCst), 1, "follow-up should start immediately");
    assert!(chat.pending_auto_review_range.is_none(), "pending should be consumed");
    let running = chat.background_review.as_ref().expect("follow-up running");
    assert_eq!(
        running.base.as_ref().map(code_git_tooling::GhostCommit::id),
        Some(pending_base.id()),
        "follow-up should cover skipped base",
    );
    }
    
    #[test]
    fn multiple_skipped_auto_reviews_collapse_to_first_base() {
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    let _rt = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    let launches = Arc::new(AtomicUsize::new(0));
    let launches_clone = launches.clone();
    let _stub = AutoReviewStubGuard::install(move || {
        launches_clone.fetch_add(1, Ordering::SeqCst);
    });
    
    chat.config.tui.auto_review_enabled = true;
    chat.turn_sequence = 1;
    chat.turn_had_code_edits = true;
    let first_base = GhostCommit::new("base-first".to_string(), None);
    chat.auto_review_baseline = Some(first_base.clone());
    
    chat.background_review = Some(BackgroundReviewState {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-running".to_string(),
        agent_id: Some("agent-running".to_string()),
        snapshot: Some("ghost-running".to_string()),
        base: Some(GhostCommit::new("running-base".to_string(), None)),
        last_seen: Instant::now(),
    });
    
    chat.maybe_trigger_auto_review();
    assert_eq!(launches.load(Ordering::SeqCst), 0);
    let pending = chat
        .pending_auto_review_range
        .as_ref()
        .expect("first pending queued");
    assert_eq!(pending.base.id(), first_base.id());
    
    // Second skip while review still running
    chat.auto_review_baseline = Some(GhostCommit::new("base-second".to_string(), None));
    chat.turn_had_code_edits = true;
    chat.maybe_trigger_auto_review();
    
    let pending_after_second = chat
        .pending_auto_review_range
        .as_ref()
        .expect("pending should persist");
    assert_eq!(pending_after_second.base.id(), first_base.id());
    
    chat.on_background_review_finished(BackgroundReviewFinishedEvent {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-running".to_string(),
        has_findings: false,
        findings: 0,
        summary: None,
        error: None,
        agent_id: Some("agent-running".to_string()),
        snapshot: Some("ghost-running".to_string()),
    });
    
    assert_eq!(launches.load(Ordering::SeqCst), 1, "collapsed follow-up should run once");
    let running = chat.background_review.as_ref().expect("follow-up running");
    assert_eq!(
        running.base.as_ref().map(code_git_tooling::GhostCommit::id),
        Some(first_base.id())
    );
    assert!(chat.pending_auto_review_range.is_none());
    }
    
    #[test]
    fn stale_background_review_is_reclaimed() {
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    let _rt = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    let launches = Arc::new(AtomicUsize::new(0));
    let launches_clone = launches.clone();
    let _stub = AutoReviewStubGuard::install(move || {
        launches_clone.fetch_add(1, Ordering::SeqCst);
    });
    
    chat.config.tui.auto_review_enabled = true;
    chat.turn_had_code_edits = true;
    let base = GhostCommit::new("stale-base".to_string(), None);
    let stale_started = Instant::now()
        .checked_sub(Duration::from_secs(400))
        .unwrap_or_else(Instant::now);
    
    chat.background_review = Some(BackgroundReviewState {
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "auto-review-running".to_string(),
        agent_id: Some("agent-running".to_string()),
        snapshot: Some("ghost-running".to_string()),
        base: Some(base.clone()),
        last_seen: stale_started,
    });
    
    chat.maybe_trigger_auto_review();
    
    assert_eq!(launches.load(Ordering::SeqCst), 1, "stale review should be relaunched");
    let running = chat.background_review.as_ref().expect("reclaimed review running");
    assert_eq!(
        running.base.as_ref().map(code_git_tooling::GhostCommit::id),
        Some(base.id())
    );
    assert!(chat.pending_auto_review_range.is_none());
    }
    
    #[test]
    fn auto_drive_ctrl_s_overlay_keeps_screen_readable() {
    use crate::test_helpers::AutoContinueModeFixture;
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    harness.auto_drive_activate(
        "write some code",
        false,
        true,
        AutoContinueModeFixture::Immediate,
    );
    
    harness.open_auto_drive_settings();
    let frame_with_settings = crate::test_helpers::render_chat_widget_to_vt100(&mut harness, 90, 24);
    assert!(frame_with_settings.contains("Auto Drive Settings"));
    assert!(!frame_with_settings.contains('\u{fffd}'));
    
    harness.close_auto_drive_settings();
    let frame_after_close = crate::test_helpers::render_chat_widget_to_vt100(&mut harness, 90, 24);
    assert!(!frame_after_close.contains("Auto Drive Settings"));
    assert!(!frame_after_close.contains('\u{fffd}'));
    }
    
    #[test]
    fn slash_command_from_line_parses_prompt_expanding_commands() {
    assert!(matches!(
        ChatWidget::slash_command_from_line("/plan build it"),
        Some(SlashCommand::Plan)
    ));
    assert!(matches!(
        ChatWidget::slash_command_from_line("/code"),
        Some(SlashCommand::Code)
    ));
    assert_eq!(ChatWidget::slash_command_from_line("not-a-command"), None);
    }
    
    #[test]
    fn plan_multiline_commands_are_not_split() {
    assert!(ChatWidget::multiline_slash_command_requires_split("/auto"));
    assert!(!ChatWidget::multiline_slash_command_requires_split("/plan"));
    assert!(!ChatWidget::multiline_slash_command_requires_split("/solve add context"));
    }
    
    #[test]
    fn transient_error_sets_reconnect_ui() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness
        .chat()
        .on_error("stream error: retrying 1/5".to_string());

    assert!(harness.chat().reconnect_notice_active);
    assert!(harness.chat().bottom_pane.is_task_running());
    harness.chat().clear_reconnecting();
    assert!(!harness.chat().reconnect_notice_active);
    assert!(!harness.chat().bottom_pane.is_task_running());
    }

    #[test]
    fn warning_events_do_not_clear_reconnect_ui() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();

    chat.on_error("stream error: retrying 1/5".to_string());
    assert!(chat.reconnect_notice_active, "expected reconnect UI to be active");

    chat.handle_code_event(Event {
        id: "turn-1".to_string(),
        event_seq: 1,
        msg: EventMsg::Warning(code_core::protocol::WarningEvent {
            message: "background warning".to_string(),
        }),
        order: None,
    });

    assert!(
        chat.reconnect_notice_active,
        "warning events should not clear reconnect UI state",
    );
    assert!(
        chat.bottom_pane.is_task_running(),
        "reconnect UI should keep the spinner running until cleared explicitly",
    );
    }

    #[test]
    fn reconnect_ui_is_cleared_after_idle_timeout() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();

    chat.on_error("stream error: retrying 1/5".to_string());
    assert!(chat.reconnect_notice_active);
    assert!(chat.bottom_pane.is_task_running());

    // Simulate the reconnect banner being active for long enough while the UI
    // is otherwise idle.
    chat.reconnect_notice_started_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(10));
    chat.maybe_clear_stale_reconnecting_for_idle();

    assert!(
        !chat.reconnect_notice_active,
        "expected reconnect UI to clear once idle timeout elapses",
    );
    assert!(
        !chat.bottom_pane.is_task_running(),
        "expected spinner to stop after reconnect UI clears in idle state",
    );
    }

    #[test]
    fn startup_mcp_errors_do_not_push_history() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    chat.system_cell_by_id.clear();
    chat.startup_mcp_error_summary = None;

    chat.on_error(
        "MCP server `sequential-thinking` failed to start: failed to spawn MCP server".to_string(),
    );
    assert!(chat.history_cells.is_empty());
    let first = chat.startup_mcp_error_summary.clone().unwrap_or_default();
    assert!(first.contains("sequential-thinking"));
    assert!(first.contains("Run /mcp status"));

    chat.on_error(
        "MCP server `brave_search` failed to list tools: timed out".to_string(),
    );
    assert!(chat.history_cells.is_empty());
    let second = chat.startup_mcp_error_summary.clone().unwrap_or_default();
    assert!(second.contains("brave_search"));
    assert!(second.contains("Run /mcp status"));
    }
    #[test]
    fn review_dialog_uncommitted_option_runs_workspace_scope() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.open_review_dialog();
    chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    
    let (target, prompt, hint, preparation_label, auto_resolve) = harness
        .drain_events()
        .into_iter()
        .find_map(|event| match event {
            AppEvent::RunReviewWithScope {
                target,
                prompt,
                hint,
                preparation_label,
                auto_resolve,
            } => Some((target, prompt, hint, preparation_label, auto_resolve)),
            _ => None,
        })
        .expect("uncommitted preset should dispatch a workspace review");
    
    assert_eq!(target, ReviewTarget::UncommittedChanges);
    assert_eq!(
        prompt,
        "Review the current workspace changes (staged, unstaged, and untracked files) and highlight bugs, regressions, risky patterns, and missing tests before merge.".to_string()
    );
    assert_eq!(hint.as_deref(), Some("current workspace changes"));
    assert_eq!(
        preparation_label.as_deref(),
        Some("Preparing code review for current changes")
    );
    assert!(auto_resolve, "auto resolve now defaults to on for workspace reviews");
    }
    
    #[test]
    fn esc_router_prioritizes_auto_stop_when_waiting_for_review() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.on_begin_review(false);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AutoStopActive);
    assert!(!route.allows_double_esc);
    }
    
    #[test]
    fn esc_router_stops_auto_drive_while_waiting_for_response() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.auto_state.set_coordinator_waiting(true);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AutoStopActive);
    assert!(!route.allows_double_esc);
    }
    
    #[test]
    fn esc_router_prioritizes_cli_interrupt_before_agent_cancel() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.active_agents.push(AgentInfo {
        id: "agent-1".to_string(),
        name: "Agent 1".to_string(),
        status: AgentStatus::Running,
        source_kind: None,
        batch_id: Some("batch-1".to_string()),
        model: None,
        result: None,
        error: None,
        last_progress: None,
    });
    chat.active_task_ids.insert("turn-1".to_string());
    chat.bottom_pane.set_task_running(true);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelTask);
    }
    
    #[test]
    fn esc_router_cancels_agents_when_only_agents_running() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.active_agents.push(AgentInfo {
        id: "agent-1".to_string(),
        name: "Agent 1".to_string(),
        status: AgentStatus::Running,
        source_kind: None,
        batch_id: Some("batch-1".to_string()),
        model: None,
        result: None,
        error: None,
        last_progress: None,
    });
    chat.bottom_pane.set_task_running(true);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelAgents);
    }
    
    #[test]
    fn esc_router_skips_auto_review_cancel() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.active_agents.push(AgentInfo {
        id: "auto-1".to_string(),
        name: "Auto Review".to_string(),
        status: AgentStatus::Running,
        source_kind: Some(AgentSourceKind::AutoReview),
        batch_id: Some("review-batch".to_string()),
        model: None,
        result: None,
        error: None,
        last_progress: None,
    });
    
    let route = chat.describe_esc_context();
    assert_ne!(route.intent, EscIntent::CancelAgents);
    }
    
    #[test]
    fn cancelable_agents_excludes_auto_review_entries() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.active_agents.push(AgentInfo {
        id: "auto-1".to_string(),
        name: "Auto Review".to_string(),
        status: AgentStatus::Running,
        source_kind: Some(AgentSourceKind::AutoReview),
        batch_id: Some("review-batch".to_string()),
        model: None,
        result: None,
        error: None,
        last_progress: None,
    });
    
    chat.active_agents.push(AgentInfo {
        id: "agent-1".to_string(),
        name: "Other Agent".to_string(),
        status: AgentStatus::Pending,
        source_kind: None,
        batch_id: Some("work".to_string()),
        model: None,
        result: None,
        error: None,
        last_progress: None,
    });
    
    let (batches, agents) = chat.collect_cancelable_agents();
    assert_eq!(batches, vec!["work".to_string()]);
    assert!(agents.is_empty(), "batch cancel should cover the non-auto agent");
    }
    
    #[test]
    fn esc_router_cancels_active_auto_turn_streaming() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.active_task_ids.insert("turn-1".to_string());
    chat.bottom_pane.set_task_running(true);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelTask);
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    
    assert!(
        !chat.auto_state.is_active(),
        "Auto Drive should stop after cancelling the active turn",
    );
    }
    
    #[test]
    fn esc_requires_follow_up_after_canceling_agents() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.active_agents.push(AgentInfo {
        id: "agent-1".to_string(),
        name: "Agent 1".to_string(),
        status: AgentStatus::Running,
        source_kind: None,
        batch_id: Some("batch-1".to_string()),
        model: None,
        result: None,
        error: None,
        last_progress: None,
    });
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AutoStopActive);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    assert!(!chat.auto_state.is_active(), "Auto Drive stops before canceling agents");
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelAgents);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    assert!(!chat.auto_state.is_active());
    assert!(chat.has_cancelable_agents());
    assert!(chat.auto_state.last_run_summary.is_none());
    }
    
    #[test]
    fn cancel_agents_preserves_spinner_for_running_terminal_when_auto_inactive() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    let terminal_launch = TerminalLaunch {
        id: 42,
        title: "Terminal".to_string(),
        command: vec!["sleep".to_string(), "10".to_string()],
        command_display: "sleep 10".to_string(),
        controller: None,
        auto_close_on_success: false,
        start_running: true,
    };
    chat.terminal_open(&terminal_launch);
    
    chat.active_agents.push(AgentInfo {
        id: "agent-1".to_string(),
        name: "Agent 1".to_string(),
        status: AgentStatus::Running,
        source_kind: None,
        batch_id: Some("batch-1".to_string()),
        model: None,
        result: None,
        error: None,
        last_progress: None,
    });
    
    let mut route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::DismissModal);
    let mut attempts = 0;
    while route.intent == EscIntent::DismissModal && attempts < 3 {
        assert!(chat.execute_esc_intent(
            route.intent,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        ));
        route = chat.describe_esc_context();
        attempts += 1;
    }
    
    assert_eq!(route.intent, EscIntent::CancelAgents);
    assert!(chat.execute_esc_intent(
        route.intent,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
    ));
    
    assert!(!chat.auto_state.is_active(), "Auto Drive remains inactive");
    assert!(chat.has_cancelable_agents());
    chat.maybe_hide_spinner();
    assert!(
        chat.bottom_pane.is_task_running(),
        "Spinner stays active while agents or terminal work are still running",
    );
    }
    
    #[test]
    fn esc_cancels_agents_then_command_and_stops_auto_drive() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.active_agents.push(AgentInfo {
        id: "agent-1".to_string(),
        name: "Agent 1".to_string(),
        status: AgentStatus::Running,
        source_kind: None,
        batch_id: Some("batch-1".to_string()),
        model: None,
        result: None,
        error: None,
        last_progress: None,
    });
    
    chat.exec.running_commands.insert(
        ExecCallId("exec-1".to_string()),
        RunningCommand {
            command: vec!["echo".to_string(), "hi".to_string()],
            parsed: Vec::new(),
            history_index: None,
            history_id: None,
            explore_entry: None,
            stdout_offset: 0,
            stderr_offset: 0,
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
        },
    );
    chat.bottom_pane.set_task_running(true);
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelTask);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelAgents);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    assert!(!chat.auto_state.is_active(), "Auto Drive should stop after cancelling the command");
    assert!(chat.auto_state.last_run_summary.is_none());
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelAgents);
    }
    
    #[test]
    fn auto_disabled_cli_turn_preserves_send_prompt_label() {
    let mut harness = ChatWidgetHarness::new();
    harness.with_chat(|chat| {
        chat.config.auto_drive.coordinator_routing = false;
        chat.auto_state.continue_mode = AutoContinueMode::Immediate;
        chat.auto_state.goal = Some("Ship feature".to_string());
        chat.auto_state.set_phase(AutoRunPhase::Active);
        chat.schedule_auto_cli_prompt(0, "echo ready".to_string());
    });
    
    let (button_label, countdown_override, ctrl_switch_hint, manual_hint_present) =
        harness.with_chat(|chat| {
            let model = chat
                .bottom_pane
                .auto_view_model()
                .expect("auto coordinator view should be active");
            match model {
                AutoCoordinatorViewModel::Active(active) => (
                    active
                        .button
                        .as_ref()
                        .expect("button expected")
                        .label
                        .clone(),
                    chat.auto_state.countdown_override,
                    active.ctrl_switch_hint.clone(),
                    active.manual_hint.is_some(),
                ),
            }
        });
    
    assert!(button_label.starts_with("Send prompt"));
    assert_eq!(countdown_override, None);
    assert_eq!(ctrl_switch_hint.as_str(), "Esc to edit");
    assert!(manual_hint_present);
    
    harness.with_chat(|chat| {
        chat.auto_submit_prompt();
    });
    
    let auto_pending = harness.with_chat(|chat| chat.auto_pending_goal_request);
    assert!(!auto_pending);
    }
    
    #[test]
    fn auto_drive_view_marks_running_when_agents_active() {
    let mut harness = ChatWidgetHarness::new();
    harness.with_chat(|chat| {
        chat.auto_state.set_phase(AutoRunPhase::Active);
        chat.auto_state.goal = Some("Ship feature".to_string());
        chat.auto_rebuild_live_ring();
    });
    
    harness.handle_event(Event {
        id: "turn-1".to_string(),
        event_seq: 0,
        msg: EventMsg::AgentStatusUpdate(AgentStatusUpdateEvent {
            agents: vec![CoreAgentInfo {
                id: "agent-1".to_string(),
                name: "Worker".to_string(),
                status: "running".to_string(),
                batch_id: Some("batch-1".to_string()),
                model: None,
                last_progress: None,
                result: None,
                error: None,
                elapsed_ms: None,
                token_count: None,
                last_activity_at: None,
                seconds_since_last_activity: None,
                source_kind: None,
            }],
            context: None,
            task: None,
        }),
        order: None,
    });
    
    let cli_running = harness.with_chat(|chat| {
        chat
            .bottom_pane
            .auto_view_model()
            .map(|model| match model {
                AutoCoordinatorViewModel::Active(active) => active.cli_running,
            })
            .unwrap_or(false)
    });
    
    assert!(
        cli_running,
        "auto drive view should treat running agents as active"
    );
    }
    
    #[test]
    fn auto_drive_error_enters_transient_recovery() {
    let mut harness = ChatWidgetHarness::new();
    harness.with_chat(|chat| {
        chat.auto_state.set_phase(AutoRunPhase::Active);
        chat.auto_state.goal = Some("Ship feature".to_string());
        chat.auto_state.on_prompt_ready(true);
        chat.auto_rebuild_live_ring();
    });
    
    harness.handle_event(Event {
        id: "turn-1".to_string(),
        event_seq: 0,
        msg: EventMsg::Error(ErrorEvent {
            message: "internal error; agent loop died unexpectedly".to_string(),
        }),
        order: None,
    });
    
    let (still_active, in_recovery) = harness.with_chat(|chat| {
        (chat.auto_state.is_active(), chat.auto_state.in_transient_recovery())
    });
    assert!(
        still_active && in_recovery,
        "auto drive should pause for recovery after an error event"
    );
    }
    
    #[test]
    fn auto_bootstrap_starts_from_history() {
    let mut harness = ChatWidgetHarness::new();
    {
        let chat = harness.chat();
        chat.config.auto_drive.coordinator_routing = false;
        chat.config.sandbox_policy = SandboxPolicy::DangerFullAccess;
        chat.config.approval_policy = AskForApproval::Never;
    }
    
    {
        let chat = harness.chat();
        insert_plain_cell(chat, &["User: summarize recent progress"]);
        insert_plain_cell(chat, &["Assistant: Tests are passing, next step pending."]);
        chat.handle_auto_command(Some(String::new()));
    }
    
    let chat = harness.chat();
    assert!(chat.auto_pending_goal_request);
    assert!(!chat.auto_goal_bootstrap_done);
    assert_eq!(
        chat.auto_state.goal.as_deref(),
        Some(AUTO_BOOTSTRAP_GOAL_PLACEHOLDER)
    );
    assert!(chat.next_cli_text_format.is_none());
    let pending_prompt = chat
        .auto_state
        .current_cli_prompt
        .as_deref()
        .expect("bootstrap prompt");
    assert!(pending_prompt.trim().is_empty());
    }
    
    #[test]
    fn auto_bootstrap_updates_goal_after_first_decision() {
    let mut harness = ChatWidgetHarness::new();
    {
        let chat = harness.chat();
        chat.auto_state.set_phase(AutoRunPhase::Active);
        chat.auto_state.goal = Some(AUTO_BOOTSTRAP_GOAL_PLACEHOLDER.to_string());
        chat.auto_goal_bootstrap_done = false;
    }
    
    {
        let chat = harness.chat();
        chat.auto_handle_decision(AutoDecisionEvent {
            seq: 1,
            status: AutoCoordinatorStatus::Continue,
            status_title: None,
            status_sent_to_user: None,
            goal: Some("Finish migrations".to_string()),
            cli: Some(AutoTurnCliAction {
                prompt: "echo ready".to_string(),
                context: None,
                suppress_ui_context: false,
                model_override: None,
                reasoning_effort_override: None,
            }),
            agents_timing: None,
            agents: Vec::new(),
            transcript: Vec::new(),
        });
    }
    
    let chat = harness.chat();
    assert_eq!(chat.auto_state.goal.as_deref(), Some("Finish migrations"));
    assert!(chat.auto_goal_bootstrap_done);
    assert!(!chat.auto_pending_goal_request);
    assert_eq!(chat.auto_state.current_cli_prompt.as_deref(), Some("echo ready"));
    }
    
    #[test]
    fn auto_card_goal_updates_after_derivation() {
    let mut harness = ChatWidgetHarness::new();
    {
        let chat = harness.chat();
        chat.auto_state.set_phase(AutoRunPhase::Active);
        chat.auto_state.goal = Some(AUTO_BOOTSTRAP_GOAL_PLACEHOLDER.to_string());
        chat.auto_card_start(Some(AUTO_BOOTSTRAP_GOAL_PLACEHOLDER.to_string()));
    }
    
    {
        let chat = harness.chat();
        chat.auto_handle_decision(AutoDecisionEvent {
            seq: 2,
            status: AutoCoordinatorStatus::Continue,
            status_title: None,
            status_sent_to_user: None,
            goal: Some("Document release tasks".to_string()),
            cli: Some(AutoTurnCliAction {
                prompt: "echo start".to_string(),
                context: None,
                suppress_ui_context: false,
                model_override: None,
                reasoning_effort_override: None,
            }),
            agents_timing: None,
            agents: Vec::new(),
            transcript: Vec::new(),
        });
    }
    
    let chat = harness.chat();
    let tracker = chat
        .tools_state
        .auto_drive_tracker
        .as_ref()
        .expect("auto drive tracker should be present");
    assert_eq!(tracker.cell.goal_text(), Some("Document release tasks"));
    }
    
    #[test]
    fn auto_action_events_land_in_auto_drive_card() {
    let mut harness = ChatWidgetHarness::new();
    let note = "Retrying prompt generation after the previous response was too long to send to the CLI.";
    
    let chat = harness.chat();
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.auto_card_start(Some("Ship feature".to_string()));
    chat.auto_handle_action(note.to_string());
    
    let tracker = chat
        .tools_state
        .auto_drive_tracker
        .as_ref()
        .expect("auto drive tracker should be present");
    let actions = tracker.cell.action_texts();
    assert!(
        actions.iter().any(|text| text == note),
        "auto drive action card should record retry note"
    );
    }
    
    #[test]
    fn auto_compacted_history_without_notice_skips_checkpoint_banner() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    let conversation = vec![
        ChatWidget::auto_drive_make_assistant_message("overlong prompt raw output".to_string())
            .expect("assistant message"),
    ];
    
    chat.auto_handle_compacted_history(std::sync::Arc::from(conversation), false);
    
    let has_checkpoint = chat.history_cells.iter().any(|cell| {
        cell.display_lines_trimmed().iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains(COMPACTION_CHECKPOINT_MESSAGE))
        })
    });
    
    assert!(
        !has_checkpoint,
        "compaction notice should not be shown when show_notice is false"
    );
    }
    
    #[test]
    fn auto_card_shows_status_title_in_state_detail() {
    let mut harness = ChatWidgetHarness::new();
    {
        let chat = harness.chat();
        chat.auto_state.set_phase(AutoRunPhase::Active);
        chat.auto_state.goal = Some("Ship feature".to_string());
        chat.auto_card_start(Some("Ship feature".to_string()));
    }
    
    {
        let chat = harness.chat();
        chat.auto_handle_decision(AutoDecisionEvent {
            seq: 3,
            status: AutoCoordinatorStatus::Continue,
            status_title: Some("Drafting fix".to_string()),
            status_sent_to_user: Some("Past work".to_string()),
            goal: None,
            cli: Some(AutoTurnCliAction {
                prompt: "echo work".to_string(),
                context: None,
                suppress_ui_context: false,
                model_override: None,
                reasoning_effort_override: None,
            }),
            agents_timing: None,
            agents: Vec::new(),
            transcript: Vec::new(),
        });
    }
    
    let chat = harness.chat();
    let tracker = chat
        .tools_state
        .auto_drive_tracker
        .as_ref()
        .expect("auto drive tracker should be present");
    let actions = tracker.cell.action_texts();
    assert!(actions.iter().any(|text| text == "Status: Drafting fix"));
    }
    
    #[test]
    fn goal_entry_esc_sequence_preserves_draft_and_summary() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.last_run_summary = Some(AutoRunSummary {
        duration: Duration::from_secs(42),
        turns_completed: 3,
        message: Some("All tasks done.".to_string()),
        goal: Some("Finish feature".to_string()),
    });
    chat.auto_show_goal_entry_panel();
    chat.handle_paste("Suggested goal".to_string());
    assert!(matches!(
        chat.auto_goal_escape_state,
        AutoGoalEscState::NeedsEnableEditing
    ));
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AutoGoalEnableEdit);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    assert!(chat.auto_state.should_show_goal_entry());
    assert!(matches!(
        chat.auto_goal_escape_state,
        AutoGoalEscState::ArmedForExit
    ));
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AutoGoalExitPreserveDraft);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    assert!(!chat.auto_state.should_show_goal_entry());
    assert_eq!(chat.bottom_pane.composer_text(), "Suggested goal");
    assert!(chat.auto_state.last_run_summary.is_some());
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AutoDismissSummary);
    }
    
    #[test]
    fn goal_entry_typing_arms_escape_state() {
    let mut harness = ChatWidgetHarness::new();
    {
        let chat = harness.chat();
        chat.auto_show_goal_entry_panel();
    }
    
    harness.send_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    
    let chat = harness.chat();
    assert!(matches!(
        chat.auto_goal_escape_state,
        AutoGoalEscState::NeedsEnableEditing
    ));
    assert_eq!(chat.bottom_pane.composer_text(), "x");
    }
    
    #[test]
    fn ctrl_g_dispatches_external_editor_event() {
    let mut harness = ChatWidgetHarness::new();
    let key_event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
    harness.chat().handle_key_event(key_event);
    let events = harness.drain_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AppEvent::OpenExternalEditor { .. })),
        "expected external editor request on Ctrl+G",
    );
    }
    
    #[test]
    fn goal_entry_esc_exits_immediately_without_suggestion() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_show_goal_entry_panel();
    assert!(chat.auto_state.should_show_goal_entry());
    assert!(matches!(chat.auto_goal_escape_state, AutoGoalEscState::Inactive));
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AutoGoalExitPreserveDraft);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    
    assert!(!chat.auto_state.should_show_goal_entry());
    assert_eq!(chat.bottom_pane.composer_text(), "");
    }
    
    #[test]
    fn esc_unwinds_cli_before_stopping_auto() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    let call_id = ExecCallId("exec-1".to_string());
    chat.exec.running_commands.insert(
        call_id,
        RunningCommand {
            command: vec!["echo".to_string()],
            parsed: Vec::new(),
            history_index: None,
            history_id: None,
            explore_entry: None,
            stdout_offset: 0,
            stderr_offset: 0,
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
        },
    );
    chat.bottom_pane.set_task_running(true);
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelTask);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    assert!(
        !chat.auto_state.is_active(),
        "Auto Drive now stops immediately after cancelling the CLI task",
    );
    
    chat.exec.running_commands.clear();
    chat.bottom_pane.set_task_running(false);
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AutoGoalExitPreserveDraft);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    }
    
    #[test]
    fn esc_router_cancels_running_task() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.active_task_ids.insert("turn-1".to_string());
    chat.bottom_pane.set_task_running(true);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelTask);
    }
    
    #[test]
    fn esc_cancel_task_while_manual_command_does_not_trigger_auto_drive() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.exec.running_commands.insert(
        ExecCallId("exec-1".to_string()),
        RunningCommand {
            command: vec!["echo".to_string()],
            parsed: Vec::new(),
            history_index: None,
            history_id: None,
            explore_entry: None,
            stdout_offset: 0,
            stderr_offset: 0,
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
        },
    );
    chat.bottom_pane.set_task_running(true);
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::CancelTask);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    assert!(
        !chat.auto_state.is_active(),
        "Auto Drive should remain inactive after cancelling manual command",
    );
    assert!(
        chat.auto_state.last_run_summary.is_none(),
        "Cancelling manual command should not create an Auto Drive summary",
    );
    }
    
    #[test]
    fn esc_router_handles_diff_confirm_prompt() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.diffs.confirm = Some(crate::chatwidget::diff_ui::DiffConfirm {
        text_to_submit: "Please undo".to_string(),
    });
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::DiffConfirm);
    
    let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    assert!(chat.execute_esc_intent(route.intent, esc_event));
    assert!(chat.diffs.confirm.is_none(), "diff confirm should clear after Esc");
    }
    
    #[test]
    fn esc_router_handles_agents_terminal_overlay() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.agents_terminal.active = true;
    chat.agents_terminal.focus_detail();
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AgentsTerminal);
    }
    
    #[test]
    fn esc_router_clears_manual_entry_input() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_show_goal_entry_panel();
    assert!(chat.auto_state.should_show_goal_entry());
    chat.bottom_pane.insert_str("draft goal");
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::AutoGoalExitPreserveDraft);
    }
    
    #[test]
    fn esc_router_defaults_to_show_hint_when_idle() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    let route = chat.describe_esc_context();
    assert_eq!(route.intent, EscIntent::ShowUndoHint);
    assert!(route.allows_double_esc);
    }
    
    #[test]
    fn reasoning_collapse_hides_intermediate_titles_in_consecutive_runs() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.config.tui.show_reasoning = false;
    
    let agent_cell = history_cell::AgentRunCell::new("Batch".to_string());
    chat.history_push(agent_cell);
    
    let reasoning_one = history_cell::CollapsibleReasoningCell::new_with_id(
        vec![Line::from("First reasoning".to_string())],
        Some("r1".to_string()),
    );
    let reasoning_two = history_cell::CollapsibleReasoningCell::new_with_id(
        vec![Line::from("Second reasoning".to_string())],
        Some("r2".to_string()),
    );
    
    chat.history_push(reasoning_one);
    chat.history_push(reasoning_two);
    
    chat.refresh_reasoning_collapsed_visibility();
    
    let reasoning_cells: Vec<&history_cell::CollapsibleReasoningCell> = chat
        .history_cells
        .iter()
        .filter_map(|cell| {
            cell.as_any()
                .downcast_ref::<history_cell::CollapsibleReasoningCell>()
        })
        .collect();
    
    assert_eq!(reasoning_cells.len(), 2, "expected exactly two reasoning cells");
    
    assert!(
        reasoning_cells[0].display_lines().is_empty(),
        "intermediate reasoning should hide when collapsed after agent anchor",
    );
    assert!(
        !reasoning_cells[1].display_lines().is_empty(),
        "last reasoning should remain visible",
    );
    }
    
    #[test]
    fn reasoning_collapse_applies_without_anchor_cells() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.config.tui.show_reasoning = false;
    
    let reasoning_one = history_cell::CollapsibleReasoningCell::new_with_id(
        vec![Line::from("First reasoning".to_string())],
        Some("r1".to_string()),
    );
    let reasoning_two = history_cell::CollapsibleReasoningCell::new_with_id(
        vec![Line::from("Second reasoning".to_string())],
        Some("r2".to_string()),
    );
    
    chat.history_push(reasoning_one);
    chat.history_push(reasoning_two);
    
    chat.refresh_reasoning_collapsed_visibility();
    
    let reasoning_cells: Vec<&history_cell::CollapsibleReasoningCell> = chat
        .history_cells
        .iter()
        .filter_map(|cell| {
            cell.as_any()
                .downcast_ref::<history_cell::CollapsibleReasoningCell>()
        })
        .collect();
    
    assert_eq!(reasoning_cells.len(), 2, "expected exactly two reasoning cells");
    
    assert!(
        reasoning_cells[0].display_lines().is_empty(),
        "intermediate reasoning should hide when collapsed without an anchor",
    );
    assert!(
        !reasoning_cells[1].display_lines().is_empty(),
        "last reasoning should remain visible",
    );
    }
    
    #[test]
    fn auto_drive_stays_paused_while_auto_resolve_pending_fix() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.auto_state.on_prompt_submitted();
    chat.auto_state.review_enabled = true;
    chat.auto_state.on_complete_review();
    chat.auto_state.set_waiting_for_response(true);
    chat.pending_turn_descriptor = None;
    chat.pending_auto_turn_config = None;
    chat.auto_resolve_state = Some(make_pending_fix_state(ReviewOutputEvent::default()));
    
    chat.auto_on_assistant_final();
    
    // With cloud-gpt-5.1-codex-max gated off, the review request is still queued but
    // may be processed synchronously; ensure the review slot was populated.
    if chat.auto_state.awaiting_review() {
        // Review remains pending; nothing else to assert.
    } else {
        assert!(chat.auto_state.current_cli_prompt.is_some());
    }
    assert!(!chat.auto_state.is_waiting_for_response());
    }
    
    #[test]
    fn auto_review_skip_resumes_auto_drive() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.auto_state.review_enabled = true;
    chat.auto_state.on_prompt_submitted();
    chat.auto_state.set_waiting_for_response(true);
    chat.auto_state.on_complete_review();
    chat.auto_state.set_waiting_for_response(true);
    
    let turn_config = TurnConfig {
        read_only: false,
        complexity: Some(TurnComplexity::Low),
        text_format_override: None,
    };
    chat.pending_auto_turn_config = Some(turn_config.clone());
    chat.pending_turn_descriptor = Some(TurnDescriptor {
        mode: TurnMode::Normal,
        read_only: false,
        complexity: Some(TurnComplexity::Low),
        agent_preferences: None,
        review_strategy: None,
        text_format_override: None,
    });
    
    let base_id = "base-commit".to_string();
    let final_id = "final-commit".to_string();
    
    chat.auto_turn_review_state = Some(AutoTurnReviewState {
        base_commit: Some(GhostCommit::new(base_id.clone(), None)),
    });
    
    let base_for_capture = base_id.clone();
    let final_for_capture = final_id.clone();
    let _capture_guard = CaptureCommitStubGuard::install(move |message, parent| {
        assert_eq!(message, "auto turn change snapshot");
        assert_eq!(parent.as_deref(), Some(base_for_capture.as_str()));
        Ok(GhostCommit::new(final_for_capture.clone(), parent))
    });
    
    let base_for_diff = base_id;
    let final_for_diff = final_id;
    let _diff_guard = GitDiffStubGuard::install(move |base, head| {
        assert_eq!(base, base_for_diff);
        assert_eq!(head, final_for_diff);
        Ok(Vec::new())
    });
    
    chat.auto_on_assistant_final();
    assert!(chat.auto_state.awaiting_review(), "post-turn review should be pending");
    
    let descriptor_snapshot = chat.pending_turn_descriptor.clone();
    chat.auto_handle_post_turn_review(turn_config, descriptor_snapshot.as_ref());
    
    assert!(
        !chat.auto_state.awaiting_review(),
        "auto drive should clear waiting flag after skipped review"
    );
    
    let skip_banner = "Auto review skipped: no file changes detected this turn.";
    let skip_present = chat.history_cells.iter().any(|cell| {
        cell.display_lines_trimmed().iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains(skip_banner))
        })
    });
    assert!(skip_present, "skip banner should appear in history");
    
    assert!(
        !chat.auto_state.is_waiting_for_response(),
        "auto drive should resume conversation after skipped review"
    );
    }
    
    #[test]
    fn auto_review_skip_stays_blocked_when_auto_resolve_pending() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.auto_state.review_enabled = true;
    chat.auto_state.on_prompt_submitted();
    chat.auto_state.on_complete_review();
    chat.auto_state.set_waiting_for_response(true);
    
    let turn_config = TurnConfig {
        read_only: false,
        complexity: Some(TurnComplexity::Low),
        text_format_override: None,
    };
    chat.pending_auto_turn_config = Some(turn_config.clone());
    chat.pending_turn_descriptor = Some(TurnDescriptor {
        mode: TurnMode::Normal,
        read_only: false,
        complexity: Some(TurnComplexity::Low),
        agent_preferences: None,
        review_strategy: None,
        text_format_override: None,
    });
    
    let base_id = "base-commit".to_string();
    let final_id = "final-commit".to_string();
    
    chat.auto_turn_review_state = Some(AutoTurnReviewState {
        base_commit: Some(GhostCommit::new(base_id.clone(), None)),
    });
    
    chat.auto_resolve_state = Some(make_pending_fix_state(ReviewOutputEvent::default()));
    
    let base_for_capture = base_id.clone();
    let final_for_capture = final_id.clone();
    let _capture_guard = CaptureCommitStubGuard::install(move |message, parent| {
        assert_eq!(message, "auto turn change snapshot");
        assert_eq!(parent.as_deref(), Some(base_for_capture.as_str()));
        Ok(GhostCommit::new(final_for_capture.clone(), parent))
    });
    
    let base_for_diff = base_id;
    let final_for_diff = final_id;
    let _diff_guard = GitDiffStubGuard::install(move |base, head| {
        assert_eq!(base, base_for_diff);
        assert_eq!(head, final_for_diff);
        Ok(Vec::new())
    });
    
    chat.auto_on_assistant_final();
    assert!(chat.auto_state.awaiting_review(), "auto-resolve should block resume before skip");
    
    let descriptor_snapshot = chat.pending_turn_descriptor.clone();
    chat.auto_handle_post_turn_review(turn_config, descriptor_snapshot.as_ref());
    
    assert!(
        chat.auto_state.awaiting_review(),
        "auto drive should remain waiting when auto-resolve blocks"
    );
    assert!(
        !chat.auto_state.is_waiting_for_response(),
        "skip should not resume coordinator when auto-resolve blocks"
    );
    }
    
    #[test]
    fn auto_resolve_limit_zero_runs_single_fix_cycle() {
    let _runtime_guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.config.tui.review_auto_resolve = true;
    chat.config.auto_drive.auto_resolve_review_attempts =
        AutoResolveAttemptLimit::try_new(0).unwrap();
    
    chat.start_review_with_scope(
        ReviewTarget::UncommittedChanges,
        "Review workspace".to_string(),
        Some("workspace".to_string()),
        Some("Preparing code review request...".to_string()),
        true,
    );
    
    let state = chat
        .auto_resolve_state
        .as_ref()
        .expect("limit 0 should still initialize auto-resolve state");
    assert_eq!(state.max_attempts, 0);
    
    chat.auto_resolve_handle_review_enter();
    
    let review = ReviewOutputEvent {
        findings: vec![ReviewFinding {
            title: "issue".to_string(),
            body: "details".to_string(),
            confidence_score: 0.6,
            priority: 1,
            code_location: ReviewCodeLocation {
                absolute_file_path: PathBuf::from("src/lib.rs"),
                line_range: ReviewLineRange { start: 1, end: 1 },
            },
        }],
        overall_correctness: "incorrect".to_string(),
        overall_explanation: "needs follow up".to_string(),
        overall_confidence_score: 0.6,
    };
    
    chat.auto_resolve_handle_review_exit(Some(review.clone()));
    assert!(
        matches!(
            chat.auto_resolve_state
                .as_ref()
                .map(|state| &state.phase),
            Some(AutoResolvePhase::PendingFix { .. })
        ),
        "limit 0 should still request an automated fix"
    );
    
    chat.auto_resolve_on_task_complete(Some("fix applied".to_string()));
    assert!(
        matches!(
            chat.auto_resolve_state
                .as_ref()
                .map(|state| &state.phase),
            Some(AutoResolvePhase::AwaitingFix { .. })
        ),
        "auto-resolve should wait for judge after fix"
    );
    
    chat.auto_resolve_on_task_complete(Some("ready for judge".to_string()));
    assert!(
        matches!(
            chat.auto_resolve_state
                .as_ref()
                .map(|state| &state.phase),
            Some(AutoResolvePhase::AwaitingJudge { .. })
        ),
        "auto-resolve should request a status check"
    );
    
    chat.auto_resolve_process_judge(
        review,
        r#"{"status":"review_again","rationale":"double-check"}"#.to_string(),
    );
    
    assert!(
        chat.auto_resolve_state.is_none(),
        "automation should halt after judge when limit is zero"
    );
    
    let attempts_string_present = chat.history_cells.iter().any(|cell| {
        cell.display_lines_trimmed().iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("attempt 1 of 0"))
        })
    });
    assert!(
        !attempts_string_present,
        "history should not mention impossible attempt counts"
    );
    }
    
    #[test]
    fn auto_resolve_limit_one_stops_after_single_retry() {
    let _runtime_guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.config.tui.review_auto_resolve = true;
    chat.config.auto_drive.auto_resolve_review_attempts =
        AutoResolveAttemptLimit::try_new(1).unwrap();
    
    chat.start_review_with_scope(
        ReviewTarget::UncommittedChanges,
        "Review workspace".to_string(),
        Some("workspace".to_string()),
        Some("Preparing code review request...".to_string()),
        true,
    );
    
    assert_eq!(
        chat.auto_resolve_state.as_ref().map(|state| state.max_attempts),
        Some(1),
        "auto-resolve state should honor configured limit"
    );
    
    chat.auto_resolve_handle_review_enter();
    
    let review = ReviewOutputEvent {
        findings: vec![ReviewFinding {
            title: "issue".to_string(),
            body: "details".to_string(),
            confidence_score: 0.6,
            priority: 1,
            code_location: ReviewCodeLocation {
                absolute_file_path: PathBuf::from("src/lib.rs"),
                line_range: ReviewLineRange { start: 1, end: 1 },
            },
        }],
        overall_correctness: "incorrect".to_string(),
        overall_explanation: "needs follow up".to_string(),
        overall_confidence_score: 0.6,
    };
    
    chat.auto_resolve_handle_review_exit(Some(review.clone()));
    assert!(
        matches!(
            chat.auto_resolve_state.as_ref().map(|state| &state.phase),
            Some(AutoResolvePhase::PendingFix { .. })
        ),
        "auto-resolve should request a fix after first findings"
    );
    
    chat.auto_resolve_on_task_complete(Some("fix applied".to_string()));
    chat.auto_resolve_process_judge(
        review.clone(),
        r#"{"status":"review_again","rationale":"double-check"}"#.to_string(),
    );
    
    let state = chat
        .auto_resolve_state
        .as_ref()
        .expect("limit 1 should schedule a single re-review");
    assert!(matches!(state.phase, AutoResolvePhase::WaitingForReview));
    
    chat.auto_resolve_handle_review_enter();
    chat.auto_resolve_handle_review_exit(Some(review));
    
    assert!(
        chat.auto_resolve_state.is_none(),
        "automation should halt after completing the allowed re-review"
    );
    
    let mut history_strings = Vec::new();
    for cell in &chat.history_cells {
        for line in cell.display_lines_trimmed() {
            for span in &line.spans {
                history_strings.push(span.content.to_string());
            }
        }
    }
    
    let attempt_limit_notice_present = history_strings
        .iter()
        .any(|line| line.contains("attempt limit") && line.contains("reached"));
    assert!(
        attempt_limit_notice_present,
        "user should be notified when the attempt limit stops automation"
    );
    
    assert!(
        history_strings
            .iter()
            .all(|line| !line.contains("attempt 1 of 0")),
        "no messaging should reference impossible attempt counts"
    );
    }
    
    #[test]
    fn auto_handle_decision_launches_cli_agents_and_review() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.auto_state.review_enabled = true;
    chat.config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    
    chat.auto_handle_decision(AutoDecisionEvent {
        seq: 4,
        status: AutoCoordinatorStatus::Continue,
        status_title: Some("Running unit tests".to_string()),
        status_sent_to_user: Some("Finished setup".to_string()),
        goal: Some("Refine goal".to_string()),
        cli: Some(AutoTurnCliAction {
            prompt: "Run cargo test".to_string(),
            context: Some("use --all-features".to_string()),
            suppress_ui_context: false,
            model_override: None,
            reasoning_effort_override: None,
        }),
        agents_timing: Some(AutoTurnAgentsTiming::Parallel),
        agents: vec![AutoTurnAgentsAction {
            prompt: "Draft alternative fix".to_string(),
            context: None,
            write: false,
            write_requested: Some(false),
            models: None,
        }],
        transcript: Vec::new(),
    });
    
    assert_eq!(
        chat.auto_state.current_cli_prompt.as_deref(),
        Some("Run cargo test")
    );
    assert!(!chat.auto_state.awaiting_review());
    assert_eq!(chat.auto_state.pending_agent_actions.len(), 1);
    assert_eq!(
        chat.auto_state.pending_agent_timing,
        Some(AutoTurnAgentsTiming::Parallel)
    );
    let action = &chat.auto_state.pending_agent_actions[0];
    assert_eq!(action.prompt, "Draft alternative fix");
    assert!(action.write);
    
    let notice = "Auto Drive enabled write mode";
    let write_notice_present = chat
        .history_cells
        .iter()
        .any(|cell| {
            cell.display_lines_trimmed().iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.content.contains(notice))
            })
        });
    assert!(write_notice_present);
    }
    
    #[test]
    fn coordinator_router_emits_notice_for_status_question() {
    let mut harness = ChatWidgetHarness::new();
    {
        let chat = harness.chat();
    chat.auto_state.set_phase(AutoRunPhase::Active);
        chat.config.auto_drive.coordinator_routing = true;
        chat.config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    }
    
    let baseline_notice_count = {
        let chat = harness.chat();
        chat.history_cells
            .iter()
            .filter(|cell| matches!(cell.kind(), HistoryCellType::Notice))
            .count()
    };
    
    {
        let chat = harness.chat();
        chat.auto_handle_user_reply(
            Some("Two active agents reporting steady progress.".to_string()),
            None,
        );
    }
    
    let notice_count = {
        let chat = harness.chat();
        chat.history_cells
            .iter()
            .filter(|cell| matches!(cell.kind(), HistoryCellType::Notice))
            .count()
    };
    assert!(notice_count > baseline_notice_count);
    
    let header_span = {
        let chat = harness.chat();
        let notice_cell = chat
            .history_cells
            .iter()
            .rev()
            .find(|cell| matches!(cell.kind(), HistoryCellType::Notice))
            .expect("notice cell");
        let lines = notice_cell.display_lines_trimmed();
        assert!(!lines.is_empty());
        lines
            .first()
            .and_then(|line| line.spans.first())
            .map(|span| span.content.to_string())
            .unwrap_or_default()
    };
    assert_eq!(header_span, "AUTO DRIVE RESPONSE");
    }
    
    #[test]
    fn coordinator_router_injects_cli_for_plan_requests() {
    let mut harness = ChatWidgetHarness::new();
    {
        let chat = harness.chat();
    chat.auto_state.set_phase(AutoRunPhase::Active);
        chat.config.auto_drive.coordinator_routing = true;
        chat.config.sandbox_policy = SandboxPolicy::DangerFullAccess;
    }
    
    harness.drain_events();
    
    {
        let chat = harness.chat();
        chat.auto_handle_user_reply(None, Some("/plan".to_string()));
    }
    
    let events = harness.drain_events();
    let (command, payload) = events
        .iter()
        .find_map(|event| match event {
            AppEvent::DispatchCommand(cmd, payload) => Some((cmd, payload.clone())),
            _ => None,
        })
        .expect("dispatch for /plan");
    assert_eq!(*command, SlashCommand::Auto);
    assert!(payload.contains("/plan"), "payload={payload}");
    }
    
    #[test]
    fn coordinator_router_bypasses_slash_commands() {
    let mut harness = ChatWidgetHarness::new();
    {
        let chat = harness.chat();
    chat.auto_state.set_phase(AutoRunPhase::Active);
        chat.config.auto_drive.coordinator_routing = true;
    }
    
    harness.drain_events();
    {
        let chat = harness.chat();
        chat.submit_user_message(UserMessage::from("/status".to_string()));
    }
    
    let events = harness.drain_events();
    assert!(
        events.iter().any(|event| matches!(event, AppEvent::DispatchCommand(_, _))
            || matches!(event, AppEvent::CodexOp(_))),
        "slash command should follow existing dispatch path"
    );
    }
    
    #[test]
    fn build_turn_message_includes_agent_guidance() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    chat.auto_state.subagents_enabled = true;
    chat.auto_state.pending_agent_actions = vec![AutoTurnAgentsAction {
        prompt: "Draft alternative fix".to_string(),
        context: Some("Focus on parser module".to_string()),
        write: false,
        write_requested: Some(false),
        models: Some(vec![
            "claude-sonnet-4.5".to_string(),
            "gemini-3-pro".to_string(),
        ]),
    }];
    chat.auto_state.pending_agent_timing = Some(AutoTurnAgentsTiming::Blocking);
    
    chat.auto_state.current_cli_context = Some("Workspace root: /tmp".to_string());
    
    let message = chat
        .build_auto_turn_message("Run diagnostics")
        .expect("message");
    assert!(message.contains("Workspace root: /tmp"));
    assert!(message.contains("Run diagnostics"));
    assert!(message.contains("Please run agent.create"));
    assert!(message.contains("write: false"));
    assert!(message.contains("Models: [claude-sonnet-4.5, gemini-3-pro]"));
    assert!(message.contains("Draft alternative fix"));
    assert!(message.contains("Focus on parser module"));
    assert!(message.contains("agent.wait"));
    assert!(message.contains("Timing (blocking)"));
    assert!(message.contains("Launch these agents first"));
    assert!(!message.contains("agent {\"action\""), "message should not include raw agent JSON");
    }
    
    #[test]
    fn task_complete_triggers_review_when_waiting_flag_set() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    
    let _stub_lock = AUTO_STUB_LOCK.lock().unwrap();
    
    chat.auto_state.set_phase(AutoRunPhase::Active);
    chat.auto_state.review_enabled = true;
    chat.auto_state.on_prompt_submitted();
    
    let turn_config = TurnConfig {
        read_only: false,
        complexity: Some(TurnComplexity::Low),
        text_format_override: None,
    };
    chat.pending_auto_turn_config = Some(turn_config.clone());
    chat.pending_turn_descriptor = Some(TurnDescriptor {
        mode: TurnMode::Normal,
        read_only: false,
        complexity: Some(TurnComplexity::Low),
        agent_preferences: None,
        review_strategy: None,
        text_format_override: None,
    });
    
    let base_id = "base-commit".to_string();
    let final_id = "final-commit".to_string();
    
    chat.auto_turn_review_state = Some(AutoTurnReviewState {
        base_commit: Some(GhostCommit::new(base_id.clone(), None)),
    });
    
    let base_for_capture = base_id.clone();
    let final_for_capture = final_id.clone();
    let _capture_guard = CaptureCommitStubGuard::install(move |message, parent| {
        assert_eq!(message, "auto turn change snapshot");
        assert_eq!(parent.as_deref(), Some(base_for_capture.as_str()));
        Ok(GhostCommit::new(final_for_capture.clone(), parent))
    });
    
    let base_for_diff = base_id;
    let final_for_diff = final_id;
    let _diff_guard = GitDiffStubGuard::install(move |base, head| {
        assert_eq!(base, base_for_diff);
        assert_eq!(head, final_for_diff);
        Ok(Vec::new())
    });
    
    chat.auto_on_assistant_final();
    assert!(chat.auto_state.awaiting_review());
    
    let descriptor_snapshot = chat.pending_turn_descriptor.clone();
    chat.auto_handle_post_turn_review(turn_config, descriptor_snapshot.as_ref());
    
    chat.handle_code_event(Event {
        id: "turn".to_string(),
        event_seq: 42,
        msg: EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: None,
        }),
        order: None,
    });
    
    assert!(
        !chat.auto_state.awaiting_review(),
        "waiting flag should clear after TaskComplete launches skip review"
    );
    
    let skip_banner = "Auto review skipped: no file changes detected this turn.";
    let skip_present = chat.history_cells.iter().any(|cell| {
        cell.display_lines_trimmed().iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains(skip_banner))
        })
    });
    assert!(skip_present, "skip banner should appear after review skip");
    }
    
    #[test]
    fn finalize_explore_updates_even_with_stale_index() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    let call_id = "call-explore".to_string();
    let order = OrderMeta {
        request_ordinal: 1,
        output_index: Some(0),
        sequence_number: Some(0),
    };
    
    chat.handle_code_event(Event {
        id: call_id.clone(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "cat foo.txt".into()],
            cwd: std::env::temp_dir(),
            parsed_cmd: vec![ParsedCommand::Read {
                cmd: "cat foo.txt".to_string(),
                name: "foo.txt".to_string(),
            }],
            parent_call_id: None,
        }),
        order: Some(order),
    });

    let exec_call_id = ExecCallId(call_id);
    let running = chat
        .exec
        .running_commands
        .get_mut(&exec_call_id)
        .expect("explore command should be tracked");
    let (agg_idx, entry_idx) = running
        .explore_entry
        .expect("read command should register an explore entry");
    
    // Simulate an out-of-date index so finalize must recover by searching.
    running.explore_entry = Some((usize::MAX, entry_idx));
    chat.exec.running_explore_agg_index = Some(usize::MAX);
    
    chat.finalize_all_running_due_to_answer();
    
    let cell = chat.history_cells[agg_idx]
        .as_any()
        .downcast_ref::<ExploreAggregationCell>()
        .expect("explore aggregation cell should remain present");
    let entry = cell
        .record()
        .entries
        .get(entry_idx)
        .expect("entry index should still be valid");
    assert!(
        !matches!(entry.status, history_cell::ExploreEntryStatus::Running),
        "explore entry should not remain running after finalize_all_running_due_to_answer"
    );
    assert!(
        !chat.exec.running_commands.contains_key(&exec_call_id),
        "finalization should clear the running command"
    );
    }
    
    #[test]
    fn ordering_keeps_new_answers_after_prior_backgrounds() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    chat.last_seen_request_index = 1;
    chat.current_request_index = 1;
    chat.internal_seq = 0;
    
    chat.push_background_tail("background-one".to_string());
    chat.push_background_tail("background-two".to_string());
    
    assert_eq!(chat.history_cells.len(), 2, "expected two background cells");
    
    let answer_id = "answer-turn-1";
    let seeded_key = OrderKey {
        req: 1,
        out: 1,
        seq: 0,
    };
    chat.seed_stream_order_key(StreamKind::Answer, answer_id, seeded_key);
    
    let response_text = "assistant-response";
    chat.insert_final_answer_with_id(
        Some(answer_id.to_string()),
        vec![Line::from(response_text)],
        response_text.to_string(),
    );
    
    assert_eq!(chat.history_cells.len(), 3, "expected assistant cell to be added");
    
    let tail_kinds: Vec<HistoryCellType> = chat
        .history_cells
        .iter()
        .map(super::super::history_cell::HistoryCell::kind)
        .collect();
    
    let len = tail_kinds.len();
    assert_eq!(
        &tail_kinds[len - 3..],
        &[
            HistoryCellType::BackgroundEvent,
            HistoryCellType::BackgroundEvent,
            HistoryCellType::Assistant,
        ],
        "assistant output should appear after existing background cells",
    );
    }
    
    #[test]
    fn final_answer_clears_spinner_when_agent_never_reports_terminal_status() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    let turn_id = "turn-1".to_string();
    
    chat.handle_code_event(Event {
        id: turn_id.clone(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: None,
    });
    
    chat.handle_code_event(Event {
        id: turn_id.clone(),
        event_seq: 1,
        msg: EventMsg::AgentStatusUpdate(AgentStatusUpdateEvent {
            agents: vec![CoreAgentInfo {
                id: "agent-1".to_string(),
                name: "Todo Agent".to_string(),
                status: "running".to_string(),
                batch_id: Some("batch-single".to_string()),
                model: None,
                last_progress: None,
                result: None,
                error: None,
                elapsed_ms: None,
                token_count: None,
                last_activity_at: None,
                seconds_since_last_activity: None,
                source_kind: None,
            }],
            context: None,
            task: None,
        }),
        order: None,
    });
    assert!(
        chat.bottom_pane.is_task_running(),
        "spinner should remain active while the agent reports running"
    );
    
    chat.handle_code_event(Event {
        id: turn_id.clone(),
        event_seq: 2,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Completed todo items.".to_string(),
        }),
        order: None,
    });
    assert!(
        chat.bottom_pane.is_task_running(),
        "spinner should remain active after an assistant message until TaskComplete"
    );
    
    assert_eq!(chat.overall_task_status, "running".to_string());
    
    chat.handle_code_event(Event {
        id: turn_id,
        event_seq: 3,
        msg: EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: None,
        }),
        order: None,
    });
    
    assert!(
        !chat.bottom_pane.is_task_running(),
        "spinner should clear on TaskComplete even when agent runtime is missing"
    );
    
    assert_eq!(chat.overall_task_status, "complete".to_string());
    
    assert!(
        chat
            .agent_runtime
            .values()
            .all(|rt| rt.completed_at.is_none()),
        "runtime should remain incomplete until backend reports a terminal status"
    );
    
    assert!(
        chat
            .active_agents
            .iter()
            .all(|agent| !matches!(agent.status, AgentStatus::Pending | AgentStatus::Running)),
        "agents should be forced into a terminal status after the answer completes"
    );
    }
    
    #[test]
    fn spinner_rearms_when_late_agent_update_reports_running() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    let turn_id = "turn-1".to_string();
    
    chat.handle_code_event(Event {
        id: turn_id.clone(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: None,
    });
    
    chat.handle_code_event(Event {
        id: turn_id.clone(),
        event_seq: 1,
        msg: EventMsg::AgentStatusUpdate(AgentStatusUpdateEvent {
            agents: vec![CoreAgentInfo {
                id: "agent-1".to_string(),
                name: "Todo Agent".to_string(),
                status: "running".to_string(),
                batch_id: Some("batch-single".to_string()),
                model: None,
                last_progress: None,
                result: None,
                error: None,
                elapsed_ms: None,
                token_count: None,
                last_activity_at: None,
                seconds_since_last_activity: None,
                source_kind: None,
            }],
            context: None,
            task: None,
        }),
        order: None,
    });
    
    assert!(chat.bottom_pane.is_task_running(), "spinner should be running initially");
    
    chat.handle_code_event(Event {
        id: turn_id.clone(),
        event_seq: 2,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Completed todo items.".to_string(),
        }),
        order: None,
    });
    
    assert!(chat.bottom_pane.is_task_running(), "spinner stays running after assistant message");
    
    chat.handle_code_event(Event {
        id: turn_id.clone(),
        event_seq: 3,
        msg: EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: None,
        }),
        order: None,
    });
    
    assert!(
        !chat.bottom_pane.is_task_running(),
        "TaskComplete should clear the spinner"
    );
    
    chat.handle_code_event(Event {
        id: turn_id,
        event_seq: 4,
        msg: EventMsg::AgentStatusUpdate(AgentStatusUpdateEvent {
            agents: vec![CoreAgentInfo {
                id: "agent-1".to_string(),
                name: "Todo Agent".to_string(),
                status: "running".to_string(),
                batch_id: Some("batch-single".to_string()),
                model: None,
                last_progress: None,
                result: None,
                error: None,
                elapsed_ms: None,
                token_count: None,
                last_activity_at: None,
                seconds_since_last_activity: None,
                source_kind: None,
            }],
            context: None,
            task: None,
        }),
        order: None,
    });
    
    assert!(
        chat.bottom_pane.is_task_running(),
        "late running update should re-enable the spinner"
    );
    }

    #[test]
    fn answer_delta_strips_citation_markup_from_stream_preview() {
    let mut harness = ChatWidgetHarness::new();
    harness.with_chat(reset_history);

    harness.handle_event(Event {
        id: "turn-1".to_string(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: None,
    });
    harness.handle_event(Event {
        id: "answer-1".to_string(),
        event_seq: 1,
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "Visible <oai-mem-citation>doc-1</oai-mem-citation> text".to_string(),
        }),
        order: Some(OrderMeta {
            request_ordinal: 1,
            output_index: Some(0),
            sequence_number: Some(0),
        }),
    });
    harness.flush_into_widget();

    harness.with_chat(|chat| {
        let state = chat
            .history_state
            .assistant_stream_state("answer-1")
            .cloned()
            .expect("expected stream state");
        assert!(
            !state.preview_markdown.contains("<oai-mem-citation>"),
            "stream preview should hide citation tags"
        );
        assert!(state.preview_markdown.contains("Visible"));
        assert!(state.preview_markdown.contains("text"));
        assert_eq!(
            state.metadata.as_ref().map(|meta| meta.citations.clone()),
            Some(vec!["doc-1".to_string()]),
        );
    });
    }

    #[test]
    fn turn_aborted_keeps_proposed_plan_extracted_from_stream_deltas() {
    let mut harness = ChatWidgetHarness::new();
    harness.with_chat(|chat| {
        reset_history(chat);
        chat.collaboration_mode = code_core::protocol::CollaborationModeKind::Plan;
    });

    harness.handle_event(Event {
        id: "turn-1".to_string(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: None,
    });
    harness.handle_event(Event {
        id: "answer-1".to_string(),
        event_seq: 1,
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "Before\n<proposed_plan>\n- step".to_string(),
        }),
        order: Some(OrderMeta {
            request_ordinal: 1,
            output_index: Some(0),
            sequence_number: Some(0),
        }),
    });
    harness.handle_event(Event {
        id: "answer-1".to_string(),
        event_seq: 2,
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: " one\n</proposed_plan>\nAfter".to_string(),
        }),
        order: Some(OrderMeta {
            request_ordinal: 1,
            output_index: Some(0),
            sequence_number: Some(1),
        }),
    });
    harness.handle_event(Event {
        id: "turn-1".to_string(),
        event_seq: 3,
        msg: EventMsg::TurnAborted(TurnAbortedEvent {
            reason: TurnAbortReason::Interrupted,
        }),
        order: None,
    });
    harness.flush_into_widget();

    harness.with_chat(|chat| {
        let has_plan = chat.history_state.records.iter().any(|record| {
            matches!(record, HistoryRecord::ProposedPlan(state) if state.markdown.contains("- step one"))
        });
        assert!(has_plan, "proposed plan should be preserved on turn abort");

        let assistant_messages: Vec<String> = chat
            .history_state
            .records
            .iter()
            .filter_map(|record| match record {
                HistoryRecord::AssistantMessage(state) => Some(state.markdown.clone()),
                _ => None,
            })
            .collect();

        assert!(
            assistant_messages.iter().any(|message| message.contains("Before")),
            "final assistant message should keep visible text"
        );
        assert!(
            assistant_messages.iter().any(|message| message.contains("After")),
            "final assistant message should keep trailing visible text"
        );
        assert!(
            assistant_messages
                .iter()
                .all(|message| !message.contains("<proposed_plan>")),
            "assistant message should hide proposed_plan tags"
        );
    });
    }
    
    #[test]
    fn scrollback_spacer_preserves_top_cell_bottom_line() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    insert_plain_cell(chat, &["old-1", "old-2"]);
    insert_plain_cell(chat, &["mid-1", "mid-2"]);
    insert_plain_cell(chat, &["new-1", "new-2"]);
    
    let viewport_height = 6;
    chat.layout.scroll_offset.set(2);
    
    let mut terminal = Terminal::new(TestBackend::new(40, viewport_height)).expect("terminal");
    terminal
        .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
        .expect("draw history");
    
    let adjusted = chat.history_render.adjust_scroll_to_content(2);
    assert_eq!(adjusted, 1, "scroll origin should step back from spacer row");
    
    let prefix = chat.history_render.prefix_sums.borrow();
    assert!(!prefix.is_empty(), "prefix sums populated after draw");
    let start_idx = match prefix.binary_search(&adjusted) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    assert_eq!(start_idx, 0, "expected first cell to be visible after adjustment");
    
    let content_y = prefix[start_idx];
    drop(prefix);
    let skip_top = adjusted.saturating_sub(content_y);
    assert_eq!(skip_top, 1, "should display the second line of the oldest cell");
    
    let cell = &chat.history_cells[start_idx];
    let lines = cell.display_lines_trimmed();
    let line = lines
        .get(skip_top as usize)
        .expect("line available after scroll adjustment");
    let text: String = line.spans.iter().map(|span| span.content.as_ref()).collect();
    assert_eq!(text.trim(), "old-2");
    }
    
    #[test]
    fn final_answer_without_task_complete_clears_spinner() {
    let _rt = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    {
        let chat = harness.chat();
        reset_history(chat);
    }
    
    let turn_id = "turn-1".to_string();
    let order = OrderMeta {
        request_ordinal: 1,
        output_index: Some(0),
        sequence_number: Some(0),
    };
    
    harness.handle_event(Event {
        id: turn_id.clone(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: None,
    });
    
    harness.handle_event(Event {
        id: turn_id.clone(),
        event_seq: 1,
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "thinking about the change".to_string(),
        }),
        order: Some(order),
    });

    harness.handle_event(Event {
        id: turn_id,
        event_seq: 2,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "All done".to_string(),
        }),
        order: Some(OrderMeta {
            request_ordinal: 1,
            output_index: Some(0),
            sequence_number: Some(1),
        }),
    });
    
    harness.flush_into_widget();
    
    assert!(
        !harness.chat().bottom_pane.is_task_running(),
        "spinner should clear after the final answer even when TaskComplete never arrives"
    );
    }
    
    #[test]
    fn scrollback_spacer_exact_offset_adjusts_to_content() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    insert_plain_cell(chat, &["old-1", "old-2"]);
    insert_plain_cell(chat, &["mid-1", "mid-2"]);
    insert_plain_cell(chat, &["new-1", "new-2"]);
    
    let viewport_height = 6;
    chat.layout.scroll_offset.set(2);
    
    {
        let mut terminal =
            Terminal::new(TestBackend::new(40, viewport_height)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw history");
    }
    
    let ranges = chat.history_render.spacing_ranges_for_test();
    let (pos, _) = ranges
        .first()
        .copied()
        .expect("expected a spacer-induced adjustment");
    let adjusted = chat.history_render.adjust_scroll_to_content(pos);
    assert!(
        adjusted < pos,
        "scroll adjustment should reduce the origin when landing on a spacer"
    );
    }
    
    #[test]
    fn scrollback_top_boundary_retains_oldest_content() {
    let mut harness = ChatWidgetHarness::new();
    let chat = harness.chat();
    reset_history(chat);
    
    insert_plain_cell(chat, &["old-1", "old-2"]);
    insert_plain_cell(chat, &["mid-1", "mid-2"]);
    insert_plain_cell(chat, &["new-1", "new-2"]);
    
    {
        let mut terminal = Terminal::new(TestBackend::new(40, 6)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw history");
    }
    
    let max_scroll = chat.layout.last_max_scroll.get();
    assert!(max_scroll > 0, "expected overflow to produce a positive max scroll");
    chat.layout.scroll_offset.set(max_scroll);
    
    let mut terminal = Terminal::new(TestBackend::new(40, 6)).expect("terminal");
    terminal
        .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
        .expect("draw history at top boundary");
    
    let max_scroll = chat.layout.last_max_scroll.get();
    let scroll_from_top = max_scroll.saturating_sub(chat.layout.scroll_offset.get());
    let effective = chat.history_render.adjust_scroll_to_content(scroll_from_top);
    let prefix = chat.history_render.prefix_sums.borrow();
    let mut start_idx = match prefix.binary_search(&effective) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    start_idx = start_idx.min(prefix.len().saturating_sub(1));
    start_idx = start_idx.min(chat.history_cells.len().saturating_sub(1));
    let content_y = prefix[start_idx];
    drop(prefix);
    
    let skip = effective.saturating_sub(content_y) as usize;
    let cell = &chat.history_cells[start_idx];
    let lines = cell.display_lines_trimmed();
    let target_index = skip.min(lines.len().saturating_sub(1));
    let visible = lines
        .get(target_index)
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .unwrap_or_default();
    
    assert!(
        visible.contains("old-1"),
        "scrolling to the top should keep the oldest content visible"
    );
    }
    
