use super::*;

use std::path::PathBuf;
use std::sync::mpsc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::chrome::ChromeMode;

fn make_prompt(name: &str) -> CustomPrompt {
    CustomPrompt {
        name: name.to_string(),
        path: PathBuf::from(format!("/tmp/{name}.md")),
        content: "hello\nworld".to_string(),
        description: None,
        argument_hint: None,
    }
}

#[test]
fn content_only_mouse_hit_testing_uses_content_only_layout() {
    let (tx, _rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let prompts = vec![make_prompt("one")];
    let mut view = PromptsSettingsView::new(prompts, app_event_tx);

    // Select the "Add new…" row so a hover in row 0 must change selection.
    view.list_state.selected_idx = Some(view.prompts.len());
    view.list_state.scroll_top = 0;

    let area = Rect::new(0, 0, 60, 14);
    let content_layout = view
        .list_page()
        .layout_in_chrome(ChromeMode::ContentOnly, area)
        .expect("content layout");
    let framed_layout = view
        .list_page()
        .layout_in_chrome(ChromeMode::Framed, area)
        .expect("framed layout");

    let pos = Position {
        x: content_layout.body.x,
        y: content_layout.body.y,
    };
    assert!(content_layout.body.contains(pos));
    assert!(!framed_layout.body.contains(pos));

    let mouse_event = MouseEvent {
        kind: MouseEventKind::Moved,
        column: pos.x,
        row: pos.y,
        modifiers: KeyModifiers::NONE,
    };

    assert!(!view.handle_mouse_event_direct_framed(mouse_event, area));
    assert_eq!(view.list_state.selected_idx, Some(view.prompts.len()));

    assert!(view.handle_mouse_event_direct_content_only(mouse_event, area));
    assert_eq!(view.list_state.selected_idx, Some(0));
}

#[test]
fn list_navigation_scrolls_to_keep_selection_visible() {
    let (tx, _rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);

    let prompts = (0..30)
        .map(|idx| make_prompt(&format!("p{idx}")))
        .collect::<Vec<_>>();
    let mut view = PromptsSettingsView::new(prompts, app_event_tx);
    view.list_viewport_rows.set(3);

    for _ in 0..3 {
        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }

    assert_eq!(view.list_state.selected_idx, Some(3));
    assert_eq!(view.list_state.scroll_top, 1);
}

