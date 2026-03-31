use super::*;

use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event::AppEvent;

fn make_sender() -> (AppEventSender, mpsc::Receiver<AppEvent>) {
    let (tx, rx) = mpsc::channel();
    (AppEventSender::new(tx), rx)
}

fn make_entry_global(name: &str) -> SecretListEntry {
    SecretListEntry {
        scope: code_secrets::SecretScope::Global,
        name: code_secrets::SecretName::new(name).unwrap(),
    }
}

fn shared_state_ready(env_id: &str, entries: Vec<SecretListEntry>) -> Arc<Mutex<SecretsSharedState>> {
    let mut state = SecretsSharedState::default();
    state.list = crate::chatwidget::SecretsListState::Ready {
        env_id: env_id.to_string(),
        entries,
    };
    Arc::new(Mutex::new(state))
}

#[test]
fn new_requests_list_when_uninitialized() {
    let env_id = "env123".to_string();
    let shared_state = Arc::new(Mutex::new(SecretsSharedState::default()));
    let (app_event_tx, rx) = make_sender();

    let _view = SecretsSettingsView::new(shared_state, env_id.clone(), app_event_tx);

    let event = rx.recv().unwrap();
    assert!(matches!(event, AppEvent::FetchSecretsList { env_id: id } if id == env_id));
}

#[test]
fn delete_flow_emits_delete_secret() {
    let env_id = "env123".to_string();
    let entry = make_entry_global("OPENAI_API_KEY");
    let shared_state = shared_state_ready(&env_id, vec![entry.clone()]);
    let (app_event_tx, rx) = make_sender();

    let mut view = SecretsSettingsView::new(shared_state, env_id.clone(), app_event_tx);

    // No events should be emitted on construction when the snapshot is already ready.
    assert!(rx.try_recv().is_err());

    // Delete opens confirmation.
    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Delete,
        KeyModifiers::NONE,
    )));

    // Focus Delete.
    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Tab,
        KeyModifiers::NONE,
    )));

    // Confirm.
    assert!(view.handle_key_event_direct(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));

    let event = rx.recv().unwrap();
    assert!(
        matches!(
            event,
            AppEvent::DeleteSecret { env_id: id, entry: e } if id == env_id && e == entry
        ),
        "expected DeleteSecret for selected entry",
    );
}

