    #[cfg(feature = "managed-network-proxy")]
    #[test]
    fn network_settings_emits_apply_event_with_expected_fields() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    use crate::bottom_pane::SettingsSection;

    {
        let chat = harness.chat();
        chat.ensure_settings_overlay_section(SettingsSection::Network);
        chat.show_settings_overlay(Some(SettingsSection::Network));
    }
    harness.flush_into_widget();

    {
        let chat = harness.chat();
        // Navigate to "Advanced" (row 5), toggle on, toggle off.
        for _ in 0..5 {
            chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Back to Enabled and toggle it on.
        for _ in 0..5 {
            chat.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        }
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Move to Apply and activate.
        for _ in 0..6 {
            chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    let mut applied = None;
    for event in harness.drain_events() {
        if let AppEvent::SetNetworkProxySettings(settings) = event {
            applied = Some(settings);
            break;
        }
    }

    let applied = applied.expect("expected SetNetworkProxySettings event");
    assert!(applied.enabled, "expected Enabled to be true after toggle");
    }

    #[test]
    fn js_repl_settings_emits_apply_event_with_expected_fields() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    use crate::bottom_pane::SettingsSection;

    {
        let chat = harness.chat();
        chat.ensure_settings_overlay_section(SettingsSection::JsRepl);
        chat.show_settings_overlay(Some(SettingsSection::JsRepl));
    }
    harness.flush_into_widget();

    {
        let chat = harness.chat();
        // Toggle Enabled (row 0).
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Move to Apply and activate.
        for _ in 0..7 {
            chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    let mut applied = None;
    for event in harness.drain_events() {
        if let AppEvent::SetJsReplSettings(settings) = event {
            applied = Some(settings);
            break;
        }
    }

    let applied = applied.expect("expected SetJsReplSettings event");
    assert!(applied.enabled, "expected Enabled to be true after toggle");
    }

    #[test]
    fn exec_limits_settings_emits_apply_event_with_expected_fields() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    use crate::bottom_pane::SettingsSection;

    {
        let chat = harness.chat();
        chat.ensure_settings_overlay_section(SettingsSection::ExecLimits);
        chat.show_settings_overlay(Some(SettingsSection::ExecLimits));
    }
    harness.flush_into_widget();

    {
        let chat = harness.chat();

        // PIDs row: Auto -> Disabled -> Edit.
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        for ch in ['5', '1', '2'] {
            chat.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        chat.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

        // Move to Apply and activate.
        for _ in 0..4 {
            chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    let mut applied = None;
    for event in harness.drain_events() {
        if let AppEvent::SetExecLimitsSettings(settings) = event {
            applied = Some(settings);
            break;
        }
    }

    let applied = applied.expect("expected SetExecLimitsSettings event");
    assert!(
        matches!(applied.pids_max, code_core::config::ExecLimitToml::Value(512)),
        "expected pids_max=512, got: {:?}",
        applied.pids_max
    );
    }

    #[test]
    fn memories_status_command_dispatches_async_status_load() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        chat.handle_memories_command("status".to_string());
    });

    let mut saw_status_load = false;
    for event in harness.drain_events() {
        if let AppEvent::RunMemoriesStatusLoad { target } = event {
            assert_eq!(
                target,
                crate::app_event::MemoriesStatusLoadTarget::SlashCommand
            );
            saw_status_load = true;
        }
    }

    assert!(saw_status_load, "expected RunMemoriesStatusLoad event");
    }

    fn sample_memories_status() -> code_core::MemoriesStatus {
    code_core::MemoriesStatus {
        artifacts: code_core::MemoriesArtifactsStatus {
            memory_root: PathBuf::from("/tmp/code-home/memories"),
            summary: code_core::MemoryArtifactStatus {
                exists: true,
                modified_at: Some("2026-03-07T12:00:00Z".to_string()),
            },
            raw_memories: code_core::MemoryArtifactStatus {
                exists: true,
                modified_at: Some("2026-03-07T12:00:00Z".to_string()),
            },
            rollout_summaries: code_core::MemoryArtifactStatus {
                exists: true,
                modified_at: Some("2026-03-07T12:00:00Z".to_string()),
            },
            rollout_summary_count: 2,
        },
        db: code_core::MemoriesDbStatus {
            db_exists: true,
            thread_count: 3,
            stage1_epoch_count: 2,
            pending_stage1_count: 1,
            running_stage1_count: 0,
            dead_lettered_stage1_count: 1,
            artifact_job_running: false,
            artifact_dirty: true,
            last_artifact_build_at: Some("2026-03-07T12:00:00Z".to_string()),
        },
        effective: code_core::config_types::MemoriesConfig::default(),
        sources: code_core::MemoriesResolvedSources {
            no_memories_if_mcp_or_web_search: code_core::MemoriesSettingSource::Default,
            generate_memories: code_core::MemoriesSettingSource::Global,
            use_memories: code_core::MemoriesSettingSource::Profile,
            max_raw_memories_for_consolidation: code_core::MemoriesSettingSource::Default,
            max_rollout_age_days: code_core::MemoriesSettingSource::Default,
            max_rollouts_per_startup: code_core::MemoriesSettingSource::Default,
            min_rollout_idle_hours: code_core::MemoriesSettingSource::Default,
        },
    }
    }

    #[test]
    fn memories_status_loaded_success_renders_history_notice() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        chat.on_memories_status_loaded(
            crate::app_event::MemoriesStatusLoadTarget::SlashCommand,
            Ok(sample_memories_status()),
        );
    });

    let saw_report = harness.with_chat(|chat| {
        chat.history_cells.iter().any(|cell| {
            let rendered = cell
                .display_lines_trimmed()
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            rendered.contains("Memories root:")
                && rendered.contains("SQLite: present")
                && rendered.contains("dead_lettered=1")
                && rendered.contains("artifact_dirty=on")
        })
    });

    assert!(saw_report, "expected slash-command memories status report in history");
    }

    #[test]
    fn memories_status_loaded_failure_renders_error_event() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        chat.on_memories_status_loaded(
            crate::app_event::MemoriesStatusLoadTarget::SlashCommand,
            Err("boom".to_string()),
        );
    });

    let saw_error = harness.with_chat(|chat| {
        chat.history_cells.iter().any(|cell| {
            cell.display_lines_trimmed().iter().any(|line| {
                line.to_string()
                    .contains("Failed to read memories status: boom")
            })
        })
    });

    assert!(saw_error, "expected slash-command memories status error in history");
    }

    #[test]
    fn settings_overlay_focus_allows_sidebar_navigation_without_getting_stuck() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    use crate::bottom_pane::SettingsSection;

    {
        let chat = harness.chat();
        chat.show_settings_overlay(Some(SettingsSection::Experimental));
    }
    harness.flush_into_widget();

    let render = |harness: &mut ChatWidgetHarness| -> String {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
        terminal.backend().to_string()
    };

    // Default focus is content. Home should not switch sections while content is focused.
    let output_before = render(&mut harness);
    assert!(
        output_before.contains("Experimental Features") && output_before.contains("Focus: Content"),
        "expected Experimental settings with content focus, got:\n{output_before}",
    );

    harness.with_chat(|chat| {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.handle_key_event(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    });
    let output_after_home = render(&mut harness);
    assert!(
        output_after_home.contains("Experimental Features"),
        "expected Home to not switch sections while content is focused, got:\n{output_after_home}",
    );

    // Shift+Tab switches to the sidebar, where Home should jump to the Model page.
    harness.with_chat(|chat| {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.handle_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    });
    let output_sidebar = render(&mut harness);
    assert!(
        output_sidebar.contains("Focus: Sidebar"),
        "expected sidebar focus after Shift+Tab, got:\n{output_sidebar}",
    );

    harness.with_chat(|chat| {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.handle_key_event(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    });
    let output_model = render(&mut harness);
    assert!(
        output_model.contains("Select Model & Reasoning"),
        "expected sidebar navigation to reach Model settings, got:\n{output_model}",
    );

    // Tab returns focus to content.
    harness.with_chat(|chat| {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    });
    let output_content = render(&mut harness);
    assert!(
        output_content.contains("Focus: Content"),
        "expected content focus after Tab, got:\n{output_content}",
    );
    }

    #[test]
    fn auto_settings_menu_switches_between_overlay_and_bottom_on_resize() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    use crate::bottom_pane::SettingsSection;
    use code_core::config_types::{SettingsMenuConfig, SettingsMenuOpenMode};

    {
        let chat = harness.chat();
        chat.apply_tui_settings_menu(SettingsMenuConfig {
            open_mode: SettingsMenuOpenMode::Auto,
            overlay_min_width: 100,
        });
        chat.layout.last_frame_width.set(120);
        chat.show_settings_overlay(Some(SettingsSection::Experimental));
        assert!(
            chat.settings.overlay.is_some(),
            "expected overlay settings at wide width",
        );
    }

    {
        let chat = harness.chat();
        chat.sync_settings_route_for_width(80);
        assert!(
            chat.settings.overlay.is_none(),
            "expected bottom-pane settings after narrowing width",
        );
        assert!(
            chat.bottom_pane.has_active_view(),
            "expected an active bottom-pane settings view",
        );
    }

    {
        let chat = harness.chat();
        chat.sync_settings_route_for_width(120);
        let overlay = chat
            .settings
            .overlay
            .as_ref()
            .expect("expected overlay settings after widening width");
        assert_eq!(
            overlay.active_section(),
            SettingsSection::Experimental,
            "expected active settings section to be preserved across mode switches",
        );
        assert!(
            !chat.bottom_pane.has_active_view(),
            "expected bottom-pane settings view to be cleared when overlay route is restored",
        );
    }
    }

    #[test]
    fn apply_tui_settings_menu_reroutes_open_settings_ui() {
        let _guard = enter_test_runtime_guard();
        let mut harness = ChatWidgetHarness::new();
        use crate::bottom_pane::SettingsSection;
        use code_core::config_types::{SettingsMenuConfig, SettingsMenuOpenMode};

        {
            let chat = harness.chat();
            chat.apply_tui_settings_menu(SettingsMenuConfig {
                open_mode: SettingsMenuOpenMode::Overlay,
                overlay_min_width: 100,
            });
            chat.layout.last_frame_width.set(120);
            chat.show_settings_overlay(Some(SettingsSection::Interface));
            assert!(
                chat.settings.overlay.is_some(),
                "expected overlay settings to be open before rerouting",
            );
        }

        {
            let chat = harness.chat();
            chat.apply_tui_settings_menu(SettingsMenuConfig {
                open_mode: SettingsMenuOpenMode::Bottom,
                overlay_min_width: 100,
            });
            assert!(
                chat.settings.overlay.is_none(),
                "expected overlay settings to close after switching to bottom mode",
            );
            assert!(
                chat.bottom_pane.has_active_view(),
                "expected a bottom-pane settings view after switching to bottom mode",
            );
            assert_eq!(
                chat.settings.bottom_route,
                Some(Some(SettingsSection::Interface)),
                "expected current settings section to be preserved when rerouting to bottom pane",
            );
        }

        {
            let chat = harness.chat();
            chat.apply_tui_settings_menu(SettingsMenuConfig {
                open_mode: SettingsMenuOpenMode::Overlay,
                overlay_min_width: 100,
            });
            assert!(
                chat.settings.overlay.is_some(),
                "expected overlay settings to be restored after switching back to overlay mode",
            );
            assert!(
                !chat.bottom_pane.has_active_view(),
                "expected bottom-pane view to be cleared when rerouting back to overlay",
            );
        }
    }

    #[test]
    fn auto_settings_menu_can_reroute_accounts_section_to_bottom_pane() {
        let _guard = enter_test_runtime_guard();
        let mut harness = ChatWidgetHarness::new();
        use crate::bottom_pane::SettingsSection;
        use code_core::config_types::{SettingsMenuConfig, SettingsMenuOpenMode};

        {
            let chat = harness.chat();
            chat.apply_tui_settings_menu(SettingsMenuConfig {
                open_mode: SettingsMenuOpenMode::Auto,
                overlay_min_width: 100,
            });
            chat.layout.last_frame_width.set(120);
            chat.show_settings_overlay(Some(SettingsSection::Accounts));
            assert!(
                chat.settings.overlay.is_some(),
                "expected overlay settings to open at wide width",
            );
        }

        {
            let chat = harness.chat();
            chat.sync_settings_route_for_width(80);
            assert!(
                chat.settings.overlay.is_none(),
                "expected Accounts section to reroute to bottom-pane settings when width is narrow",
            );
            assert!(
                chat.bottom_pane.has_active_view(),
                "expected a bottom-pane settings view to be active after rerouting",
            );
            assert_eq!(
                chat.settings.bottom_route,
                Some(Some(SettingsSection::Accounts)),
                "expected Accounts section to be tracked as the active bottom-pane route",
            );
        }
    }

    #[test]
    fn settings_overlay_overview_keeps_selected_row_visible_when_scrolling() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    use crate::bottom_pane::SettingsSection;
    use code_core::config_types::{SettingsMenuConfig, SettingsMenuOpenMode};

    {
        let chat = harness.chat();
        chat.apply_tui_settings_menu(SettingsMenuConfig {
            open_mode: SettingsMenuOpenMode::Overlay,
            overlay_min_width: 80,
        });
        chat.show_settings_overlay(None);
    }
    harness.flush_into_widget();

    harness.with_chat(|chat| {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        for _ in 0..40 {
            if chat
                .settings
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.active_section() == SettingsSection::Limits)
            {
                break;
            }
            chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert!(
            chat.settings
                .overlay
                .as_ref()
                .is_some_and(|overlay| overlay.active_section() == SettingsSection::Limits),
            "expected to navigate to Limits section in overlay overview",
        );
    });

    let output = {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(72, 16)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
        terminal.backend().to_string()
    };

    assert!(
        output.contains("› Limits") || output.contains("» Limits"),
        "expected selected Limits row to remain visible in overview:\n{output}",
    );
    }

    #[test]
    fn settings_overlay_overview_includes_all_sections() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    use crate::bottom_pane::SettingsSection;
    use code_core::config_types::{SettingsMenuConfig, SettingsMenuOpenMode};

    {
        let chat = harness.chat();
        chat.apply_tui_settings_menu(SettingsMenuConfig {
            open_mode: SettingsMenuOpenMode::Overlay,
            overlay_min_width: 80,
        });
        chat.show_settings_overlay(None);
    }
    harness.flush_into_widget();

    let sections = {
        let chat = harness.chat();
        let overlay = chat
            .settings
            .overlay
            .as_ref()
            .expect("expected settings overlay to be open");
        overlay.overview_sections()
    };

    assert_eq!(
        sections,
        SettingsSection::ALL.to_vec(),
        "expected /settings overview to include every registered settings section",
    );
    }

    #[test]
    fn plugins_section_is_built_in_settings_overlay() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    use crate::bottom_pane::SettingsSection;
    use code_core::config_types::{SettingsMenuConfig, SettingsMenuOpenMode};

    {
        let chat = harness.chat();
        chat.apply_tui_settings_menu(SettingsMenuConfig {
            open_mode: SettingsMenuOpenMode::Overlay,
            overlay_min_width: 80,
        });
        chat.show_settings_overlay(Some(SettingsSection::Plugins));
    }
    harness.flush_into_widget();

    let chat = harness.chat();
    let overlay = chat
        .settings
        .overlay
        .as_ref()
        .expect("expected settings overlay to be open");

    assert_eq!(overlay.active_section(), SettingsSection::Plugins);
    assert!(
        overlay.has_plugins_content(),
        "expected Plugins content to be populated in /settings overlay",
    );
    }

    #[test]
    fn clicking_wide_settings_overview_opens_selected_section() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    use code_core::config_types::{SettingsMenuConfig, SettingsMenuOpenMode};
    use crate::test_backend::VT100Backend;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::Terminal;

    {
        let chat = harness.chat();
        chat.apply_tui_settings_menu(SettingsMenuConfig {
            open_mode: SettingsMenuOpenMode::Overlay,
            overlay_min_width: 80,
        });
        chat.show_settings_overlay(None);
    }
    harness.flush_into_widget();

    let (theme_col, theme_row) = {
        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(120, 30)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
        let snapshot = terminal.backend().to_string();
        let (row, col) = snapshot
            .lines()
            .enumerate()
            .find_map(|(idx, line)| {
                if !line.contains("Theme:") {
                    return None;
                }
                let col = line.find("Theme")?;
                Some((idx, col))
            })
            .expect("expected Theme row to be present in Settings overview snapshot");
        (col as u16, row as u16)
    };

    harness.with_chat(|chat| {
        chat.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: theme_col,
            row: theme_row,
            modifiers: KeyModifiers::NONE,
        });
    });

    let output = {
        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(120, 30)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
        terminal.backend().to_string()
    };

    assert!(
        output.contains("Settings ▸ Theme") || output.contains("Theme Settings"),
        "expected click on overview row to open Theme settings, got:\n{output}",
    );
    assert!(
        !output.contains("Settings ▸ Overview"),
        "expected overlay to leave overview mode after click, got:\n{output}",
    );
    }

    #[test]
    fn network_approval_renders_network_modal_without_exec_persist_options() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.handle_event(Event {
        id: "turn-1".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecApprovalRequest(code_core::protocol::ExecApprovalRequestEvent {
            call_id: "approve-1".to_string(),
            approval_id: None,
            turn_id: "turn-1".to_string(),
            command: vec![
                "/bin/bash".to_string(),
                "-lc".to_string(),
                "curl https://example.com".to_string(),
            ],
            cwd: std::env::temp_dir(),
            reason: Some("Allowlist miss".to_string()),
            network_approval_context: Some(code_core::protocol::NetworkApprovalContext {
                host: "example.com".to_string(),
                protocol: code_core::protocol::NetworkApprovalProtocol::Https,
            }),
            additional_permissions: None,
        }),
        order: None,
    });
    harness.flush_into_widget();

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
        output.contains("Network access blocked"),
        "expected network approval title, got:\n{output}",
    );
    assert!(
        output.contains("Host:") && output.contains("example.com"),
        "expected host line, got:\n{output}",
    );
    assert!(
        output.contains("Protocol:") && output.contains("HTTPS"),
        "expected protocol line, got:\n{output}",
    );
    assert!(output.contains("Allow once"), "missing Allow once option:\n{output}");
    assert!(
        output.contains("Allow for session"),
        "missing Allow for session option:\n{output}",
    );
    assert!(
        output.contains("Deny network for this run"),
        "missing deny-for-run option:\n{output}",
    );
    assert!(
        output.contains("Settings -> Network"),
        "missing settings hint:\n{output}",
    );
    assert!(
        !output.contains("Always allow") && !output.contains("project"),
        "network approvals should not include exec persist options:\n{output}",
    );
    }

    #[cfg(feature = "managed-network-proxy")]
    #[test]
    fn statusline_network_segment_click_on_top_opens_network_settings() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        use code_core::config_types::StatusLineLane;
        use crate::bottom_pane::settings_pages::status_line::StatusLineItem;

        chat.setup_status_line(
            vec![StatusLineItem::ModelName, StatusLineItem::NetworkMediation],
            Vec::new(),
            StatusLineLane::Top,
        );
    });

    // Render once to populate clickable regions.
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
            .min_by_key(|region| region.rect.y)
            .expect("expected a top statusline region for network settings");
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

    #[test]
    fn statusline_service_tier_segment_click_dispatches_toggle() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        use code_core::config_types::StatusLineLane;
        use crate::bottom_pane::settings_pages::status_line::StatusLineItem;

        chat.config.model = "gpt-5.4".to_string();
        chat.config.service_tier = None;
        chat.setup_status_line(
            vec![StatusLineItem::ServiceTier],
            Vec::new(),
            StatusLineLane::Top,
        );
    });

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
            .find(|region| region.action == ClickableAction::ToggleServiceTier)
            .expect("expected status line region for service tier");
        let x = region.rect.x.saturating_add(region.rect.width.saturating_div(2));
        (x, region.rect.y)
    });

    harness.with_chat(|chat| chat.handle_click((x, y)));

    let events = harness.drain_events();
    assert!(events.into_iter().any(|event| matches!(
        event,
        AppEvent::UpdateServiceTierSelection {
            service_tier: Some(code_core::config_types::ServiceTier::Fast),
        }
    )));
    }

    #[test]
    fn default_header_service_tier_segment_click_dispatches_toggle() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        chat.config.model = "gpt-5.4".to_string();
        chat.config.service_tier = None;
    });

    let _env_lock = crate::chatwidget::smoke_helpers::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _header_guard = crate::tui_env::ForceMinimalHeaderOverrideGuard::set(false);

    {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(140, 24)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
    }

    let (x, y) = harness.with_chat(|chat| {
        let regions = chat.clickable_regions.borrow();
        let region = regions
            .iter()
            .find(|region| region.action == ClickableAction::ToggleServiceTier)
            .expect("expected default header region for service tier");
        let x = region.rect.x.saturating_add(region.rect.width.saturating_div(2));
        (x, region.rect.y)
    });

    harness.with_chat(|chat| chat.handle_click((x, y)));

    let events = harness.drain_events();
    assert!(events.into_iter().any(|event| matches!(
        event,
        AppEvent::UpdateServiceTierSelection {
            service_tier: Some(code_core::config_types::ServiceTier::Fast),
        }
    )));
    }

    #[test]
    fn service_tier_controls_hide_for_non_gpt_5_4_models() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        use code_core::config_types::StatusLineLane;
        use crate::bottom_pane::settings_pages::status_line::StatusLineItem;

        chat.config.model = "gpt-5.4-mini".to_string();
        chat.config.service_tier = None;
        chat.setup_status_line(
            vec![StatusLineItem::ServiceTier],
            Vec::new(),
            StatusLineLane::Top,
        );
    });

    {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(140, 24)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
    }

    harness.with_chat(|chat| {
        let regions = chat.clickable_regions.borrow();
        assert!(
            !regions
                .iter()
                .any(|region| region.action == ClickableAction::ToggleServiceTier)
        );
    });
    }

    #[test]
    fn header_directory_segment_click_dispatches_switch_cwd() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let _env_lock = crate::chatwidget::smoke_helpers::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let picked = std::env::temp_dir().join("picked-status-dir");
    std::fs::create_dir_all(&picked).expect("create picked directory");
    crate::native_picker::set_test_pick_result(Some(picked.clone()));

    harness.with_chat(|chat| {
        chat.config.tui.header.top_line_text = Some("Directory: {directory}".to_string());
    });

    {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(100, 24)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
    }

    let (x, y) = harness.with_chat(|chat| {
        let regions = chat.clickable_regions.borrow();
        let region = regions
            .iter()
            .find(|region| region.action == ClickableAction::ShowDirectoryPicker)
            .expect("expected header region for directory picker");
        let x = region.rect.x.saturating_add(region.rect.width.saturating_div(2));
        (x, region.rect.y)
    });

    harness.with_chat(|chat| chat.handle_click((x, y)));

    let events = harness.drain_events();
    assert!(events.into_iter().any(|event| matches!(
        event,
        AppEvent::SwitchCwd(path, None) if path == picked
    )));
    }

    #[test]
    fn statusline_directory_segment_click_dispatches_switch_cwd() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let _env_lock = crate::chatwidget::smoke_helpers::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let picked = std::env::temp_dir().join("picked-top-status-dir");
    std::fs::create_dir_all(&picked).expect("create picked directory");
    crate::native_picker::set_test_pick_result(Some(picked.clone()));

    harness.with_chat(|chat| {
        use code_core::config_types::StatusLineLane;
        use crate::bottom_pane::settings_pages::status_line::StatusLineItem;

        chat.setup_status_line(
            vec![StatusLineItem::CurrentDir],
            Vec::new(),
            StatusLineLane::Top,
        );
    });

    {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(100, 24)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
    }

    let (x, y) = harness.with_chat(|chat| {
        let regions = chat.clickable_regions.borrow();
        let region = regions
            .iter()
            .find(|region| region.action == ClickableAction::ShowDirectoryPicker)
            .expect("expected status line region for directory picker");
        let x = region.rect.x.saturating_add(region.rect.width.saturating_div(2));
        (x, region.rect.y)
    });

    harness.with_chat(|chat| chat.handle_click((x, y)));

    let events = harness.drain_events();
    assert!(events.into_iter().any(|event| matches!(
        event,
        AppEvent::SwitchCwd(path, None) if path == picked
    )));
    }

    #[test]
    fn startup_notice_preserves_second_line_directory_click_regions() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let _env_lock = crate::chatwidget::smoke_helpers::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let picked = std::env::temp_dir().join("picked-notice-dir");
    std::fs::create_dir_all(&picked).expect("create picked directory");
    crate::native_picker::set_test_pick_result(Some(picked.clone()));

    harness.with_chat(|chat| {
        chat.startup_model_migration_notice = Some(crate::model_migration::StartupModelMigrationNotice {
            current_model_label: "gpt-5.2-codex".to_string(),
            target_model_label: "gpt-5.3-codex".to_string(),
            target_model: "gpt-5.3-codex".to_string(),
            hide_key: code_common::model_presets::HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            new_effort: None,
        });
    });

    let _header_guard = crate::tui_env::ForceMinimalHeaderOverrideGuard::set(false);

    {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(120, 24)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
    }

    let (x, y) = harness.with_chat(|chat| {
        let regions = chat.clickable_regions.borrow();
        let region = regions
            .iter()
            .find(|region| region.action == ClickableAction::ShowDirectoryPicker)
            .expect("expected second-line header region for directory picker");
        let x = region.rect.x.saturating_add(region.rect.width.saturating_div(2));
        (x, region.rect.y)
    });

    harness.with_chat(|chat| chat.handle_click((x, y)));

    let events = harness.drain_events();
    assert!(events.into_iter().any(|event| matches!(
        event,
        AppEvent::SwitchCwd(path, None) if path == picked
    )));
    }

    #[test]
    fn startup_model_notice_click_dispatches_accept_event() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        chat.startup_model_migration_notice = Some(crate::model_migration::StartupModelMigrationNotice {
            current_model_label: "gpt-5.2-codex".to_string(),
            target_model_label: "gpt-5.3-codex".to_string(),
            target_model: "gpt-5.3-codex".to_string(),
            hide_key: code_common::model_presets::HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            new_effort: None,
        });
    });

    {
        use crate::test_backend::VT100Backend;
        use ratatui::Terminal;

        let chat = harness.chat();
        let mut terminal = Terminal::new(VT100Backend::new(100, 24)).expect("terminal");
        terminal
            .draw(|frame| frame.render_widget_ref(&*chat, frame.area()))
            .expect("draw");
    }

    let (x, y) = harness.with_chat(|chat| {
        let regions = chat.clickable_regions.borrow();
        let region = regions
            .iter()
            .find(|region| region.action == ClickableAction::AcceptStartupModelMigration)
            .expect("expected startup migration accept region");
        let x = region.rect.x.saturating_add(region.rect.width.saturating_div(2));
        (x, region.rect.y)
    });

    harness.with_chat(|chat| chat.handle_click((x, y)));

    let events = harness.drain_events();
    assert!(events.into_iter().any(|event| matches!(
        event,
        AppEvent::AcceptStartupModelMigration(notice)
            if notice.target_model == "gpt-5.3-codex"
    )));
    }

    #[test]
    fn statusline_shortcut_f5_opens_network_settings() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.handle_key_event(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    });

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
        "expected Network settings view after F5, got:\n{output}",
    );
    }

    #[test]
    fn statusline_shortcut_f4_opens_shell_selector() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.handle_key_event(KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
    });

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
        output.contains("Select Shell"),
        "expected shell selector after F4, got:\n{output}",
    );
    }

    #[test]
    fn statusline_shortcut_f2_opens_model_selector() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        chat.remote_model_presets =
            Some(code_common::model_presets::builtin_model_presets(None, true));
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.handle_key_event(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    });

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
        output.contains("Select Model & Reasoning"),
        "expected model selector after F2, got:\n{output}",
    );
    }

    #[test]
    fn statusline_shortcut_f3_cycles_reasoning_effort() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    let preferred = harness.with_chat(|chat| {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.config.model_reasoning_effort = code_core::config_types::ReasoningEffort::None;
        chat.config.preferred_model_reasoning_effort = None;
        chat.handle_key_event(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
        chat.config.preferred_model_reasoning_effort
    });

    assert_eq!(
        preferred,
        Some(code_core::config_types::ReasoningEffort::Minimal),
        "expected F3 to set preferred reasoning effort",
    );
    }

    #[test]
    fn statusline_shortcut_remap_f6_opens_network_settings() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        chat.config.tui.hotkeys.network_settings = code_core::config_types::TuiHotkey::Function(
            code_core::config_types::FunctionKeyHotkey::F6,
        );
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.handle_key_event(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE));
    });

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
        "expected Network settings view after remapped F6, got:\n{output}",
    );
    }

    #[test]
    fn statusline_shortcut_ctrl_h_opens_network_settings() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();

    harness.with_chat(|chat| {
        chat.config.tui.hotkeys.network_settings =
            code_core::config_types::TuiHotkey::Chord(code_core::config_types::TuiHotkeyChord {
                ctrl: true,
                alt: false,
                key: 'h',
            });
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        chat.handle_key_event(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL));
    });

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
        "expected Network settings view after Ctrl+H, got:\n{output}",
    );
    }
