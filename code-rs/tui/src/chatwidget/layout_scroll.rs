//! Layout computation and scrolling/HUD helpers for ChatWidget.

use super::ChatWidget;
use crate::height_manager::HeightEvent;
use ratatui::layout::Rect;

/// Common epilogue after a user-initiated scroll operation: flash the
/// scrollbar, sync virtualization, request a redraw, record the event
/// for the height manager, show the nav hint, and track the delta.
fn scroll_epilogue(chat: &mut ChatWidget<'_>, before: u16) {
    flash_scrollbar(chat);
    chat.sync_history_virtualization();
    chat.app_event_tx
        .send(crate::app_event::AppEvent::RequestRedraw);
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::UserScroll);
    chat.maybe_show_history_nav_hint_on_first_scroll();
    chat.perf_track_scroll_delta(before, chat.layout.scroll_offset.get());
}

pub(super) fn jump_to_history_index(chat: &mut ChatWidget<'_>, idx: usize) {
    let max_scroll = chat.layout.last_max_scroll.get();
    if max_scroll == 0 {
        return;
    }

    let before = chat.layout.scroll_offset.get();
    let scroll_from_top = chat
        .history_render
        .prefix_sums
        .borrow()
        .get(idx)
        .copied()
        .unwrap_or(max_scroll)
        .min(max_scroll);
    let new_offset = max_scroll.saturating_sub(scroll_from_top);
    if new_offset == before {
        return;
    }

    chat.layout.scroll_offset.set(new_offset);
    chat.bottom_pane.set_compact_compose(new_offset > 0);
    scroll_epilogue(chat, before);
}

pub(super) fn autoscroll_if_near_bottom(chat: &mut ChatWidget<'_>) {
    if chat.layout.scroll_offset.get() == 0 {
        let before = chat.layout.scroll_offset.get();
        chat.layout.scroll_offset.set(0);
        chat.bottom_pane.set_compact_compose(false);
        chat.height_manager
            .borrow_mut()
            .record_event(HeightEvent::ComposerModeChange);
        chat.perf_track_scroll_delta(before, chat.layout.scroll_offset.get());
        chat.sync_history_virtualization();
    }
}

pub(super) fn page_up(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset.get();
    let step = chat.layout.last_history_viewport_height.get().max(1);
    let new_offset = before
        .saturating_add(step)
        .min(chat.layout.last_max_scroll.get());
    if new_offset == before {
        return;
    }
    chat.layout.scroll_offset.set(new_offset);
    chat.bottom_pane.set_compact_compose(true);
    scroll_epilogue(chat, before);
}

pub(super) fn line_up(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset.get();
    let new_offset = before
        .saturating_add(1)
        .min(chat.layout.last_max_scroll.get());
    if new_offset == before {
        return;
    }
    chat.layout.scroll_offset.set(new_offset);
    chat.bottom_pane.set_compact_compose(true);
    scroll_epilogue(chat, before);
}

pub(super) fn line_down(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset.get();
    if before == 0 {
        return;
    }
    let new_offset = before.saturating_sub(1);
    chat.layout.scroll_offset.set(new_offset);
    if new_offset == 0 {
        chat.bottom_pane.set_compact_compose(false);
    }
    scroll_epilogue(chat, before);
}

pub(super) fn page_down(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset.get();
    if before == 0 {
        return;
    }
    let step = chat.layout.last_history_viewport_height.get().max(1);
    if before > step {
        chat.layout
            .scroll_offset
            .set(before.saturating_sub(step));
    } else {
        chat.layout.scroll_offset.set(0);
        chat.bottom_pane.set_compact_compose(false);
    }
    scroll_epilogue(chat, before);
}

pub(super) fn mouse_scroll(chat: &mut ChatWidget<'_>, up: bool) {
    let before = chat.layout.scroll_offset.get();
    if up {
        let new_offset = before
            .saturating_add(3)
            .min(chat.layout.last_max_scroll.get());
        if new_offset == before {
            return;
        }
        chat.layout.scroll_offset.set(new_offset);
        chat.bottom_pane.set_compact_compose(true);
    } else {
        if before == 0 {
            return;
        }
        if before >= 3 {
            chat.layout
                .scroll_offset
                .set(before.saturating_sub(3));
        } else {
            chat.layout.scroll_offset.set(0);
        }
        if chat.layout.scroll_offset.get() == 0 {
            chat.bottom_pane.set_compact_compose(false);
        }
    }
    scroll_epilogue(chat, before);
}

pub(super) fn flash_scrollbar(chat: &ChatWidget<'_>) {
    use std::time::{Duration, Instant};
    let until = Instant::now() + Duration::from_millis(1200);
    chat.layout.scrollbar_visible_until.set(Some(until));
    let tx = chat.app_event_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1300)).await;
        tx.send(crate::app_event::AppEvent::RequestRedraw);
    });
}

/// Jump to the very top of the history (oldest content).
pub(super) fn to_top(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset.get();
    let max = chat.layout.last_max_scroll.get();
    if before == max {
        return;
    }
    chat.layout.scroll_offset.set(max);
    chat.bottom_pane.set_compact_compose(true);
    scroll_epilogue(chat, before);
}

/// Jump to the very bottom of the history (latest content).
pub(super) fn to_bottom(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset.get();
    if before == 0 {
        return;
    }
    chat.layout.scroll_offset.set(0);
    chat.bottom_pane.set_compact_compose(false);
    scroll_epilogue(chat, before);
}

pub(super) fn layout_areas(chat: &ChatWidget<'_>, area: Rect) -> Vec<Rect> {
    let bottom_desired = chat.bottom_pane.desired_height(area.width);
    let font_cell = chat.measured_font_size();
    let mut hm = chat.height_manager.borrow_mut();
    hm.begin_frame(
        area,
        false,
        bottom_desired,
        font_cell,
        None,
        // Status bar rows are configurable.
        chat.status_bar_height_rows(),
    )
}
