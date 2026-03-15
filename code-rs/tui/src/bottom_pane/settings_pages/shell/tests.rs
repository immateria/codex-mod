use super::*;
use std::sync::mpsc;

use crate::app_event::AppEvent;
use crate::bottom_pane::SettingsSection;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn preset(command: &str) -> ShellPreset {
    ShellPreset {
        id: command.to_string(),
        command: command.to_string(),
        display_name: command.to_string(),
        description: String::new(),
        default_args: Vec::new(),
        script_style: None,
        show_in_picker: true,
    }
}

fn shell(path: &str) -> ShellConfig {
    ShellConfig {
        path: path.to_string(),
        args: Vec::new(),
        script_style: None,
        command_safety: code_core::config_types::CommandSafetyProfileConfig::default(),
        dangerous_command_detection: None,
    }
}

#[test]
fn matches_termux_bash_path_to_bash_preset() {
    assert!(ShellSelectionView::current_matches_preset(
        &shell("/data/data/com.termux/files/usr/bin/bash"),
        &preset("bash"),
    ));
}

#[test]
fn does_not_match_unrelated_basename() {
    assert!(!ShellSelectionView::current_matches_preset(
        &shell("/usr/bin/bashful"),
        &preset("bash"),
    ));
}

#[test]
fn matches_windows_exe_suffix() {
    assert!(ShellSelectionView::current_matches_preset(
        &shell("C:\\Program Files\\PowerShell\\7\\pwsh.exe"),
        &preset("pwsh"),
    ));
}

#[test]
fn list_mode_enter_on_auto_clears_override_and_closes_confirmed() {
    let (tx, rx) = mpsc::channel::<AppEvent>();
    let mut view = ShellSelectionView::new(
        Some(shell("bash")),
        vec![preset("bash")],
        AppEventSender::new(tx),
    );
    view.selected_index = 0;
    assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Enter)));

    match rx.recv().expect("update selection") {
        AppEvent::UpdateShellSelection { path, .. } => {
            assert_eq!(path, "-");
        }
        other => panic!("unexpected event: {other:?}"),
    }
    match rx.recv().expect("closed") {
        AppEvent::ShellSelectionClosed { confirmed } => assert!(confirmed),
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn ctrl_p_opens_shell_profiles_settings_and_closes_picker() {
    let (tx, rx) = mpsc::channel::<AppEvent>();
    let mut view = ShellSelectionView::new(None, vec![preset("bash")], AppEventSender::new(tx));
    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )));

    match rx.recv().expect("closed") {
        AppEvent::ShellSelectionClosed { confirmed } => assert!(!confirmed),
        other => panic!("unexpected event: {other:?}"),
    }
    match rx.recv().expect("open settings") {
        AppEvent::OpenSettings { section } => {
            assert_eq!(section, Some(SettingsSection::ShellProfiles));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn edit_mode_tab_switches_focus_and_enter_activates_back_action() {
    let (tx, rx) = mpsc::channel::<AppEvent>();
    let mut view = ShellSelectionView::new(None, vec![preset("bash")], AppEventSender::new(tx));
    view.open_custom_input_with_prefill("bash".to_string(), None);
    assert!(view.custom_input_mode);
    assert_eq!(view.edit_focus, EditFocus::Field);

    assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Tab)));
    assert_eq!(view.edit_focus, EditFocus::Actions);

    // Move selection to Back.
    for _ in 0..5 {
        assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Right)));
    }
    assert_eq!(view.selected_action, EditAction::Back);
    assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Enter)));
    assert!(!view.custom_input_mode);
    assert!(rx.try_recv().is_err());
}

#[test]
fn edit_mode_apply_action_submits_and_closes() {
    let (tx, rx) = mpsc::channel::<AppEvent>();
    let mut view = ShellSelectionView::new(None, vec![preset("bash")], AppEventSender::new(tx));
    view.open_custom_input_with_prefill("bash".to_string(), None);
    assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Tab)));
    assert_eq!(view.edit_focus, EditFocus::Actions);
    assert_eq!(view.selected_action, EditAction::Apply);
    assert!(view.handle_key_event_direct(KeyEvent::from(KeyCode::Enter)));

    match rx.recv().expect("update selection") {
        AppEvent::UpdateShellSelection { path, args, .. } => {
            assert_eq!(path, "bash");
            assert!(args.is_empty());
        }
        other => panic!("unexpected event: {other:?}"),
    }
    match rx.recv().expect("closed") {
        AppEvent::ShellSelectionClosed { confirmed } => assert!(confirmed),
        other => panic!("unexpected event: {other:?}"),
    }
}
