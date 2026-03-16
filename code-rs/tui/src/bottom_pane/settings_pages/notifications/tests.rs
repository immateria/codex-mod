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
    let click = mouse_left_click(content_layout.body.x, content_layout.body.y);

    view.state.selected_idx = Some(1);
    assert!(!view.handle_mouse_event_direct_framed(click, area));
    assert_eq!(view.state.selected_idx, Some(1));
    assert!(matches!(view.mode, NotificationsMode::Toggle { enabled: false }));

    view.state.selected_idx = Some(1);
    assert!(view.handle_mouse_event_direct_content_only(click, area));
    assert_eq!(view.state.selected_idx, Some(0));
    assert!(matches!(view.mode, NotificationsMode::Toggle { enabled: true }));
}
