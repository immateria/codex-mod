impl ChatWidget<'_> {
    /// Construct a ChatWidget from an existing conversation (forked session).
    pub(crate) fn new_from_existing(args: ForkedChatWidgetInit) -> Self {
        let ForkedChatWidgetInit {
            config,
            conversation,
            session_configured,
            app_event_tx,
            terminal_info,
            show_order_overlay,
            latest_upgrade_version,
            startup_model_migration_notice,
            auth_manager,
            show_welcome,
        } = args;
        remember_cwd_history(&config.cwd);
        let (code_op_tx, code_op_rx) = unbounded_channel::<Op>();

        let auto_drive_variant = AutoDriveVariant::from_env();

        // Forward events from existing conversation.
        agent::spawn_existing_conversation_runtime(
            conversation,
            session_configured,
            app_event_tx.clone(),
            code_op_rx,
        );

        // Basic widget state mirrors `new`
        let history_cells: Vec<Box<dyn HistoryCell>> = Vec::new();

        let mut bottom_pane = BottomPane::new(BottomPaneParams {
            app_event_tx: app_event_tx.clone(),
            has_input_focus: true,
            using_chatgpt_auth: config.using_chatgpt_auth,
            auto_drive_variant,
        });
        bottom_pane.set_subagent_commands(
            config
                .subagent_commands
                .iter()
                .map(|command| command.name.clone())
                .collect(),
        );

        let mut w = Self {
            app_event_tx,
            code_op_tx,
            bottom_pane,
            auth_manager,
            login_view_state: None,
            login_add_view_state: None,
            active_exec_cell: None,
            history_cells,
            config: config.clone(),
            turn_sleep_inhibitor: SleepInhibitor::new(config.prevent_idle_sleep),
            mcp_tool_catalog_by_id: HashMap::new(),
            mcp_tools_by_server: HashMap::new(),
            mcp_disabled_tools_by_server: HashMap::new(),
            mcp_resources_by_server: HashMap::new(),
            mcp_resource_templates_by_server: HashMap::new(),
            mcp_server_failures: HashMap::new(),
            mcp_auth_statuses: HashMap::new(),
            startup_mcp_error_summary: None,
            remote_model_presets: None,
            allow_remote_default_at_startup: !config.model_explicit,
            chat_model_selected_explicitly: false,
            collaboration_mode: code_core::protocol::CollaborationModeKind::from_sandbox_policy(
                &config.sandbox_policy,
            ),
            planning_restore: None,
            history_debug_events: if history_cell_logging_enabled() {
                Some(RefCell::new(Vec::new()))
            } else {
                None
            },
            latest_upgrade_version,
            startup_model_migration_notice,
            reconnect_notice_active: false,
            initial_user_message: None,
            total_token_usage: TokenUsage::default(),
            last_token_usage: TokenUsage::default(),
            rate_limit_snapshot: None,
            rate_limit_warnings: RateLimitWarningState::default(),
            rate_limit_fetch_inflight: false,
            rate_limit_last_fetch_at: None,
            rate_limit_primary_next_reset_at: None,
            rate_limit_secondary_next_reset_at: None,
            rate_limit_refresh_scheduled_for: None,
            rate_limit_refresh_schedule_id: Arc::new(AtomicU64::new(0)),
            content_buffer: String::new(),
            last_assistant_message: None,
            last_answer_stream_id_in_turn: None,
            last_answer_history_id_in_turn: None,
            last_seen_answer_stream_id_in_turn: None,
            mid_turn_answer_ids_in_turn: HashSet::new(),
            last_user_message: None,
            last_developer_message: None,
            pending_turn_origin: None,
            pending_request_user_input: None,
            current_turn_origin: None,
            cleared_lingering_execs_this_turn: true,
            exec: ExecState {
                running_commands: HashMap::new(),
                running_explore_agg_index: None,
                pending_exec_ends: HashMap::new(),
                suppressed_exec_end_call_ids: HashSet::new(),
                suppressed_exec_end_order: VecDeque::new(),
            },
            canceled_exec_call_ids: HashSet::new(),
            tools_state: ToolState {
                running_custom_tools: HashMap::new(),
                web_search_sessions: HashMap::new(),
                web_search_by_call: HashMap::new(),
                web_search_by_order: HashMap::new(),
                running_wait_tools: HashMap::new(),
                running_kill_tools: HashMap::new(),
                image_viewed_calls: HashSet::new(),
                browser_sessions: HashMap::new(),
                browser_session_by_call: HashMap::new(),
                browser_session_by_order: HashMap::new(),
                browser_last_key: None,
                agent_runs: HashMap::new(),
                agent_run_by_call: HashMap::new(),
                agent_run_by_order: HashMap::new(),
                agent_run_by_batch: HashMap::new(),
                agent_run_by_agent: HashMap::new(),
                agent_last_key: None,
                auto_drive_tracker: None,
            },
            live_builder: RowBuilder::new(usize::MAX),
            header_wave: {
                let effect = HeaderWaveEffect::new();
                if ENABLE_WARP_STRIPES {
                    effect.set_enabled(true, Instant::now());
                } else {
                    effect.set_enabled(false, Instant::now());
                }
                effect
            },
            browser_overlay_visible: false,
            browser_overlay_state: BrowserOverlayState::default(),
            pending_images: HashMap::new(),
            welcome_shown: false,
            test_mode: is_test_mode(),
            latest_browser_screenshot: Arc::new(Mutex::new(None)),
            browser_autofix_requested: Arc::new(AtomicBool::new(false)),
            cached_image_protocol: RefCell::new(None),
            cached_picker: RefCell::new(terminal_info.picker.clone()),
            cached_cell_size: std::cell::OnceCell::new(),
            git_branch_cache: RefCell::new(GitBranchCache::default()),
            terminal_info,
            active_agents: Vec::new(),
            agents_ready_to_start: false,
            last_agent_prompt: None,
            agent_context: None,
            agent_task: None,
            recent_agent_hint: None,
            suppress_next_agent_hint: false,
            active_review_hint: None,
            active_review_prompt: None,
            auto_resolve_state: None,
            auto_resolve_attempts_baseline: config.auto_drive.auto_resolve_review_attempts.get(),
            turn_had_code_edits: false,
            background_review: None,
            auto_review_status: None,
            auto_review_notice: None,
            auto_review_baseline: None,
            auto_review_reviewed_marker: None,
            pending_auto_review_range: None,
            turn_sequence: 0,
            review_guard: None,
            background_review_guard: None,
            processed_auto_review_agents: HashSet::new(),
            pending_turn_descriptor: None,
            render_request_cache: RefCell::new(Vec::new()),
            render_request_cache_dirty: Cell::new(true),
            history_prefix_append_only: Cell::new(true),
            pending_auto_turn_config: None,
            overall_task_status: "preparing".to_string(),
            active_plan_title: None,
            agent_runtime: HashMap::new(),
            pending_agent_updates: HashMap::new(),
            stream: crate::streaming::controller::StreamController::new(config.clone()),
            stream_state: StreamState {
                current_kind: None,
                closed_answer_ids: HashSet::new(),
                closed_reasoning_ids: HashSet::new(),
                seq_answer_final: None,
                drop_streaming: false,
                answer_markup: HashMap::new(),
            },
            interrupts: interrupts::InterruptManager::new(),
            interrupt_flush_scheduled: false,
            ended_call_ids: HashSet::new(),
            diffs: DiffsState {
                session_patch_sets: Vec::new(),
                baseline_file_contents: HashMap::new(),
                overlay: None,
                confirm: None,
                body_visible_rows: std::cell::Cell::new(0),
            },
            help: HelpState {
                overlay: None,
                body_visible_rows: std::cell::Cell::new(0),
            },
            settings: SettingsState::default(),
            pending_settings_return: None,
            limits: LimitsState::default(),
            terminal: TerminalState::default(),
            pending_manual_terminal: HashMap::new(),
            agents_overview_selected_index: 0,
            agents_terminal: AgentsTerminalState::new(),
            js_repl_last_runtime: None,
            pending_git_init_resume: None,
            git_init_inflight: false,
            git_init_declined: false,
            pending_upgrade_notice: None,
            history_render: HistoryRenderState::new(),
            last_render_settings: Cell::new(RenderSettings::new(0, 0, false)),
            render_theme_epoch: 0,
            history_state: HistoryState::new(),
            history_snapshot_dirty: false,
            history_snapshot_last_flush: None,
            context_cell_id: None,
            context_summary: None,
            context_last_sequence: None,
            context_browser_sequence: None,
            history_cell_ids: Vec::new(),
            history_live_window: None,
            history_frozen_width: 0,
            history_frozen_count: 0,
            height_manager: RefCell::new(HeightManager::new(
                crate::height_manager::HeightManagerConfig::default(),
            )),
            layout: LayoutState {
                scroll_offset: Cell::new(0),
                last_max_scroll: std::cell::Cell::new(0),
                last_history_viewport_height: std::cell::Cell::new(0),
                vertical_scrollbar_state: std::cell::RefCell::new(ScrollbarState::default()),
                scrollbar_visible_until: std::cell::Cell::new(None),
                last_bottom_reserved_rows: std::cell::Cell::new(0),
                last_frame_height: std::cell::Cell::new(0),
                last_frame_width: std::cell::Cell::new(0),
                last_bottom_pane_area: std::cell::Cell::new(Rect::default()),
            },
            last_theme: crate::theme::current_theme(),
            perf_state: PerfState {
                enabled: false,
                stats: RefCell::new(PerfStats::default()),
                pending_scroll_rows: Cell::new(0),
            },
            session_id: None,
            active_task_ids: HashSet::new(),
            queued_user_messages: std::collections::VecDeque::new(),
            pending_dispatched_user_messages: std::collections::VecDeque::new(),
            pending_user_prompts_for_next_turn: 0,
            queue_block_started_at: None,
            ghost_snapshots: Vec::new(),
            ghost_snapshots_disabled: false,
            ghost_snapshots_disabled_reason: None,
            ghost_snapshot_queue: VecDeque::new(),
            active_ghost_snapshot: None,
            next_ghost_snapshot_id: 0,
            history_virtualization_sync_pending: Cell::new(false),
            auto_drive_card_sequence: 0,
            auto_drive_variant,
            auto_state: AutoDriveController::default(),
            auto_goal_escape_state: AutoGoalEscState::Inactive,
            auto_handle: None,
            auto_drive_pid_guard: None,
            auto_history: AutoDriveHistory::new(),
            auto_compaction_overlay: None,
            auto_turn_review_state: None,
            auto_pending_goal_request: false,
            auto_goal_bootstrap_done: false,
            cloud_tasks_selected_env: None,
            cloud_tasks_environments: Vec::new(),
            cloud_tasks_last_tasks: Vec::new(),
            cloud_tasks_best_of_n: 1,
            cloud_tasks_creation_inflight: false,
            cloud_task_apply_tickets: HashMap::new(),
            cloud_task_create_ticket: None,
            browser_is_external: false,
            next_cli_text_format: None,
            // Strict ordering init for forked widget
            cell_order_seq: vec![OrderKey {
                req: 0,
                out: -1,
                seq: 0,
            }],
            cell_order_dbg: vec![None; 1],
            reasoning_index: HashMap::new(),
            stream_order_seq: HashMap::new(),
            order_request_bias: 0,
            resume_expected_next_request: None,
            resume_provider_baseline: None,
            last_seen_request_index: 0,
            current_request_index: 0,
            internal_seq: 0,
            show_order_overlay,
            scroll_history_hint_shown: false,
            access_status_idx: None,
            standard_terminal_mode: !config.tui.alternate_screen,
            pending_agent_notes: Vec::new(),
            synthetic_system_req: None,
            system_cell_by_id: HashMap::new(),
            ui_background_seq_counters: HashMap::new(),
            last_assigned_order: None,
            replay_history_depth: 0,
            resume_placeholder_visible: false,
            resume_picker_loading: false,
            clickable_regions: RefCell::new(Vec::new()),
            hovered_clickable_action: RefCell::new(None),
        };
        w.load_auto_review_baseline_marker();
        if let Ok(Some(active_id)) = auth_accounts::get_active_account_id(&config.code_home)
            && let Ok(records) = account_usage::list_rate_limit_snapshots(&config.code_home)
                && let Some(record) = records.into_iter().find(|r| r.account_id == active_id) {
                    w.rate_limit_primary_next_reset_at = record.primary_next_reset_at;
                    w.rate_limit_secondary_next_reset_at = record.secondary_next_reset_at;
                    w.maybe_schedule_rate_limit_refresh();
                }
        w.set_standard_terminal_mode(!config.tui.alternate_screen);
        let welcome_brand_title = w.config.tui.branding.title.as_deref();
        if show_welcome {
            w.history_push_top_next_req(history_cell::new_animated_welcome(
                welcome_brand_title,
            ));
        }
        if w.test_mode {
            w.bottom_pane.set_task_running(false);
            w.bottom_pane.update_status_text(String::new());
            #[cfg(any(test, feature = "test-helpers"))]
            w.seed_test_mode_greeting();
        }
        w.maybe_start_auto_upgrade_task();
        w
    }
}
