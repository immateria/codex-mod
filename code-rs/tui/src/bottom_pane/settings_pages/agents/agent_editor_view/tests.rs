use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app_event_sender::AppEventSender;

use super::model::{FIELD_COMMAND, FIELD_TOGGLE};
use super::{AgentEditorInit, AgentEditorView};

#[test]
fn field_order_matches_visual_layout() {
    let (app_event_tx_raw, _app_event_rx) = std::sync::mpsc::channel();
    let app_event_tx = AppEventSender::new(app_event_tx_raw);
    let mut view = AgentEditorView::new(AgentEditorInit {
        name: "coder".to_string(),
        enabled: true,
        args_read_only: None,
        args_write: None,
        instructions: None,
        description: Some("desc".to_string()),
        command: "coder".to_string(),
        builtin: true,
        app_event_tx,
    });

    view.field = FIELD_COMMAND;
    view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(view.field, FIELD_TOGGLE);
}

