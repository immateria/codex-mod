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
    let layout = page
        .layout_in_chrome(crate::bottom_pane::chrome::ChromeMode::ContentOnly, area)
        .expect("layout");
    let rows = view.menu_rows();
    assert_eq!(
        crate::bottom_pane::settings_ui::menu_page::SettingsMenuPage::selection_menu_id_in_body(
            layout.body,
            // Hit-testing requires the pointer to be over the visible text (not row padding).
            layout.body.x.saturating_add(2),
            layout.body.y,
            0,
            &rows,
        ),
        Some(PlanningRow::CustomModel)
    );
    assert!(view
        .content_only_mut()
        .handle_mouse_event_direct(
            left_click(layout.body.x.saturating_add(2), layout.body.y),
            area,
        ));
}
