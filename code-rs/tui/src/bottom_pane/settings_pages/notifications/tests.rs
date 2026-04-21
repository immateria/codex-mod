use super::*;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::chrome::ChromeMode;
use crate::chatwidget::BackgroundOrderTicket;
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

fn make_view(enabled: bool) -> NotificationsSettingsView {
    let (tx, _rx) = channel();
    NotificationsSettingsView::new(
        NotificationsMode::Toggle { enabled },
        false,
        AppEventSender::new(tx),
        BackgroundOrderTicket::test_ticket(1),
    )
}

#[test]
fn content_only_mouse_uses_content_geometry_not_framed_geometry() {
    let mut view = make_view(false);
    let area = Rect::new(0, 0, 50, 9);
    let content_layout = view
        .page()
        .layout_in_chrome(ChromeMode::ContentOnly, area)
        .expect("layout");
    // Hit-testing requires the pointer to be over the visible text (not row padding).
    let click = mouse_left_click(
        content_layout.body.x.saturating_add(2),
        content_layout.body.y,
    );

    view.state.selected_idx = Some(1);
    assert!(!view.handle_mouse_event_direct_framed(click, area));
    assert_eq!(view.state.selected_idx, Some(1));
    assert!(matches!(view.mode, NotificationsMode::Toggle { enabled: false }));

    view.state.selected_idx = Some(1);
    assert!(view.handle_mouse_event_direct_content_only(click, area));
    assert_eq!(view.state.selected_idx, Some(0));
    assert!(matches!(view.mode, NotificationsMode::Toggle { enabled: true }));
}

#[test]
fn space_on_prevent_sleep_row_sends_update_event() {
    let (tx, rx) = channel();
    let mut view = NotificationsSettingsView::new(
        NotificationsMode::Toggle { enabled: false },
        false,
        AppEventSender::new(tx),
        BackgroundOrderTicket::test_ticket(2),
    );

    view.state.selected_idx = Some(1);
    assert!(view.handle_key_event_direct(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        KeyModifiers::NONE,
    )));

    match rx.try_recv().expect("UpdatePreventIdleSleep") {
        crate::app_event::AppEvent::UpdatePreventIdleSleep(enabled) => assert!(enabled),
        other => panic!("unexpected app event: {other:?}"),
    }
}
