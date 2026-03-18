use super::*;

use crate::app_event::AppEvent;
use crate::bottom_pane::chrome::ChromeMode;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::sync::mpsc::channel;

fn mouse_left_click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn mouse_click_on_main_row_toggles_auto_switch() {
    let (tx, rx) = channel();
    let mut view = AccountSwitchSettingsView::new(
        AppEventSender::new(tx),
        false,
        false,
        AuthCredentialsStoreMode::File,
    );
    let area = Rect::new(0, 0, 80, 18);
    let layout = view
        .main_page()
        .layout_in_chrome(ChromeMode::ContentOnly, area)
        .expect("layout");

    assert!(view.content_only_mut().handle_mouse_event_direct(
        mouse_left_click(layout.body.x + 1, layout.body.y),
        area,
    ));
    assert_eq!(view.main_state.selected_idx, Some(0));
    assert!(view.auto_switch_enabled);
    match rx.recv().expect("auto-switch event") {
        AppEvent::SetAutoSwitchAccountsOnRateLimit(enabled) => assert!(enabled),
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn content_only_body_geometry_differs_from_framed_for_mouse_routing() {
    let (tx, rx) = channel();
    let mut view = AccountSwitchSettingsView::new(
        AppEventSender::new(tx),
        false,
        false,
        AuthCredentialsStoreMode::File,
    );
    // Force a non-default selection so selection changes are observable.
    view.main_state.selected_idx = Some(1);

    let area = Rect::new(0, 0, 80, 18);
    let layout = view
        .main_page()
        .layout_in_chrome(ChromeMode::ContentOnly, area)
        .expect("layout");
    let click = mouse_left_click(layout.body.x + 1, layout.body.y);

    // Same coordinates should be outside the framed body, so nothing happens.
    assert!(!view.framed_mut().handle_mouse_event_direct(click, area));
    assert_eq!(view.main_state.selected_idx, Some(1));
    assert!(!view.auto_switch_enabled);
    assert!(rx.try_recv().is_err());

    // Content-only routing should handle and toggle row 0.
    assert!(view.content_only_mut().handle_mouse_event_direct(click, area));
    assert_eq!(view.main_state.selected_idx, Some(0));
    assert!(view.auto_switch_enabled);
    match rx.recv().expect("auto-switch event") {
        AppEvent::SetAutoSwitchAccountsOnRateLimit(enabled) => assert!(enabled),
        other => panic!("unexpected event: {other:?}"),
    }
}

