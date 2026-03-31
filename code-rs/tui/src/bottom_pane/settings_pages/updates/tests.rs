use super::*;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::chrome::ChromeMode;
use crate::chatwidget::BackgroundOrderTicket;
use crate::colors;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

fn mouse_left_click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn make_view() -> UpdateSettingsView {
    let (tx, _rx) = channel();
    UpdateSettingsView::new(UpdateSettingsInit {
        app_event_tx: AppEventSender::new(tx),
        ticket: BackgroundOrderTicket::test_ticket(1),
        current_version: "1.0.0".to_string(),
        auto_enabled: false,
        command: None,
        command_display: None,
        manual_instructions: None,
        shared: Arc::new(Mutex::new(UpdateSharedState::default())),
    })
}

#[test]
fn automatic_upgrades_row_uses_shared_toggle_word() {
    let row = UpdateSettingsView::auto_upgrade_row(false);
    let value = row.value.expect("toggle value");
    assert_eq!(value.text.as_ref(), "disabled");
    assert_eq!(value.style.fg, Some(colors::text_dim()));
}

#[test]
fn content_only_mouse_uses_content_geometry_not_framed_geometry() {
    let mut view = make_view();
    let area = Rect::new(0, 0, 50, 8);
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

    view.state.selected_idx = Some(1);
    assert!(view.handle_mouse_event_direct_content_only(click, area));
    assert_eq!(view.state.selected_idx, Some(0));
}

#[test]
fn desired_height_constants_match_render_lines() {
    let view = make_view();
    assert_eq!(UpdateSettingsView::HEADER_LINE_COUNT, view.header_lines().len());
    assert_eq!(UpdateSettingsView::FOOTER_LINE_COUNT, UpdateSettingsView::footer_lines().len());
}
