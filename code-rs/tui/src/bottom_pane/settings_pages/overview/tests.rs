use super::*;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::sync::mpsc::channel;

fn left_click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn mouse_activation_opens_expected_section() {
    let (tx, rx) = channel();
    let mut view = SettingsOverviewView::new(
        vec![
            (crate::bottom_pane::SettingsSection::Model, None),
            (
                crate::bottom_pane::SettingsSection::Theme,
                Some("dark".to_string()),
            ),
            (crate::bottom_pane::SettingsSection::Interface, None),
        ],
        crate::bottom_pane::SettingsSection::Model,
        AppEventSender::new(tx),
    );
    let area = Rect::new(0, 0, 50, 12);
    let layout = view.page().framed().layout(area).expect("layout");

    assert!(view.handle_mouse_event_direct(
        left_click(layout.body.x, layout.body.y.saturating_add(1)),
        area,
    ));

    match rx.recv().expect("open settings") {
        crate::app_event::AppEvent::OpenSettings { section } => {
            assert_eq!(section, Some(crate::bottom_pane::SettingsSection::Theme))
        }
        other => panic!("unexpected event: {other:?}"),
    }
    assert!(view.is_complete);
}

