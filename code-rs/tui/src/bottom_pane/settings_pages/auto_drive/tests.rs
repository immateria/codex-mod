use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::sync::mpsc::channel;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn build_view(
    model_routing_enabled: bool,
    entries: Vec<AutoDriveModelRoutingEntry>,
) -> AutoDriveSettingsView {
    let (tx, _rx) = channel();
    AutoDriveSettingsView::new(AutoDriveSettingsInit {
        app_event_tx: AppEventSender::new(tx),
        model: "gpt-5.3-codex".to_string(),
        model_reasoning: ReasoningEffort::High,
        use_chat_model: false,
        review_enabled: true,
        agents_enabled: true,
        cross_check_enabled: true,
        qa_automation_enabled: true,
        model_routing_enabled,
        model_routing_entries: entries,
        routing_model_options: vec![
            "gpt-5.3-codex".to_string(),
            "gpt-5.3-codex-spark".to_string(),
        ],
        continue_mode: AutoContinueMode::Manual,
    })
}

#[test]
fn routing_list_keeps_one_entry_enabled_when_routing_on() {
    let mut view = build_view(
        true,
        vec![AutoDriveModelRoutingEntry {
            model: "gpt-5.3-codex".to_string(),
            enabled: true,
            reasoning_levels: vec![ReasoningEffort::High],
            description: String::new(),
        }],
    );

    for _ in 0..4 {
        view.handle_key_event_direct(key(KeyCode::Down));
    }
    view.handle_key_event_direct(key(KeyCode::Enter));
    view.handle_key_event_direct(key(KeyCode::Char(' ')));

    assert!(view.model_routing_entries[0].enabled);
}

#[test]
fn routing_list_supports_add_and_save_entry() {
    let mut view = build_view(
        true,
        vec![AutoDriveModelRoutingEntry {
            model: "gpt-5.3-codex".to_string(),
            enabled: true,
            reasoning_levels: vec![ReasoningEffort::High],
            description: String::new(),
        }],
    );

    for _ in 0..4 {
        view.handle_key_event_direct(key(KeyCode::Down));
    }
    view.handle_key_event_direct(key(KeyCode::Enter));
    view.handle_key_event_direct(key(KeyCode::Char('a')));

    for _ in 0..3 {
        view.handle_key_event_direct(key(KeyCode::Down));
    }
    for ch in "fast loop".chars() {
        view.handle_key_event_direct(key(KeyCode::Char(ch)));
    }
    view.handle_key_event_direct(key(KeyCode::Down));
    view.handle_key_event_direct(key(KeyCode::Enter));

    assert_eq!(view.model_routing_entries.len(), 2);
    assert_eq!(view.model_routing_entries[1].description, "fast loop");
}

#[test]
fn routing_toggle_requires_enabled_entry_before_turning_on() {
    let mut view = build_view(
        false,
        vec![AutoDriveModelRoutingEntry {
            model: "gpt-5.3-codex".to_string(),
            enabled: false,
            reasoning_levels: vec![ReasoningEffort::High],
            description: String::new(),
        }],
    );

    for _ in 0..3 {
        view.handle_key_event_direct(key(KeyCode::Down));
    }
    view.handle_key_event_direct(key(KeyCode::Enter));

    assert!(!view.model_routing_enabled);
    assert_eq!(
        view.status_message.as_deref(),
        Some("Enable at least one routing entry before turning routing on.")
    );
}

#[test]
fn routing_editor_rejects_save_without_reasoning() {
    let mut view = build_view(
        true,
        vec![AutoDriveModelRoutingEntry {
            model: "gpt-5.3-codex".to_string(),
            enabled: true,
            reasoning_levels: vec![ReasoningEffort::High],
            description: String::new(),
        }],
    );

    for _ in 0..4 {
        view.handle_key_event_direct(key(KeyCode::Down));
    }
    view.handle_key_event_direct(key(KeyCode::Enter));
    view.handle_key_event_direct(key(KeyCode::Char('a')));

    view.handle_key_event_direct(key(KeyCode::Down));
    view.handle_key_event_direct(key(KeyCode::Down));
    for _ in 0..3 {
        view.handle_key_event_direct(key(KeyCode::Right));
    }
    view.handle_key_event_direct(key(KeyCode::Enter));
    view.handle_key_event_direct(key(KeyCode::Down));
    view.handle_key_event_direct(key(KeyCode::Down));
    view.handle_key_event_direct(key(KeyCode::Enter));

    assert!(matches!(view.mode, AutoDriveSettingsMode::RoutingEditor(_)));
    assert_eq!(view.model_routing_entries.len(), 1);
    assert_eq!(
        view.status_message.as_deref(),
        Some("Select at least one reasoning level.")
    );
}

