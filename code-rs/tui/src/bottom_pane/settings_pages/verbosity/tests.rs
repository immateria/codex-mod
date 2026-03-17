use super::*;
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
fn mouse_click_selects_expected_verbosity_row() {
    let (tx, _rx) = channel();
    let mut view = VerbositySelectionView::new(TextVerbosity::Low, AppEventSender::new(tx));
    let area = Rect::new(0, 0, 50, 9);
    let layout = view
        .page()
        .layout_in_chrome(crate::bottom_pane::chrome::ChromeMode::Framed, area)
        .expect("layout");

    assert!(view.handle_mouse_event_direct(
        mouse_left_click(layout.body.x + 1, layout.body.y + 1),
        area,
    ));
    assert_eq!(view.selected_verbosity(), TextVerbosity::Medium);
    assert!(view.is_complete);
}
