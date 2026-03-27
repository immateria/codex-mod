use super::*;

use std::sync::{Arc, Mutex, mpsc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;
use crate::chatwidget::{AppsAccountStatusState, AppsSharedState};

fn make_shared_state() -> Arc<Mutex<AppsSharedState>> {
    Arc::new(Mutex::new(AppsSharedState {
        active_profile: None,
        sources_snapshot: AppsSourcesToml::default(),
        accounts_snapshot: vec![crate::chatwidget::AppsAccountSnapshot {
            id: "acc1".to_string(),
            label: "Account 1".to_string(),
            is_chatgpt: true,
            is_active_model_account: true,
        }],
        status_by_account_id: Default::default(),
        pending_status_refresh_account_ids: None,
        action_in_progress: None,
        action_error: None,
    }))
}

#[test]
fn space_toggles_pin_and_ctrl_s_emits_set_apps_sources() {
    let shared_state = make_shared_state();
    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = AppsSettingsView::new(shared_state, app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)));
    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)));

    match rx.try_recv().expect("SetAppsSources") {
        AppEvent::SetAppsSources { sources } => {
            assert_eq!(sources.pinned_account_ids, vec!["acc1".to_string()]);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn r_requests_status_refresh_for_effective_sources() {
    let shared_state = make_shared_state();
    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = AppsSettingsView::new(shared_state, app_event_tx);

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)));

    match rx.try_recv().expect("FetchAppsStatus") {
        AppEvent::FetchAppsStatus { account_ids, force_refresh_tools } => {
            assert_eq!(account_ids, vec!["acc1".to_string()]);
            assert!(force_refresh_tools);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn l_in_detail_mode_only_handles_when_needs_login() {
    let shared_state = make_shared_state();
    {
        let mut state = shared_state.lock().unwrap_or_else(|err| err.into_inner());
        state.status_by_account_id.insert(
            "acc1".to_string(),
            AppsAccountStatusState::Failed {
                error: "Not logged in".to_string(),
                needs_login: true,
            },
        );
    }

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = AppsSettingsView::new(shared_state, app_event_tx);
    view.mode = Mode::AccountDetail { account_id: "acc1".to_string() };

    assert!(view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE)));

    match rx.try_recv().expect("OpenSettings") {
        AppEvent::OpenSettings { section } => {
            assert_eq!(section, Some(crate::bottom_pane::SettingsSection::Accounts));
        }
        other => panic!("unexpected event: {other:?}"),
    }
    match rx.try_recv().expect("ShowLoginAccounts") {
        AppEvent::ShowLoginAccounts => {}
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn r_does_nothing_when_active_account_is_not_chatgpt_and_no_pins() {
    let shared_state = Arc::new(Mutex::new(AppsSharedState {
        active_profile: None,
        sources_snapshot: AppsSourcesToml::default(),
        accounts_snapshot: vec![crate::chatwidget::AppsAccountSnapshot {
            id: "api1".to_string(),
            label: "API key".to_string(),
            is_chatgpt: false,
            is_active_model_account: true,
        }],
        status_by_account_id: Default::default(),
        pending_status_refresh_account_ids: None,
        action_in_progress: None,
        action_error: None,
    }));

    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let mut view = AppsSettingsView::new(shared_state, app_event_tx);

    assert!(!view.handle_key_event_direct(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)));
    assert!(rx.try_recv().is_err());
}
