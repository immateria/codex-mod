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
        code_core::config_types::IconMode::default(),
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
        code_core::config_types::IconMode::default(),
        app_event_tx,
    );

    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL
    )));
    assert!(rx.try_recv().is_err());
}

#[test]
fn icon_mode_cycle_does_not_mutate_global() {
    crate::icons::with_test_icon_mode(code_core::config_types::IconMode::Unicode, || {
        let (tx, _rx) = mpsc::channel();
        let app_event_tx = AppEventSender::new(tx);
        let mut view = InterfaceSettingsView::new(
            PathBuf::from("/tmp"),
            SettingsMenuConfig::default(),
            TuiHotkeysConfig::default(),
            code_core::config_types::IconMode::Unicode,
            app_event_tx,
        );

        // Cycling changes the local pending value but leaves the global untouched.
        view.cycle_icon_mode_next();
        assert_eq!(view.icon_mode, code_core::config_types::IconMode::NerdFonts);
        assert_eq!(
            crate::icons::icon_mode(),
            code_core::config_types::IconMode::Unicode,
            "global icon mode must not change until Apply"
        );

        // Revert restores local to baseline (global was never touched).
        view.revert_unapplied_live_previews();
        assert_eq!(view.icon_mode, view.icon_mode_baseline);
        assert_eq!(crate::icons::icon_mode(), code_core::config_types::IconMode::Unicode);
    });
}

#[test]
fn close_reverts_unapplied_compact_hints_preview() {
    fn compact_hint_suffix() -> String {
        crate::bottom_pane::settings_ui::hints::shortcut_line(&[
            crate::bottom_pane::settings_ui::hints::KeyHint::new("r", " refresh"),
        ])
        .spans
        .get(1)
        .map(|span| span.content.to_string())
        .unwrap_or_default()
    }

    crate::bottom_pane::settings_ui::hints::with_test_fuse_hint_key_labels(true, || {
        let (tx, _rx) = mpsc::channel();
        let app_event_tx = AppEventSender::new(tx);
        let mut view = InterfaceSettingsView::new(
            PathBuf::from("/tmp"),
            SettingsMenuConfig::default(),
            TuiHotkeysConfig::default(),
            code_core::config_types::IconMode::default(),
            app_event_tx,
        );

        assert_eq!(compact_hint_suffix(), "efresh");
        view.toggle_fuse_hint_key_labels();
        assert!(!view.settings.fuse_hint_key_labels);
        assert!(view.dirty_settings);
        assert_eq!(compact_hint_suffix(), " refresh");

        view.revert_unapplied_live_previews();
        assert!(view.settings.fuse_hint_key_labels);
        assert!(!view.dirty_settings);
        assert_eq!(compact_hint_suffix(), "efresh");
    });
}

#[test]
fn compact_hints_toggle_back_to_baseline_clears_dirty_and_save_event() {
    crate::bottom_pane::settings_ui::hints::with_test_fuse_hint_key_labels(true, || {
        let (tx, rx) = mpsc::channel();
        let app_event_tx = AppEventSender::new(tx);
        let mut view = InterfaceSettingsView::new(
            PathBuf::from("/tmp"),
            SettingsMenuConfig::default(),
            TuiHotkeysConfig::default(),
            code_core::config_types::IconMode::default(),
            app_event_tx,
        );

        view.toggle_fuse_hint_key_labels();
        view.toggle_fuse_hint_key_labels();

        assert!(view.settings.fuse_hint_key_labels);
        assert!(
            !view.dirty_settings,
            "returning to the baseline value should clear Pending state",
        );

        view.apply_settings();
        assert!(
            rx.try_recv().is_err(),
            "no settings event should be emitted when the value is back at baseline",
        );
    });
}
