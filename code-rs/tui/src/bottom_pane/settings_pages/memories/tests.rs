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
    for _ in 0..14 {
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
    for _ in 0..14 {
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
    assert!(rendered.contains("Database:"));
}

#[test]
fn view_summary_shows_content_when_artifact_exists() {
    let (tx, _rx) = channel();
    let sender = AppEventSender::new(tx);
    let temp = tempdir().expect("tempdir");
    let mem_dir = temp.path().join("memories");
    std::fs::create_dir_all(&mem_dir).unwrap();
    std::fs::write(mem_dir.join("memory_summary.md"), "# Summary\nTest content").unwrap();

    let mut view = MemoriesSettingsView::new(
        temp.path().to_path_buf(),
        PathBuf::from("/tmp/project"),
        Some("work".to_string()),
        None,
        None,
        None,
        sender,
    );

    // Navigate to ViewSummary row (index 8)
    for _ in 0..8 {
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    assert_eq!(view.selected_row(), RowKind::ViewSummary);
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(matches!(view.mode, ViewMode::TextViewer(_)));
    if let ViewMode::TextViewer(ref viewer) = view.mode {
        assert_eq!(viewer.lines.len(), 2);
        assert_eq!(viewer.lines[0], "# Summary");
        assert_eq!(viewer.lines[1], "Test content");
    }

    // Esc goes back to Main
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(view.mode, ViewMode::Main));
}

#[test]
fn view_summary_shows_error_when_missing() {
    let (tx, _rx) = channel();
    let sender = AppEventSender::new(tx);
    let temp = tempdir().expect("tempdir");

    let mut view = MemoriesSettingsView::new(
        temp.path().to_path_buf(),
        PathBuf::from("/tmp/project"),
        Some("work".to_string()),
        None,
        None,
        None,
        sender,
    );

    for _ in 0..8 {
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Should stay in Main with error status
    assert!(matches!(view.mode, ViewMode::Main));
    assert!(view.status.as_ref().is_some_and(|(_, is_error)| *is_error));
}

#[test]
fn browse_rollouts_lists_files() {
    let (tx, _rx) = channel();
    let sender = AppEventSender::new(tx);
    let temp = tempdir().expect("tempdir");
    let rollouts_dir = temp.path().join("memories").join("rollout_summaries");
    std::fs::create_dir_all(&rollouts_dir).unwrap();
    std::fs::write(rollouts_dir.join("session-abc.md"), "rollout 1").unwrap();
    std::fs::write(rollouts_dir.join("session-xyz.md"), "rollout 2").unwrap();

    let mut view = MemoriesSettingsView::new(
        temp.path().to_path_buf(),
        PathBuf::from("/tmp/project"),
        Some("work".to_string()),
        None,
        None,
        None,
        sender,
    );

    // Navigate to BrowseRollouts (index 10)
    for _ in 0..10 {
        view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    assert_eq!(view.selected_row(), RowKind::BrowseRollouts);
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(matches!(view.mode, ViewMode::RolloutList(_)));
    if let ViewMode::RolloutList(ref list) = view.mode {
        assert_eq!(list.entries.len(), 2);
    }

    // Enter opens detail
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(view.mode, ViewMode::TextViewer(_)));
    if let ViewMode::TextViewer(ref viewer) = view.mode {
        assert!(matches!(viewer.parent, TextViewerParent::RolloutList(_)));
    }

    // Esc goes back to rollout list
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(view.mode, ViewMode::RolloutList(_)));

    // Another Esc goes back to Main
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(view.mode, ViewMode::Main));
}
