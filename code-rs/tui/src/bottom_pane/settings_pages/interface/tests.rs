use super::*;

use std::path::PathBuf;
use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;

#[test]
fn ctrl_s_saves_settings_menu_config_when_dirty() {
    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = InterfaceSettingsView::new(
        PathBuf::from("/tmp"),
        SettingsMenuConfig::default(),
        TuiHotkeysConfig::default(),
        app_event_tx,
    );

    // The first row is Settings menu open mode, so Right cycles to Bottom.
    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::NONE
    )));
    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL
    )));

    match rx.try_recv().expect("SetTuiSettingsMenuConfig") {
        AppEvent::SetTuiSettingsMenuConfig(settings) => {
            assert_eq!(settings.open_mode, code_core::config_types::SettingsMenuOpenMode::Bottom);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn ctrl_s_with_no_changes_emits_no_events() {
    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = InterfaceSettingsView::new(
        PathBuf::from("/tmp"),
        SettingsMenuConfig::default(),
        TuiHotkeysConfig::default(),
        app_event_tx,
    );

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL
    )));
    assert!(rx.try_recv().is_err());
}

