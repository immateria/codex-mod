fn make_app_info(
    id: &str,
    name: &str,
    is_accessible: bool,
    is_enabled: bool,
    install_url: Option<String>,
) -> code_app_server_protocol::AppInfo {
    code_app_server_protocol::AppInfo {
        id: id.to_string(),
        name: name.to_string(),
        description: None,
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url,
        is_accessible,
        is_enabled,
        plugin_display_names: Vec::new(),
    }
}

fn plain_message_text(state: &crate::history::state::PlainMessageState) -> String {
    state
        .lines
        .iter()
        .map(|line| {
            let mut out = String::new();
            for span in &line.spans {
                out.push_str(&span.text);
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn apps_picker_is_gated_when_apps_feature_disabled() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let _ = harness.drain_events();

    harness.with_chat(|chat| {
        chat.config
            .features_effective
            .entries
            .insert("apps".to_string(), false);
        chat.show_apps_picker();
    });

    let events = harness.drain_events();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AppEvent::FetchAppsDirectory { .. })),
        "expected no FetchAppsDirectory event when apps feature is disabled, got {events:?}",
    );

    let picker_open = harness.with_chat(|chat| chat.bottom_pane.is_list_selection_open("apps_picker"));
    assert!(!picker_open, "expected apps picker to remain closed");

    let notice_text = harness.with_chat(|chat| {
        let cell = chat.history_cells.last().expect("expected notice cell");
        let plain = cell
            .as_any()
            .downcast_ref::<crate::history_cell::PlainHistoryCell>()
            .expect("expected plain history cell");
        plain_message_text(plain.state())
    });
    assert!(
        notice_text.contains("Apps are disabled."),
        "expected notice cell to mention disabled apps, got:\n{notice_text}"
    );
    assert!(
        notice_text.contains("Enable in Settings -> Experimental."),
        "expected notice cell to mention Experimental settings, got:\n{notice_text}"
    );
}

#[test]
fn apps_picker_uninitialized_cache_triggers_fetch() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let _ = harness.drain_events();

    harness.with_chat(|chat| {
        chat.apps_directory_cache = AppsDirectoryCacheState::Uninitialized;
        chat.show_apps_picker();
    });

    let events = harness.drain_events();
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::FetchAppsDirectory { force_refetch: false }
        )),
        "expected FetchAppsDirectory(force_refetch=false), got {events:?}",
    );

    let picker_open = harness.with_chat(|chat| chat.bottom_pane.is_list_selection_open("apps_picker"));
    assert!(picker_open, "expected apps picker to open in loading state");
}

#[test]
fn apps_picker_installed_app_selection_inserts_dollar_slug() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let _ = harness.drain_events();

    let app = make_app_info(
        "gdrive",
        "Google Drive",
        /*is_accessible*/ true,
        /*is_enabled*/ true,
        Some("https://chatgpt.com/apps/google-drive/gdrive".to_string()),
    );
    let expected_insert = format!("${}", code_connectors::connector_mention_slug(&app));

    harness.with_chat(|chat| {
        chat.apps_directory_cache = AppsDirectoryCacheState::Ready(vec![app.clone()]);
        chat.show_apps_picker();
        chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    });

    let events = harness.drain_events();
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::InsertText { text } if text == &expected_insert
        )),
        "expected InsertText({expected_insert:?}), got {events:?}",
    );
}

#[test]
fn apps_picker_uninstalled_app_selection_opens_app_link_view() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let _ = harness.drain_events();

    let app = make_app_info(
        "calendar",
        "Google Calendar",
        /*is_accessible*/ false,
        /*is_enabled*/ false,
        Some("https://chatgpt.com/apps/google-calendar/calendar".to_string()),
    );

    harness.with_chat(|chat| {
        chat.apps_directory_cache = AppsDirectoryCacheState::Ready(vec![app.clone()]);
        chat.show_apps_picker();
        chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    });

    let events = harness.drain_events();
    let opened_id = events.iter().find_map(|event| match event {
        AppEvent::ShowAppLinkView { params } => Some(params.app.id.clone()),
        _ => None,
    });
    assert_eq!(opened_id.as_deref(), Some("calendar"));
}

#[test]
fn apps_directory_loaded_refreshes_open_picker_in_place() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let _ = harness.drain_events();

    harness.with_chat(|chat| {
        chat.apps_directory_cache = AppsDirectoryCacheState::Ready(Vec::new());
        chat.show_apps_picker();
        assert!(chat.bottom_pane.is_list_selection_open("apps_picker"));

        let apps = vec![make_app_info(
            "gdrive",
            "Google Drive",
            /*is_accessible*/ true,
            /*is_enabled*/ true,
            None,
        )];
        chat.apps_directory_apply_loaded(false, Ok(apps));
        assert!(chat.bottom_pane.is_list_selection_open("apps_picker"));
    });
}

#[test]
fn apps_directory_loaded_does_not_open_picker_when_closed() {
    let _guard = enter_test_runtime_guard();
    let mut harness = ChatWidgetHarness::new();
    let _ = harness.drain_events();

    harness.with_chat(|chat| {
        assert!(!chat.bottom_pane.is_list_selection_open("apps_picker"));

        let apps = vec![make_app_info(
            "gdrive",
            "Google Drive",
            /*is_accessible*/ true,
            /*is_enabled*/ true,
            None,
        )];
        chat.apps_directory_apply_loaded(false, Ok(apps));
        assert!(!chat.bottom_pane.is_list_selection_open("apps_picker"));
    });
}

