use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
