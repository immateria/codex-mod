use super::*;

use std::path::PathBuf;
use std::sync::mpsc::channel;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::tempdir;

use crate::app_event::{AppEvent, MemoriesSettingsScope};
use crate::app_event_sender::AppEventSender;

fn make_view(sender: AppEventSender) -> MemoriesSettingsView {
    MemoriesSettingsView::new(
        PathBuf::from("/tmp/code-home"),
        PathBuf::from("/tmp/project"),
        Some("work".to_string()),
        None,
        None,
        None,
        sender,
    )
}

#[test]
fn apply_emits_global_memories_settings_event() {
    let (tx, rx) = channel();
    let sender = AppEventSender::new(tx);
    let mut view = make_view(sender);

    view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    for _ in 0..10 {
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match rx.recv().expect("app event") {
        AppEvent::SetMemoriesSettings { scope, settings } => {
            assert_eq!(scope, MemoriesSettingsScope::Global);
            assert_eq!(settings.generate_memories, Some(false));
        }
        other => panic!("expected SetMemoriesSettings event, got: {other:?}"),
    }
}

#[test]
fn apply_can_target_project_scope() {
    let (tx, rx) = channel();
    let sender = AppEventSender::new(tx);
    let mut view = make_view(sender);

    view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    for _ in 0..10 {
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match rx.recv().expect("app event") {
        AppEvent::SetMemoriesSettings { scope, settings } => {
            assert_eq!(
                scope,
                MemoriesSettingsScope::Project {
                    path: PathBuf::from("/tmp/project"),
                }
            );
            assert_eq!(settings.generate_memories, Some(false));
        }
        other => panic!("expected SetMemoriesSettings event, got: {other:?}"),
    }
}

#[tokio::test]
async fn header_shows_loading_when_db_exists_but_status_snapshot_is_missing() {
    let (tx, _rx) = channel();
    let sender = AppEventSender::new(tx);
    let temp = tempdir().expect("tempdir");
    tokio::fs::write(temp.path().join("memories_state.sqlite"), "")
        .await
        .expect("create db marker");
    let view = MemoriesSettingsView::new(
        temp.path().to_path_buf(),
        PathBuf::from("/tmp/project"),
        Some("work".to_string()),
        None,
        None,
        None,
        sender,
    );

    let rendered = view
        .render_header_lines()
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Memories status loading"));
}

#[tokio::test]
async fn header_shows_cached_snapshot_after_status_load() {
    let (tx, _rx) = channel();
    let sender = AppEventSender::new(tx);
    let temp = tempdir().expect("tempdir");
    tokio::fs::write(temp.path().join("memories_state.sqlite"), "")
        .await
        .expect("create db marker");
    code_core::load_memories_status(temp.path(), None, None, None)
        .await
        .expect("seed memories status cache");

    let view = MemoriesSettingsView::new(
        temp.path().to_path_buf(),
        PathBuf::from("/tmp/project"),
        Some("work".to_string()),
        None,
        None,
        None,
        sender,
    );

    let rendered = view
        .render_header_lines()
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rendered.contains("Memories status loading"));
    assert!(rendered.contains("SQLite: present"));
}

