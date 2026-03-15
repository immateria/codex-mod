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
fn planning_mouse_hit_targets_content_row() {
    let (tx, _rx) = channel();
    let mut view = PlanningSettingsView::new(
        false,
        "gpt-5.3-codex".to_string(),
        ReasoningEffort::Medium,
        AppEventSender::new(tx),
    );
    let area = Rect::new(0, 0, 40, 8);
    let page = view.page();
    let layout = page.content_only().layout(area).expect("layout");
    assert_eq!(
        view.row_at_position(layout.body, layout.body.x, layout.body.y),
        Some(PlanningRow::CustomModel)
    );
    assert!(view
        .content_only_mut()
        .handle_mouse_event_direct(left_click(layout.body.x, layout.body.y), area));
}