#[test]
fn routing_editor_cannot_disable_last_enabled_entry_when_routing_on() {
    let mut view = build_view(
        true,
        vec![AutoDriveModelRoutingEntry {
            model: "gpt-5.3-codex".to_string(),
            enabled: true,
            reasoning_levels: vec![ReasoningEffort::High],
            description: String::new(),
        }],
    );

    for _ in 0..4 {
        view.handle_key_event_direct(key(KeyCode::Down));
    }
    view.handle_key_event_direct(key(KeyCode::Enter));
    view.handle_key_event_direct(key(KeyCode::Enter));

    view.handle_key_event_direct(key(KeyCode::Down));
    view.handle_key_event_direct(key(KeyCode::Enter));
    view.handle_key_event_direct(key(KeyCode::Down));
    view.handle_key_event_direct(key(KeyCode::Down));
    view.handle_key_event_direct(key(KeyCode::Down));
    view.handle_key_event_direct(key(KeyCode::Enter));

    assert!(matches!(view.mode, AutoDriveSettingsMode::RoutingEditor(_)));
    assert!(view.model_routing_entries[0].enabled);
    assert_eq!(
        view.status_message.as_deref(),
        Some("At least one routing entry must stay enabled.")
    );
}

#[test]
fn routing_list_scrolls_to_keep_selection_visible() {
    let entries = (0..10)
        .map(|idx| AutoDriveModelRoutingEntry {
            model: format!("gpt-5.3-codex-{idx}"),
            enabled: true,
            reasoning_levels: vec![ReasoningEffort::High],
            description: String::new(),
        })
        .collect::<Vec<_>>();
    let mut view = build_view(true, entries);

    for _ in 0..4 {
        view.handle_key_event_direct(key(KeyCode::Down));
    }
    view.handle_key_event_direct(key(KeyCode::Enter));
    assert!(matches!(view.mode, AutoDriveSettingsMode::RoutingList));

    view.routing_state.selected_idx = Some(0);
    view.routing_state.scroll_top = 0;
    view.routing_viewport_rows.set(3);

    for _ in 0..3 {
        view.handle_key_event_direct(key(KeyCode::Down));
    }

    assert_eq!(view.routing_state.selected_idx, Some(3));
    assert_eq!(view.routing_state.scroll_top, 1);
}

#[test]
fn content_only_routing_list_mouse_geometry_differs_from_framed() {
    let entries = (0..3)
        .map(|idx| AutoDriveModelRoutingEntry {
            model: format!("gpt-5.3-codex-{idx}"),
            enabled: true,
            reasoning_levels: vec![ReasoningEffort::High],
            description: String::new(),
        })
        .collect::<Vec<_>>();
    let mut view = build_view(true, entries);

    for _ in 0..4 {
        view.handle_key_event_direct(key(KeyCode::Down));
    }
    view.handle_key_event_direct(key(KeyCode::Enter));
    assert!(matches!(view.mode, AutoDriveSettingsMode::RoutingList));

    view.routing_state.selected_idx = Some(0);
    view.routing_state.scroll_top = 0;

    let area = Rect::new(0, 0, 50, 20);
    let content_layout = view
        .page()
        .layout_in_chrome(crate::bottom_pane::chrome::ChromeMode::ContentOnly, area)
        .expect("layout");

    let mouse_event = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: content_layout.body.x,
        row: content_layout.body.y,
        modifiers: KeyModifiers::NONE,
    };

    let before = view.routing_state.selected_idx;
    assert!(!view.handle_mouse_event_internal(mouse_event, area));
    assert_eq!(view.routing_state.selected_idx, before);

    assert!(view.handle_mouse_event_direct_content_only(mouse_event, area));
    assert_eq!(view.routing_state.selected_idx, Some(1));
}
