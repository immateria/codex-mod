use super::*;

use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;

fn make_view(app_event_tx: AppEventSender) -> ShellEscalationSettingsView {
    ShellEscalationSettingsView::new(
        Some("default".to_string()),
        false,
        None,
        None,
        None,
        app_event_tx,
    )
}

#[test]
fn toggling_and_editing_marks_dirty() {
    let (tx, _rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = make_view(app_event_tx);

    assert!(!view.dirty);
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
    assert!(view.dirty);

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(matches!(view.mode, ViewMode::EditText { .. }));

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(matches!(view.mode, ViewMode::Main));
    assert!(view.dirty);
}

#[test]
fn ctrl_s_emits_update_shell_escalation_settings_with_trimmed_options() {
    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = make_view(app_event_tx);

    view.enabled = true;
    view.zsh_path = Some("  /opt/zsh  ".to_string());
    view.wrapper_override = Some("   ".to_string());

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL
    )));

    match rx.try_recv().expect("UpdateShellEscalationSettings") {
        AppEvent::UpdateShellEscalationSettings {
            enabled,
            zsh_path,
            wrapper,
        } => {
            assert!(enabled);
            assert_eq!(zsh_path.as_deref(), Some("/opt/zsh"));
            assert_eq!(wrapper, None);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn edit_mode_esc_cancels_without_emitting() {
    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = make_view(app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(matches!(view.mode, ViewMode::EditText { .. }));

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(matches!(view.mode, ViewMode::Main));
    assert_eq!(view.zsh_path, None);
    assert!(rx.try_recv().is_err());
}

#[test]
fn esc_from_main_marks_view_complete() {
    let (tx, _rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = make_view(app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(view.is_complete());
}

#[test]
fn status_includes_shell_override_problem() {
    let (tx, _rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);

    let shell = ShellConfig {
        path: "/bin/bash".to_string(),
        args: Vec::new(),
        script_style: None,
        command_safety: Default::default(),
        dangerous_command_detection: None,
    };

    let view = ShellEscalationSettingsView::new(
        Some("default".to_string()),
        true,
        Some(shell),
        Some(std::path::PathBuf::from("/opt/zsh-patched")),
        None,
        app_event_tx,
    );

    let status_text = view
        .status_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        status_text.contains("Shell override is not zsh"),
        "expected shell override reason, got:\n{status_text}"
    );
}
