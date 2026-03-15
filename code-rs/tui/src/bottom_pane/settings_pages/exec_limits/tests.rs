use super::*;

use std::sync::mpsc;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

#[test]
fn mouse_click_apply_emits_set_exec_limits_event() {
    let (tx, rx) = mpsc::channel();
    let app_event_tx = AppEventSender::new(tx);
    let settings = code_core::config::ExecLimitsToml::default();
    let mut view = ExecLimitsSettingsView::new(settings, app_event_tx);

    let header_height = u16::try_from(view.render_header_lines().len()).unwrap_or(u16::MAX);
    let footer_height =
        1u16.saturating_add(u16::try_from(view.render_footer_lines().len()).unwrap_or(u16::MAX));
    let rows_len = u16::try_from(ExecLimitsSettingsView::build_rows().len()).unwrap_or(u16::MAX);
    let required_inner_height = header_height
        .saturating_add(footer_height)
        .saturating_add(rows_len);

    let area = Rect {
        x: 0,
        y: 0,
        width: 80,
        // Ensure all rows are visible regardless of OS-specific header lines.
        height: required_inner_height.saturating_add(2),
    };

    let inner = Block::default().borders(Borders::ALL).inner(area);
    let apply_idx = ExecLimitsSettingsView::build_rows()
        .iter()
        .position(|row| *row == RowKind::Apply)
        .expect("apply row");
    let apply_idx = u16::try_from(apply_idx).unwrap_or(u16::MAX);

    let mouse_event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: inner.x.saturating_add(2),
        row: inner.y.saturating_add(header_height).saturating_add(apply_idx),
        modifiers: KeyModifiers::NONE,
    };

    assert!(view.handle_mouse_event_direct_framed(mouse_event, area));
    match rx.try_recv() {
        Ok(AppEvent::SetExecLimitsSettings(_)) => {}
        other => panic!("expected SetExecLimitsSettings event, got: {other:?}"),
    }
}

