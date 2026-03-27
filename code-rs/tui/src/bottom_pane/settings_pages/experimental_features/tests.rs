use super::*;

use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;

fn selected_scroll_state(idx: usize) -> ScrollState {
    let mut state = ScrollState::new();
    state.selected_idx = Some(idx);
    state
}

#[test]
fn ctrl_s_emits_update_feature_flags_with_selected_toggle() {
    let mut features = FeaturesToml::default();
    features.entries.insert("apps".to_string(), true);

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = ExperimentalFeaturesSettingsView::new(None, features, app_event_tx);

    let apps_idx = view
        .rows
        .iter()
        .position(|row| row.key == "apps")
        .expect("apps feature row");
    view.list_state.set(selected_scroll_state(apps_idx));

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)));

    match rx.try_recv().expect("UpdateFeatureFlags") {
        AppEvent::UpdateFeatureFlags { updates } => {
            assert_eq!(updates.get("apps").copied(), Some(false));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn esc_marks_view_complete() {
    let features = FeaturesToml::default();
    let (tx, _rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = ExperimentalFeaturesSettingsView::new(None, features, app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(view.is_complete());
}

