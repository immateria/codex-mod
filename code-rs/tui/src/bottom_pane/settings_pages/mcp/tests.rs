    use std::collections::BTreeMap;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    use super::{
        McpPaneHit,
        McpServerRow,
        McpSettingsFocus,
        McpSettingsView,
        McpToolHoverPart,
        McpViewLayout,
    };
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use crate::bottom_pane::ChromeMode;
    use code_core::config_types::McpDispatchMode;

    fn make_server_row(name: &str) -> McpServerRow {
        McpServerRow {
            name: name.to_string(),
            enabled: true,
            transport: "npx -y test-server --transport stdio".to_string(),
            auth_status: super::McpAuthStatus::Unsupported,
            startup_timeout: Some(Duration::from_secs(30)),
            tool_timeout: Some(Duration::from_secs(30)),
            scheduling: super::McpServerSchedulingToml::default(),
            tool_scheduling: BTreeMap::new(),
            tools: vec!["tool_a".to_string(), "tool_b".to_string()],
            disabled_tools: Vec::new(),
            resources: Vec::new(),
            resource_templates: Vec::new(),
            tool_definitions: BTreeMap::new(),
            failure: Some(
                "very long failure text that should wrap and produce vertical overflow ".repeat(12),
            ),
            status: "Failed to start".to_string(),
        }
    }

    fn make_view(rows: Vec<McpServerRow>) -> McpSettingsView {
        let (tx, _rx) = channel();
        McpSettingsView::new(rows, AppEventSender::new(tx))
    }

    fn make_view_with_rx(rows: Vec<McpServerRow>) -> (McpSettingsView, std::sync::mpsc::Receiver<AppEvent>) {
        let (tx, rx) = channel();
        (McpSettingsView::new(rows, AppEventSender::new(tx)), rx)
    }

    fn mouse_event(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn key_routing_returns_false_for_unhandled_key() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let handled = view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!handled);
    }

    #[test]
    fn server_scheduling_editor_emits_app_event() {
        let (mut view, rx) = make_view_with_rx(vec![make_server_row("server_a")]);

        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)));
        // Dispatch row -> parallel
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

        // Max concurrent row -> set to 2
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)));
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE)));

        // Min interval row -> set to 1 sec
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)));

        // Save
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)));

        let event = rx.try_recv().expect("expected a scheduling update AppEvent");
        match event {
            AppEvent::SetMcpServerScheduling { server, scheduling } => {
                assert_eq!(server, "server_a");
                assert_eq!(scheduling.dispatch, McpDispatchMode::Parallel);
                assert_eq!(scheduling.max_concurrent, 2);
                assert_eq!(scheduling.min_interval_sec, Some(Duration::from_secs(1)));
                assert_eq!(scheduling.queue_timeout_sec, None);
                assert_eq!(scheduling.max_queue_depth, None);
            }
            other => panic!("unexpected AppEvent: {other:?}"),
        }
    }

    #[test]
    fn tool_scheduling_editor_emits_app_event() {
        let (mut view, rx) = make_view_with_rx(vec![make_server_row("server_a")]);

        // Focus tools pane.
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(view.focus, McpSettingsFocus::Tools);

        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)));

        // Min interval row toggles inherit -> override (defaults to 1); set to 2.
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)));
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE)));

        // Save
        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)));

        let event = rx.try_recv().expect("expected a tool scheduling update AppEvent");
        match event {
            AppEvent::SetMcpToolSchedulingOverride { server, tool, override_cfg } => {
                assert_eq!(server, "server_a");
                assert_eq!(tool, "tool_a");
                let cfg = override_cfg.expect("override cfg should be set");
                assert_eq!(cfg.max_concurrent, None);
                assert_eq!(cfg.min_interval_sec, Some(Duration::from_secs(2)));
            }
            other => panic!("unexpected AppEvent: {other:?}"),
        }
    }

    #[test]
    fn wheel_over_summary_scrolls_details_and_sets_focus() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");

        let event = mouse_event(
            MouseEventKind::ScrollDown,
            layout.summary_inner.x.saturating_add(1),
            layout.summary_inner.y.saturating_add(1),
        );

        assert!(view.framed_mut().handle_mouse_event_direct(event, area));
        assert_eq!(view.focus, McpSettingsFocus::Summary);
        assert!(view.summary_scroll_top > 0);
    }

    #[test]
    fn stacked_layout_scrolls_focused_pane_into_view() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 60, 14);
        view.last_render.set(area, ChromeMode::Framed);

        assert_eq!(view.focus, McpSettingsFocus::Servers);
        assert_eq!(view.stacked_scroll_top, 0);

        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(view.focus, McpSettingsFocus::Summary);
        assert!(view.stacked_scroll_top > 0);

        assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(view.focus, McpSettingsFocus::Tools);
        assert!(view.stacked_scroll_top > 0);

        let layout =
            McpViewLayout::from_area_with_scroll(area, view.stacked_scroll_top).expect("layout");
        assert!(layout.stacked);
        assert!(layout.tools_rect.height > 0);
    }

    #[test]
    fn scrolling_up_from_bottom_sentinel_moves_summary_view() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let viewport = Rect::new(0, 0, 38, 6);
        let metrics = view.summary_metrics_for_viewport(viewport);
        let max_scroll = metrics.total_lines.saturating_sub(metrics.visible_lines);
        assert!(max_scroll > 0);

        view.summary_scroll_top = usize::MAX;
        view.scroll_summary_lines(-1);

        assert_eq!(view.summary_scroll_top, max_scroll.saturating_sub(1));
    }

    #[test]
    fn mouse_move_only_updates_hover_not_focus_or_selection() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");
        let initial_focus = view.focus;
        let initial_selected = view.selected;

        let event = mouse_event(
            MouseEventKind::Moved,
            layout.summary_inner.x.saturating_add(1),
            layout.summary_inner.y.saturating_add(1),
        );

        assert!(view.framed_mut().handle_mouse_event_direct(event, area));
        assert_eq!(view.focus, initial_focus);
        assert_eq!(view.selected, initial_selected);
        assert_eq!(view.hovered_pane, McpPaneHit::Summary);
    }

    #[test]
    fn content_only_mouse_geometry_differs_from_framed() {
        let area = Rect::new(0, 0, 100, 24);
        let event = mouse_event(MouseEventKind::Moved, area.x, area.y);

        let mut framed_view = make_view(vec![make_server_row("server_a")]);
        assert!(!framed_view.framed_mut().handle_mouse_event_direct(event, area));
        assert_eq!(framed_view.hovered_pane, McpPaneHit::Outside);

        let mut content_view = make_view(vec![make_server_row("server_a")]);
        assert!(content_view
            .content_only_mut()
            .handle_mouse_event_direct(event, area));
        assert_eq!(content_view.hovered_pane, McpPaneHit::Servers);
    }

    #[test]
    fn stacked_focus_scroll_uses_last_render_chrome_mode() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 3, 10);

        view.last_render.set(area, ChromeMode::ContentOnly);

        assert_eq!(view.focus, McpSettingsFocus::Servers);
        assert_eq!(view.stacked_scroll_top, 0);

        assert!(view.handle_key_event_direct(KeyEvent::new(
            KeyCode::Tab,
            KeyModifiers::NONE
        )));
        assert_eq!(view.focus, McpSettingsFocus::Summary);
        assert!(view.stacked_scroll_top > 0);
    }

    #[test]
    fn mouse_move_over_server_row_updates_list_hover() {
        let mut view = make_view(vec![make_server_row("server_a"), make_server_row("server_b")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");
        let initial_focus = view.focus;
        let initial_selected = view.selected;

        let hover_y = (layout.list_inner.y..layout.list_inner.y.saturating_add(layout.list_inner.height))
            .find(|row| view.server_index_at_mouse_row(layout.list_inner, *row) == Some(1))
            .expect("row hit for second server");
        let event = mouse_event(
            MouseEventKind::Moved,
            layout.list_inner.x.saturating_add(3),
            hover_y,
        );

        assert!(view.framed_mut().handle_mouse_event_direct(event, area));
        assert_eq!(view.focus, initial_focus);
        assert_eq!(view.selected, initial_selected);
        assert_eq!(view.hovered_pane, McpPaneHit::Servers);
        assert_eq!(view.hovered_list_index, Some(1));
    }

    #[test]
    fn server_list_is_single_line_per_server_without_summary_row() {
        let view = make_view(vec![make_server_row("server_a")]);
        let lines = view.list_lines(80);
        let line_text: Vec<String> = lines.iter().map(ToString::to_string).collect();
        assert!(line_text.iter().any(|line| line.contains("[on ] server_a")));
        assert!(
            !line_text
                .iter()
                .any(|line| line.contains("test-server --transport stdio"))
        );
    }

    #[test]
    fn server_row_click_requires_second_click_to_toggle() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");

        let click_y = (layout.list_inner.y..layout.list_inner.y.saturating_add(layout.list_inner.height))
            .find(|row| view.server_index_at_mouse_row(layout.list_inner, *row) == Some(0))
            .expect("row hit for first server");
        let click_event = mouse_event(
            MouseEventKind::Down(MouseButton::Left),
            layout.list_rect.x.saturating_add(1),
            click_y,
        );

        assert!(view.rows[0].enabled);
        assert!(view.framed_mut().handle_mouse_event_direct(click_event, area));
        assert!(view.rows[0].enabled);
        assert!(view.framed_mut().handle_mouse_event_direct(click_event, area));
        assert!(!view.rows[0].enabled);
    }

    #[test]
    fn tool_hover_distinguishes_toggle_and_expand_controls() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let layout =
            McpViewLayout::from_area_with_scroll(area, 0).expect("layout should exist");

        let tool_row_y =
            (layout.tools_inner.y..layout.tools_inner.y.saturating_add(layout.tools_inner.height))
                .find(|row| view.tool_index_at_mouse_row(layout.tools_inner, *row) == Some(0))
            .expect("row hit for first tool");

        let toggle_hover = mouse_event(
            MouseEventKind::Moved,
            layout.tools_inner.x.saturating_add(2),
            tool_row_y,
        );
        assert!(view.framed_mut().handle_mouse_event_direct(toggle_hover, area));
        assert_eq!(view.hovered_pane, McpPaneHit::Tools);
        assert_eq!(view.hovered_tool_index, Some(0));
        assert_eq!(view.hovered_tool_part, Some(McpToolHoverPart::Toggle));

        let expand_hover = mouse_event(
            MouseEventKind::Moved,
            layout.tools_inner.x.saturating_add(6),
            tool_row_y,
        );
        assert!(view.framed_mut().handle_mouse_event_direct(expand_hover, area));
        assert_eq!(view.hovered_tool_index, Some(0));
        assert_eq!(view.hovered_tool_part, Some(McpToolHoverPart::Expand));
    }

    #[test]
    fn wheel_outside_panes_does_not_mutate_state() {
        let mut view = make_view(vec![make_server_row("server_a")]);
        let area = Rect::new(0, 0, 100, 24);
        let initial_selected = view.selected;
        let initial_scroll = view.summary_scroll_top;

        let event = mouse_event(
            MouseEventKind::ScrollDown,
            area.x.saturating_add(area.width).saturating_add(1),
            area.y,
        );
        assert!(!view.framed_mut().handle_mouse_event_direct(event, area));
        assert_eq!(view.selected, initial_selected);
        assert_eq!(view.summary_scroll_top, initial_scroll);
    }

    #[test]
    fn summary_shows_resources_and_resource_templates() {
        let mut row = make_server_row("server_a");
        row.failure = None;
        row.resources = vec![code_protocol::mcp::Resource {
            annotations: None,
            description: Some("Primary docs".to_string()),
            mime_type: Some("text/markdown".to_string()),
            name: "docs".to_string(),
            size: Some(42),
            title: None,
            uri: "file:///docs/readme.md".to_string(),
            icons: None,
            meta: None,
        }];
        row.resource_templates = vec![code_protocol::mcp::ResourceTemplate {
            annotations: None,
            uri_template: "file:///docs/{slug}.md".to_string(),
            name: "docs-template".to_string(),
            title: None,
            description: Some("Parameterized docs".to_string()),
            mime_type: Some("text/markdown".to_string()),
        }];

        let view = make_view(vec![row]);
        let lines = view.summary_lines();
        let text = lines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("Resources (1)"));
        assert!(text.contains("- docs (file:///docs/readme.md) · text/markdown · 42 bytes"));
        assert!(text.contains("Resource Templates (1)"));
        assert!(text.contains("- docs-template (file:///docs/{slug}.md) · text/markdown"));
    }
